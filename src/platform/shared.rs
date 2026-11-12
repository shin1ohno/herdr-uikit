//! The part of the platform UIKit's delegates reach. UIKit creates the
//! application and scene delegates itself, from class names, so they cannot
//! be handed a reference: `run` installs this state in a main-thread local
//! before entering `UIApplicationMain`, and every delegate method runs on the
//! main thread.

use std::{
    cell::{Cell, OnceCell, RefCell},
    ptr,
    rc::Rc,
};

use dispatch2::DispatchQueue;
use gpui::{Action, AppLifecyclePhase};
use objc2::{MainThreadMarker, rc::Retained};
use objc2_ui_kit::UIScene;

use super::{
    lifecycle::{Delivery, LifecycleTracker, SceneState},
    menus::Menus,
    slot::Slot,
    thermal::ThermalObserver,
};
use crate::{BackgroundTask, window};

/// GPUI's callback for a menu command's action.
pub(super) type MenuActionCallback = dyn FnMut(&dyn Action);

/// GPUI's callback answering whether a menu command's action is available.
pub(super) type MenuValidateCallback = dyn FnMut(&dyn Action) -> bool;

thread_local! {
    static CURRENT: OnceCell<Rc<Shared>> = const { OnceCell::new() };
}

/// Callbacks GPUI registered and the launch and scene state they depend on.
#[derive(Default)]
pub(super) struct Shared {
    pub(super) notification_response: Slot<dyn FnMut(gpui::SystemNotificationResponse)>,
    pub(super) notification_delegate:
        OnceCell<Retained<super::notifications::NotificationDelegate>>,
    pub(super) displays_changed: Slot<dyn FnMut()>,
    pub(super) app_lifecycle: Slot<dyn FnMut(AppLifecyclePhase)>,
    pub(super) app_menu_action: Slot<MenuActionCallback>,
    finish_launching: RefCell<Option<Box<dyn FnOnce()>>>,
    launched: Cell<bool>,
    pub(super) memory_warning: Slot<dyn FnMut()>,
    pub(super) menus: RefCell<Menus>,
    pub(super) open_urls: Slot<dyn FnMut(Vec<String>)>,
    pub(super) quit: Slot<dyn FnMut() -> bool>,
    pub(super) reopen: Slot<dyn FnMut()>,
    scenes: RefCell<LifecycleTracker>,
    /// Kept alive here because the notification center does not retain its
    /// observers.
    pub(super) thermal_observer: OnceCell<Retained<ThermalObserver>>,
    pub(super) thermal_state_change: Slot<dyn FnMut()>,
    pub(super) validate_app_menu_command: Slot<MenuValidateCallback>,
    /// iPadOS tells apps nothing when a menu opens, so this is kept but not
    /// called: `validateCommand:` also runs while UIKit matches key commands,
    /// where clearing GPUI's pending keystrokes would break multi-keystroke
    /// bindings.
    pub(super) will_open_app_menu: Slot<dyn FnMut()>,
}

/// Makes `shared` the state the delegates reach. Returns `false`, leaving the
/// first in place, when a platform already ran in this process.
pub(super) fn install(shared: Rc<Shared>) -> bool {
    CURRENT.with(|current| current.set(shared).is_ok())
}

/// The installed state, when called on the main thread after `run`.
pub(super) fn current() -> Option<Rc<Shared>> {
    CURRENT.with(|current| current.get().cloned())
}

/// Runs `work` with the state on the main queue, after the UIKit or GPUI
/// call now on the stack returns. GPUI's app callbacks borrow the app
/// mutably, so they must not run inside a call GPUI itself made, and UIKit
/// may deliver delegate calls and notifications synchronously from one.
pub(super) fn later(work: impl FnOnce(&Shared) + Send + 'static) {
    DispatchQueue::main().exec_async(move || match current() {
        Some(shared) => work(&shared),
        None => tracing::warn!("a platform callback was queued before the platform ran"),
    });
}

/// A scene's identity while it is connected.
pub(super) fn scene_key(scene: &UIScene) -> usize {
    ptr::from_ref(scene).addr()
}

impl Shared {
    pub(super) fn set_finish_launching(&self, callback: Box<dyn FnOnce()>) {
        self.finish_launching.replace(Some(callback));
    }

    /// A window scene connected and `crate::window` has seen it. The first one
    /// runs GPUI's launch callback, so the first `open_window` has a scene to
    /// take. A later one while GPUI has no window at all is the user opening
    /// the app again, which GPUI handles as a reopen, as on macOS when the
    /// Dock icon is clicked with no windows open.
    pub(super) fn scene_connected(&self, scene: usize, mtm: MainThreadMarker) {
        self.scene_changed(scene, Some(SceneState::Background));
        if !self.launched.replace(true) {
            let launch = self.finish_launching.borrow_mut().take();
            match launch {
                Some(launch) => launch(),
                None => tracing::error!("a scene connected before GPUI's launch callback was set"),
            }
            return;
        }
        if window::window_stack(mtm).is_empty() {
            later(|shared| {
                shared.reopen.call(|reopen| reopen());
            });
        }
    }

    /// Records a scene's new state, `None` once it disconnected, and reports
    /// any change of the app's phase to GPUI. Reaching the background holds a
    /// background task until GPUI's callback for it returned, so work the app
    /// starts there begins before UIKit can suspend it (`Delivery`).
    pub(super) fn scene_changed(&self, scene: usize, state: Option<SceneState>) {
        let phases = self.scenes.borrow_mut().update(scene, state);
        if phases.is_empty() {
            return;
        }
        let delivery = Delivery::new(phases, || {
            BackgroundTask::begin("Reporting the background to the app")
        });
        // A delivery dropped unreported, when the platform is gone, still
        // ends its task.
        later(move |shared| {
            delivery.report(|phase| {
                shared.app_lifecycle.call(|callback| callback(phase));
            });
        });
    }

    /// Hands URLs the system asked the app to open to GPUI.
    pub(super) fn deliver_urls(&self, urls: Vec<String>) {
        if urls.is_empty() {
            return;
        }
        later(move |shared| {
            if shared.open_urls.call(|callback| callback(urls)).is_none() {
                tracing::warn!("the system asked to open URLs before the app registered for them");
            }
        });
    }
}
