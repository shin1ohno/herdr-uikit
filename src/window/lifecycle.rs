//! A window's life: built before it has a scene, attached when one is free,
//! shown and hidden as its scene moves between the foreground and the
//! background, and torn down when its scene disconnects or GPUI closes it.

#![allow(
    unsafe_code,
    reason = "retains the renderer's CAMetalLayer from the raw pointer the renderer hands out"
)]

use std::{
    cell::RefCell,
    mem,
    ptr::NonNull,
    rc::{Rc, Weak},
};

use gpui::{
    AnyWindowHandle, Bounds, ForegroundExecutor, WindowBackgroundAppearance, WindowParams,
    WindowVisibility,
};
use objc2::{MainThreadMarker, MainThreadOnly, rc::Retained, sel};
use objc2_foundation::{NSNotification, NSObjectProtocol, NSString};
use objc2_quartz_core::CALayer;
use objc2_ui_kit::{
    UISceneActivationState, UIViewAutoresizing, UIViewController, UIWindow, UIWindowScene,
};

use super::{
    convert,
    input_target::InputTarget,
    keyboard::{Dismissal, Event as KeyboardEvent, ResignReason, on_keyboard_event},
    pointer, scene,
    state::{Callbacks, State, Window, call},
    text_input::TextInputView,
    view::GpuiView,
};
use crate::{layout_direction, renderer::Renderer};

/// The scale to assume before the window is on a screen: every iPad that runs
/// iPadOS 17 has a 2x display.
const DEFAULT_SCALE_FACTOR: f32 = 2.0;

impl Window {
    /// Builds a window that is not yet in a scene: its view, view controller,
    /// text input view, and pointer recognizers.
    pub(super) fn new(
        handle: AnyWindowHandle,
        params: &WindowParams,
        executor: ForegroundExecutor,
        renderer: Renderer,
        mtm: MainThreadMarker,
    ) -> Rc<Self> {
        let scale_factor = scene::scale_hint(mtm).unwrap_or(DEFAULT_SCALE_FACTOR);
        let title = params
            .titlebar
            .as_ref()
            .and_then(|titlebar| titlebar.title.as_ref())
            .map(ToString::to_string)
            .unwrap_or_default();
        let bounds = Bounds {
            origin: gpui::Point::default(),
            size: params.bounds.size,
        };
        let metal_layer = metal_layer(&renderer);
        Rc::new_cyclic(|window: &Weak<Self>| {
            let view = GpuiView::new(mtm, window.clone(), convert::cg_rect(bounds));
            if let Some(layer) = &metal_layer {
                view.layer().addSublayer(layer);
            }
            pointer::install_recognizers(&view, mtm);
            let pointer = pointer::Pointer::new(&view, window.clone(), mtm);
            let edit_menu = super::edit_menu::EditMenu::new(&view, window.clone(), mtm);
            view.observe_keyboard();

            let controller = UIViewController::new(mtm);
            controller.setView(Some(&view));

            let text_input = TextInputView::new(mtm, InputTarget::new(window.clone()));
            let text_view = text_input.view();
            // Accessibility omits zero-sized native views even when their
            // accessibilityFrame is nonzero. Keep the client in the visible
            // hierarchy; its hit-test override leaves all touches to GPUI.
            text_view.setFrame(view.bounds());
            text_view.setAutoresizingMask(
                UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
            );
            view.addSubview(&text_view);

            Self {
                callbacks: Callbacks::default(),
                controller,
                display_link: RefCell::new(None),
                executor,
                handle,
                metal_layer,
                mtm,
                pointer,
                edit_menu,
                renderer: RefCell::new(renderer),
                state: RefCell::new(State::new(bounds, scale_factor, title)),
                text_input,
                view,
            }
        })
    }

    /// Puts the window in a scene: a connected one that hosts no window yet,
    /// else a new one the system is asked for. With `focus`, an idle scene in
    /// the background comes forward; a new scene comes forward on its own.
    pub(super) fn place(self: &Rc<Self>, focus: bool) {
        self.state.borrow_mut().placed = true;
        match scene::claim_idle_scene(self) {
            Some(scene) => {
                let foreground =
                    scene.activationState() == UISceneActivationState::ForegroundActive;
                self.attach(scene);
                if focus && !foreground {
                    self.activate_scene();
                }
            }
            None => scene::wait_for_scene(self, self.mtm),
        }
    }

    /// `PlatformWindow::activate`: brings the window's scene forward, or
    /// shows a window opened hidden by putting it in a scene.
    pub(super) fn activate(self: &Rc<Self>) {
        let placed = {
            let state = self.state.borrow();
            if state.closed {
                return;
            }
            state.placed
        };
        if placed {
            self.activate_scene();
        } else {
            self.place(true);
        }
    }

    /// Shows the window in `scene`: a `UIWindow` for the scene whose root
    /// view controller hosts the drawing view.
    pub(super) fn attach(self: &Rc<Self>, scene: Retained<UIWindowScene>) {
        let ui_window = UIWindow::initWithWindowScene(UIWindow::alloc(self.mtm), &scene);
        ui_window.setRootViewController(Some(&self.controller));
        let title = {
            let mut state = self.state.borrow_mut();
            state.scene = Some(scene.clone());
            state.ui_window = Some(ui_window.clone());
            state.touches.clear();
            state.title.clone()
        };
        if !title.is_empty() {
            scene.setTitle(Some(&NSString::from_str(&title)));
        }
        self.view.observe_scene(&scene);
        self.apply_layout_direction(layout_direction::current(self.mtm));
        ui_window.makeKeyAndVisible();

        let activation = scene.activationState();
        self.set_visibility(visibility(activation));
        // `makeKeyAndVisible` sized the view; take its geometry and traits now
        // rather than at the next layout pass, so GPUI's first frame has them.
        self.layout();
        self.set_active(activation == UISceneActivationState::ForegroundActive);
        self.force_frame();
    }

    /// The scene became active (focused) or stopped being so.
    pub(super) fn set_active(self: &Rc<Self>, active: bool) {
        if active {
            self.set_visibility(WindowVisibility::Visible);
        }
        let changed = {
            let mut state = self.state.borrow_mut();
            if state.closed {
                return;
            }
            if !active {
                state.keyboard.expect_hide(ResignReason::Inactive);
            }
            mem::replace(&mut state.is_active, active) != active
        };
        if !active {
            self.edit_menu.revoke(crate::RevokeReason::Inactive);
        }
        if changed {
            call(&self.callbacks.active_status, |callback| callback(active));
        }
        if active {
            self.defer(Self::update_first_responder);
        }
    }

    /// `UISceneWillEnterForegroundNotification` or
    /// `UISceneDidEnterBackgroundNotification` for the window's scene.
    pub(super) fn scene_visibility_changed(&self, visible: bool) {
        if !visible {
            self.edit_menu.revoke(crate::RevokeReason::Inactive);
            self.state
                .borrow_mut()
                .keyboard
                .expect_hide(ResignReason::Inactive);
        }
        self.set_visibility(if visible {
            WindowVisibility::Visible
        } else {
            WindowVisibility::Hidden
        });
    }

    fn set_visibility(&self, visibility: WindowVisibility) {
        let changed = {
            let mut state = self.state.borrow_mut();
            !state.closed && mem::replace(&mut state.visibility, visibility) != visibility
        };
        if !changed {
            return;
        }
        self.pointer.set_visible(visibility.is_visible());
        if visibility.is_visible() {
            // The system may have discarded the drawable in the background.
            self.force_frame();
        } else {
            self.pointer_left();
            self.stop_display_link();
        }
        call(&self.callbacks.visibility, |callback| callback(visibility));
    }

    /// The window's scene disconnected, which closes the window: GPUI's close
    /// callback removes it, and dropping it then finds nothing left to tear
    /// down in UIKit.
    pub(super) fn scene_disconnected(&self) {
        self.set_visibility(WindowVisibility::Hidden);
        let ui_window = {
            let mut state = self.state.borrow_mut();
            state.keyboard.expect_hide(ResignReason::LeavingWindow);
            state.touches.clear();
            state.scene = None;
            state.ui_window.take()
        };
        self.view.stop_observing();
        if let Some(ui_window) = ui_window {
            ui_window.setHidden(true);
        }
        self.run_close_callback();
    }

    /// The system refused the new scene this window waited for, so it can
    /// never show. It closes as when a scene disconnects.
    pub(super) fn scene_refused(&self) {
        self.run_close_callback();
    }

    /// Runs GPUI's close callback, which removes the window; dropping it then
    /// calls `close`.
    fn run_close_callback(&self) {
        let close = self.callbacks.close.borrow_mut().take();
        if let Some(close) = close {
            close();
        }
    }

    /// GPUI dropped the window. Stops drawing at once; the UIKit teardown and
    /// the scene's destruction run on the next main-queue turn, outside the
    /// GPUI update that is dropping the window, and out of the display link
    /// callback that may be on the stack.
    pub(super) fn close(&self) {
        self.edit_menu.revoke(crate::RevokeReason::Closed);
        let ui_window = {
            let mut state = self.state.borrow_mut();
            if mem::replace(&mut state.closed, true) {
                return;
            }
            state.keyboard.expect_hide(ResignReason::LeavingWindow);
            state.input_handler = None;
            state.keyboard_animation = None;
            state.touches.clear();
            state.ui_window.take()
        };
        pointer::clear_window(self);
        self.pointer.detach();
        self.stop_display_link();
        let display_link = self.display_link.borrow_mut().take();
        if let Ok(renderer) = self.renderer.try_borrow() {
            renderer.destroy();
        }
        let scene = scene::release(self);
        self.view.stop_observing();

        let metal_layer = self.metal_layer.clone();
        let mtm = self.mtm;
        let text_input_view = self.text_input.view();
        self.executor
            .spawn(async move {
                // Released here rather than in `close`, which can run inside
                // the link's own callback when a frame closes the window.
                let _display_link = display_link;
                if text_input_view.isFirstResponder() {
                    text_input_view.resignFirstResponder();
                }
                if let Some(layer) = metal_layer {
                    layer.removeFromSuperlayer();
                }
                if let Some(ui_window) = ui_window {
                    ui_window.setHidden(true);
                    ui_window.setRootViewController(None);
                }
                // Closing a window closes its scene, as closing one does on a
                // desktop. A scene that disconnected on its own was already
                // released, so it is not asked to go twice.
                if let Some(scene) = scene {
                    scene::destroy(&scene, mtm);
                }
            })
            .detach();
    }

    /// Brings the window's scene forward, if it has one yet.
    fn activate_scene(&self) {
        let scene = self.state.borrow().scene.clone();
        if let Some(scene) = scene {
            scene::activate(&scene, self.mtm);
        }
    }

    pub(super) fn set_title(&self, title: &str) {
        let scene = {
            let mut state = self.state.borrow_mut();
            title.clone_into(&mut state.title);
            state.scene.clone()
        };
        // The scene's title names the window in the app switcher and in
        // Stage Manager.
        if let Some(scene) = scene {
            scene.setTitle(Some(&NSString::from_str(title)));
        }
    }

    pub(super) fn set_background_appearance(&self, appearance: WindowBackgroundAppearance) {
        self.state.borrow_mut().background_appearance = appearance;
        let transparent = appearance != WindowBackgroundAppearance::Opaque;
        self.renderer.borrow_mut().update_transparency(transparent);
        self.view.setOpaque(!transparent);
    }

    /// Whether the scene fills its screen. iPadOS 26 reports it directly;
    /// before that, a window as large as its screen is full screen.
    pub(super) fn is_full_screen(&self) -> bool {
        let (scene, ui_window) = {
            let state = self.state.borrow();
            (state.scene.clone(), state.ui_window.clone())
        };
        let (Some(scene), Some(ui_window)) = (scene, ui_window) else {
            return false;
        };
        if scene.respondsToSelector(sel!(isFullScreen)) {
            return scene.isFullScreen();
        }
        ui_window.bounds().size == scene.screen().bounds().size
    }

    /// GPUI's focused element started or stopped taking text.
    pub(super) fn set_text_input_focused(self: &Rc<Self>, focused: bool) {
        {
            let mut state = self.state.borrow_mut();
            tracing::debug!(
                previous = state.text_input_focused,
                focused,
                dismissal = ?state.keyboard.dismissal,
                "set_text_input_focused"
            );
            state.text_input_focused = focused;
            state.keyboard.focus_changed();
        }
        self.defer(Self::update_first_responder);
    }

    /// `PlatformWindow::show_soft_keyboard`: focuses the text input view
    /// again if the focused element takes text.
    pub(super) fn show_keyboard(self: &Rc<Self>) {
        self.state.borrow_mut().keyboard.show();
        self.defer(Self::update_first_responder);
    }

    /// `PlatformWindow::hide_soft_keyboard`: dismisses the keyboard and
    /// keeps GPUI's focus where it is.
    pub(super) fn hide_keyboard(self: &Rc<Self>) {
        self.state.borrow_mut().keyboard.hide();
        self.defer(Self::update_first_responder);
    }

    /// UIKit's dismiss key can resign the text responder or retain it while
    /// minimizing. Observe visibility independently and fence stale completions.
    pub(super) fn keyboard_visibility_changed(
        &self,
        notification: &NSNotification,
        event: KeyboardEvent,
    ) {
        let context = self.keyboard_notification_context(notification);
        let mut state = self.state.borrow_mut();
        if state.closed {
            return;
        }
        state.keyboard = on_keyboard_event(state.keyboard, event, context);
        tracing::debug!(
            ?event,
            ?context,
            keyboard = ?state.keyboard,
            "keyboard visibility changed"
        );
    }

    /// Makes the text input view first responder while the focused element
    /// takes text and the scene is active, which shows the software keyboard
    /// when no hardware keyboard is attached; otherwise makes the drawing view
    /// first responder, so hardware key presses keep arriving.
    fn update_first_responder(&self) {
        let (wants_text, attached, reason) = {
            let state = self.state.borrow();
            if state.closed {
                return;
            }
            (
                state.text_input_focused
                    && state.is_active
                    && state.keyboard.dismissal.allows_text(),
                state.ui_window.is_some(),
                if !state.is_active {
                    ResignReason::Inactive
                } else if state.keyboard.dismissal == Dismissal::Application {
                    ResignReason::Application
                } else {
                    ResignReason::FocusChange
                },
            )
        };
        if !attached {
            return;
        }
        if wants_text {
            self.text_input.activate();
        } else {
            self.text_input.deactivate();
            let switch = super::edit_menu::responder_switch::ResponderSwitch::new(self);
            self.text_input.resign(reason, &switch);
            switch.acquire(&**self.view);
            self.reconcile_keyboard_layout();
        }
    }

    /// Runs `f` on the next main-queue turn, if the window is still open.
    ///
    /// GPUI calls the window from inside its own update, and UIKit answers
    /// some calls (first responder changes, caret moves) by querying the input
    /// handler synchronously, which cannot reach the app while that update
    /// runs. Deferring puts those calls outside it.
    pub(super) fn defer(self: &Rc<Self>, f: impl FnOnce(&Self) + 'static) {
        let window = Rc::downgrade(self);
        self.executor
            .spawn(async move {
                if let Some(window) = window.upgrade()
                    && !window.state.borrow().closed
                {
                    f(&window);
                }
            })
            .detach();
    }
}

/// The renderer's `CAMetalLayer`, retained for as long as the window holds it.
fn metal_layer(renderer: &Renderer) -> Option<Retained<CALayer>> {
    let layer = NonNull::new(renderer.layer_ptr().cast::<CALayer>());
    // SAFETY: the renderer's contract is that `layer_ptr` is its
    // `CAMetalLayer`, a `CALayer` subclass, alive for the renderer's lifetime;
    // retaining it keeps it alive for the window's too.
    let layer = layer.and_then(|layer| unsafe { Retained::retain(layer.as_ptr()) });
    if layer.is_none() {
        tracing::error!("the renderer has no Metal layer; the window will not draw");
    }
    layer
}

fn visibility(activation: UISceneActivationState) -> WindowVisibility {
    if activation == UISceneActivationState::ForegroundActive
        || activation == UISceneActivationState::ForegroundInactive
    {
        WindowVisibility::Visible
    } else {
        WindowVisibility::Hidden
    }
}
