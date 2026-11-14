//! The bookkeeping between UIKit's scenes and GPUI's windows (herdr-gpui's
//! docs/ios.md, "Scenes and windows"). UIKit decides when scenes connect and disconnect;
//! GPUI decides when windows open. A connected scene hosts at most one
//! window. A window opened with no idle scene waits for the scene the system
//! was asked for, and closes if the system refuses.
//!
//! The registry lives on the main thread, which every entry point proves with
//! a `MainThreadMarker`. Its borrow is never held while a window attaches or
//! a GPUI callback runs: both can come back here.

use std::{
    cell::RefCell,
    ptr::{self, NonNull},
    rc::{Rc, Weak},
};

use block2::RcBlock;
use dispatch2::{DispatchQueue, MainThreadBound};
use gpui::{AnyWindowHandle, Edges, Pixels};
use objc2::{MainThreadMarker, Message, rc::Retained};
use objc2_foundation::NSError;
use objc2_ui_kit::{UIApplication, UISceneSessionActivationRequest, UIWindowScene};

use super::{
    requests::{Answer, RequestId, Requests},
    state::Window,
};

thread_local! {
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());
}

#[derive(Default)]
struct Registry {
    /// Counts activations, to order windows by how recently they were active.
    activations: u64,
    /// New scenes asked for, each for a window opened before a scene was
    /// free for it.
    requests: Requests<Weak<Window>>,
    scenes: Vec<SceneEntry>,
}

struct SceneEntry {
    active: bool,
    /// Its window closed, or it answered a request whose window had, and the
    /// system was asked to destroy it, so it must not take a new window in
    /// the moment before it disconnects.
    closing: bool,
    last_activation: u64,
    scene: Retained<UIWindowScene>,
    window: Option<Weak<Window>>,
}

impl SceneEntry {
    fn is_idle(&self) -> bool {
        !self.closing
            && self
                .window
                .as_ref()
                .is_none_or(|window| window.strong_count() == 0)
    }

    fn live_window(&self) -> Option<Rc<Window>> {
        self.window.as_ref()?.upgrade()
    }
}

impl Registry {
    fn entry_mut(&mut self, scene: &UIWindowScene) -> Option<&mut SceneEntry> {
        self.scenes
            .iter_mut()
            .find(|entry| ptr::eq(&*entry.scene, scene))
    }
}

/// A scene connected. It hosts the window of the oldest request for a scene,
/// is closed if that window closed meanwhile, or, when nothing asked for it,
/// waits for launch/reopen to open a window, or closes if one already exists.
pub(crate) fn scene_connected(scene: &UIWindowScene, mtm: MainThreadMarker) {
    let has_windows = !window_stack(mtm).is_empty();
    let answer = REGISTRY.with_borrow_mut(|registry| {
        if registry.entry_mut(scene).is_some() {
            tracing::debug!("a window scene reported connecting twice");
            return Answer::Unrequested;
        }
        let answer = match registry.requests.answer() {
            Answer::Window(window) => window.upgrade().map_or(Answer::Unwanted, Answer::Window),
            Answer::Unrequested => Answer::Unrequested.with_existing_windows(has_windows),
            Answer::Unwanted => Answer::Unwanted,
        };
        registry.scenes.push(SceneEntry {
            active: false,
            closing: matches!(answer, Answer::Unwanted),
            last_activation: 0,
            scene: scene.retain(),
            window: match &answer {
                Answer::Window(window) => Some(Rc::downgrade(window)),
                Answer::Unrequested | Answer::Unwanted => None,
            },
        });
        answer
    });
    match answer {
        Answer::Window(window) => window.attach(scene.retain()),
        Answer::Unwanted => destroy_unwanted(scene, mtm),
        Answer::Unrequested => {}
    }
}

/// A scene disconnected, which closes the window it hosted.
pub(crate) fn scene_disconnected(scene: &UIWindowScene, _mtm: MainThreadMarker) {
    let window = REGISTRY.with_borrow_mut(|registry| {
        let index = registry
            .scenes
            .iter()
            .position(|entry| ptr::eq(&*entry.scene, scene))?;
        registry.scenes.remove(index).live_window()
    });
    if let Some(window) = window {
        window.scene_disconnected();
    }
}

/// A scene became active (foreground and focused) or stopped being so.
pub(crate) fn scene_activation_changed(
    scene: &UIWindowScene,
    active: bool,
    _mtm: MainThreadMarker,
) {
    let window = REGISTRY.with_borrow_mut(|registry| {
        registry.activations += 1;
        let activation = registry.activations;
        let entry = registry.entry_mut(scene)?;
        entry.active = active;
        if active {
            entry.last_activation = activation;
        }
        entry.live_window()
    });
    if let Some(window) = window {
        window.set_active(active);
    }
}

/// The scene's effective geometry changed, including its window controls.
pub(crate) fn scene_geometry_changed(scene: &UIWindowScene, _mtm: MainThreadMarker) {
    let window = REGISTRY.with_borrow(|registry| {
        registry
            .scenes
            .iter()
            .find(|entry| ptr::eq(&*entry.scene, scene))?
            .live_window()
    });
    if let Some(window) = window {
        window.view.setNeedsLayout();
    }
}

/// The window in the active scene, if any. Side by side, several scenes can
/// be active at once; the one activated last is GPUI's active window.
pub(crate) fn active_window(_mtm: MainThreadMarker) -> Option<AnyWindowHandle> {
    REGISTRY.with_borrow(|registry| {
        registry
            .scenes
            .iter()
            .filter(|entry| entry.active)
            .filter_map(|entry| Some((entry.last_activation, entry.live_window()?)))
            .max_by_key(|(activation, _)| *activation)
            .map(|(_, window)| window.handle)
    })
}

/// The cached window-control insets for a window in a connected scene.
pub(crate) fn window_control_insets(
    handle: AnyWindowHandle,
    _mtm: MainThreadMarker,
) -> Option<Edges<Pixels>> {
    let window = REGISTRY.with_borrow(|registry| {
        registry
            .scenes
            .iter()
            .filter_map(SceneEntry::live_window)
            .find(|window| window.handle == handle)
    })?;
    let state = window.state.borrow();
    (!state.closed).then_some(state.window_controls)
}

/// Windows front to back, as best UIKit reports scene order: UIKit has no
/// z-order across scenes, so active scenes come first, then the rest by how
/// recently each was active.
pub(crate) fn window_stack(_mtm: MainThreadMarker) -> Vec<AnyWindowHandle> {
    REGISTRY.with_borrow(|registry| {
        let mut windows: Vec<_> = registry
            .scenes
            .iter()
            .filter_map(|entry| {
                let window = entry.live_window()?;
                Some(((entry.active, entry.last_activation), window.handle))
            })
            .collect();
        windows.sort_by(|(a, _), (b, _)| b.cmp(a));
        windows.into_iter().map(|(_, handle)| handle).collect()
    })
}

/// The windows in connected scenes. The registry is released before the
/// caller touches them, as their views may call back here.
pub(super) fn live_windows(_mtm: MainThreadMarker) -> Vec<Rc<Window>> {
    REGISTRY.with_borrow(|registry| {
        registry
            .scenes
            .iter()
            .filter_map(SceneEntry::live_window)
            .collect()
    })
}

/// Whether a connected scene hosts no window, so a new window can open now.
pub(super) fn has_idle_scene(_mtm: MainThreadMarker) -> bool {
    REGISTRY.with_borrow(|registry| registry.scenes.iter().any(SceneEntry::is_idle))
}

/// Gives `window` a connected scene that hosts no window, the most recently
/// active first, and records it as that scene's window.
pub(super) fn claim_idle_scene(window: &Rc<Window>) -> Option<Retained<UIWindowScene>> {
    REGISTRY.with_borrow_mut(|registry| {
        let entry = registry
            .scenes
            .iter_mut()
            .filter(|entry| entry.is_idle())
            .max_by_key(|entry| (entry.active, entry.last_activation))?;
        entry.window = Some(Rc::downgrade(window));
        Some(entry.scene.clone())
    })
}

/// Asks the system for a new scene for `window`, which attaches when the
/// scene connects. The system activates a scene it creates, so the window
/// takes focus whatever GPUI asked.
pub(super) fn wait_for_scene(window: &Rc<Window>, mtm: MainThreadMarker) {
    let id = REGISTRY.with_borrow_mut(|registry| registry.requests.push(Rc::downgrade(window)));
    let request = UISceneSessionActivationRequest::request();
    UIApplication::sharedApplication(mtm)
        .activateSceneSessionForRequest_errorHandler(&request, Some(&new_scene_refused(id)));
}

/// Forgets a window GPUI closed: no longer waiting, and no longer its
/// scene's. Returns the scene it was in, if that scene is still connected,
/// and marks it closing, as the caller asks the system to destroy it.
pub(super) fn release(window: &Window) -> Option<Retained<UIWindowScene>> {
    REGISTRY.with_borrow_mut(|registry| {
        registry
            .requests
            .forget(|waiting| ptr::eq(waiting.as_ptr(), window));
        let entry = registry.scenes.iter_mut().find(|entry| {
            entry
                .window
                .as_ref()
                .is_some_and(|hosted| ptr::eq(hosted.as_ptr(), window))
        })?;
        entry.window = None;
        entry.closing = true;
        Some(entry.scene.clone())
    })
}

/// The scale of a connected scene's screen, for a window not yet in a scene.
pub(super) fn scale_hint(_mtm: MainThreadMarker) -> Option<f32> {
    let scene = REGISTRY.with_borrow(|registry| {
        let entry = registry
            .scenes
            .iter()
            .max_by_key(|entry| (entry.active, entry.last_activation))?;
        Some(entry.scene.clone())
    })?;
    Some(scene.screen().scale() as f32)
}

/// Brings `scene` to the front.
pub(super) fn activate(scene: &UIWindowScene, mtm: MainThreadMarker) {
    let request = UISceneSessionActivationRequest::requestWithSession(&scene.session());
    UIApplication::sharedApplication(mtm)
        .activateSceneSessionForRequest_errorHandler(&request, Some(&scene_request_failed()));
}

/// Asks the system to close `scene`; it disconnects once it has.
pub(super) fn destroy(scene: &UIWindowScene, mtm: MainThreadMarker) {
    UIApplication::sharedApplication(mtm).requestSceneSessionDestruction_options_errorHandler(
        &scene.session(),
        None,
        Some(&scene_request_failed()),
    );
}

/// Closes a scene that is no longer wanted. The scene is
/// still connecting, so it is asked to go on the next main-queue turn.
fn destroy_unwanted(scene: &UIWindowScene, mtm: MainThreadMarker) {
    let scene = MainThreadBound::new(scene.retain(), mtm);
    DispatchQueue::main().exec_async(move || {
        if let Some(mtm) = MainThreadMarker::new() {
            destroy(&scene.into_inner(mtm), mtm);
        }
    });
}

/// The system refused request `id`, so its window can never show: it closes,
/// as when a scene disconnects, and GPUI drops it rather than keep a window
/// that never draws or takes input.
fn scene_request_refused(id: RequestId, _mtm: MainThreadMarker) {
    let window = REGISTRY
        .with_borrow_mut(|registry| registry.requests.refuse(id))
        .and_then(|window| window.upgrade());
    if let Some(window) = window {
        window.scene_refused();
    }
}

/// Handles the system refusing request `id` for a new scene. UIKit does not
/// say which thread calls the handler, so the window is reached on the next
/// main-queue turn, which also puts it after GPUI has registered the window's
/// callbacks if UIKit refuses while `IosWindow::open` is still running.
fn new_scene_refused(id: RequestId) -> RcBlock<dyn Fn(NonNull<NSError>)> {
    RcBlock::new(move |error: NonNull<NSError>| {
        log_refusal(error);
        DispatchQueue::main().exec_async(move || {
            if let Some(mtm) = MainThreadMarker::new() {
                scene_request_refused(id, mtm);
            }
        });
    })
}

/// Logs a request about an existing scene the system refused. A scene whose
/// destruction was refused stays marked closing and takes no new window.
fn scene_request_failed() -> RcBlock<dyn Fn(NonNull<NSError>)> {
    RcBlock::new(log_refusal)
}

fn log_refusal(error: NonNull<NSError>) {
    #[allow(
        unsafe_code,
        reason = "UIKit's error handler passes the error as a raw pointer"
    )]
    // SAFETY: UIKit passes a valid NSError that lives for the call.
    let error = unsafe { error.as_ref() };
    tracing::warn!(
        error = %error.localizedDescription(),
        "the system refused a window scene request"
    );
}

/// Publish consumer edit state for precisely this live window.
pub(crate) fn set_native_edit_availability(
    handle: AnyWindowHandle,
    availability: Option<crate::NativeEditAvailability>,
    mtm: MainThreadMarker,
) -> bool {
    let window = REGISTRY.with_borrow(|registry| {
        registry
            .scenes
            .iter()
            .filter_map(SceneEntry::live_window)
            .find(|window| window.handle == handle)
    });
    let Some(window) = window else {
        return false;
    };
    let mut state = window.state.borrow_mut();
    if state.closed {
        return false;
    }
    if state.native_edit_availability != availability {
        state.native_edit_availability = availability;
        drop(state);
        window.edit_menu.availability(availability.is_some());
        objc2_ui_kit::UIMenuSystem::mainSystem(mtm).setNeedsRevalidate();
    }
    true
}

pub(super) fn menu_window(handle: AnyWindowHandle) -> Option<Rc<Window>> {
    REGISTRY.with_borrow(|registry| {
        registry
            .scenes
            .iter()
            .filter_map(SceneEntry::live_window)
            .find(|window| window.handle == handle)
    })
}

/// Resolve outside observer borrows; never hand ownership to the observer.
#[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
pub(crate) fn with_observation_registration<R>(
    handle: AnyWindowHandle,
    f: impl FnOnce(&crate::native_edit_observer::Registration) -> R,
) -> Result<R, crate::native_edit_observer::ObserverError> {
    let window = observation_window(handle)?;
    Ok(f(window.text_input.observation_registration()))
}

#[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
fn observation_window(
    handle: AnyWindowHandle,
) -> Result<Rc<Window>, crate::native_edit_observer::ObserverError> {
    use crate::native_edit_observer::ObserverError;
    let window = REGISTRY.with(|cell| {
        let registry = cell.try_borrow().map_err(|_| ObserverError::Contention)?;
        registry
            .scenes
            .iter()
            .filter_map(SceneEntry::live_window)
            .find(|window| window.handle == handle)
            .ok_or(ObserverError::ViewUnavailable)
    })?;
    let closed = window
        .state
        .try_borrow()
        .map_err(|_| ObserverError::Contention)?
        .closed;
    if closed {
        return Err(ObserverError::ViewUnavailable);
    }
    Ok(window)
}

/// Synchronous messages through the real Objective-C selector entrypoints.
#[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
#[allow(
    unsafe_code,
    reason = "fixed UIKit selectors take a nullable object sender and return void"
)]
pub(crate) fn invoke_observation_selector(
    handle: AnyWindowHandle,
    action: crate::native_edit_observer::Action,
    responder: crate::native_edit_observer::ResponderOrigin,
) -> Result<(), crate::native_edit_observer::ObserverError> {
    use crate::native_edit_observer::{Action, ObserverError, ResponderOrigin};
    let window = observation_window(handle)?;
    let view: Retained<objc2_ui_kit::UIView> = match responder {
        ResponderOrigin::Text => window.text_input.view(),
        ResponderOrigin::Drawing => Retained::into_super(window.view.clone()),
    };
    let sender: Option<&objc2::runtime::AnyObject> = None;
    // SAFETY: both responders implement these four selectors with this ABI.
    unsafe {
        match action {
            Action::Copy => {
                let (): () = objc2::msg_send![&*view, copy: sender];
            }
            Action::Cut => {
                let (): () = objc2::msg_send![&*view, cut: sender];
            }
            Action::Paste => {
                let (): () = objc2::msg_send![&*view, paste: sender];
            }
            Action::SelectAll => {
                let (): () = objc2::msg_send![&*view, selectAll: sender];
            }
            Action::Unknown => return Err(ObserverError::InvalidAction),
        }
    }
    Ok(())
}
