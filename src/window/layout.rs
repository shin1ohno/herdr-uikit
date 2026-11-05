//! Geometry, traits, and insets. UIKit reports changes to the drawing view
//! (`layoutSubviews`, `traitCollectionDidChange:`, `safeAreaInsetsDidChange`,
//! and the keyboard's frame notification); each is sampled here once and
//! cached, so GPUI's getters stay consistent through a frame and never call
//! into UIKit.

#![allow(
    unsafe_code,
    reason = "reads UIKit constants, traits and NSValue's CGRect payload, and sends the \
              keyboard layout guide selectors whose generated superclass feature is not enabled"
)]

use std::{
    ptr,
    time::{Duration, Instant},
};

use gpui::{Bounds, px, size};
use objc2::{msg_send, rc::Retained};
use objc2_core_foundation::{CGFloat, CGRect};
use objc2_foundation::{NSDictionary, NSNotification, NSNumber, NSString, NSValue};
use objc2_quartz_core::CATransaction;
use objc2_ui_kit::{
    NSValueUIGeometryExtensions, UICoordinateSpace, UIKeyboardAnimationDurationUserInfoKey,
    UIKeyboardFrameEndUserInfoKey, UIKeyboardIsLocalUserInfoKey, UILayoutGuide,
    UISceneActivationState, UIScreen, UITraitEnvironment, UIUserInterfaceStyle,
};

use super::{
    convert,
    frame::InsetAnimation,
    keyboard::{Context as KeyboardContext, ResignContext},
    state::{Window, call},
};

mod corner_region;
mod keyboard_geometry;

impl Window {
    /// The view was laid out: its size, the window's position in the scene,
    /// or the screen scale may have changed. Keeps the Metal layer and its
    /// drawable matching the view, then tells GPUI.
    pub(super) fn layout(&self) {
        let ui_window = {
            let state = self.state.borrow();
            if state.closed {
                return;
            }
            state.ui_window.clone()
        };
        // Laid out outside a window (detached, or before attaching), the view
        // has no screen to size the drawable for.
        let Some(ui_window) = ui_window else {
            return;
        };
        // A scene moved to another screen lays out again; frames then follow
        // that screen's refresh rate.
        self.follow_screen(&ui_window.screen());
        let view_bounds = self.view.bounds();
        let scale_factor = self.sample_scale_factor();
        let content_size = size(
            px(view_bounds.size.width as f32),
            px(view_bounds.size.height as f32),
        );
        let origin = convert::point(ui_window.frame().origin);

        if let Some(layer) = &self.metal_layer {
            // The layer follows the view at once rather than animating there.
            CATransaction::begin();
            CATransaction::setDisableActions(true);
            layer.setFrame(view_bounds);
            layer.setContentsScale(CGFloat::from(scale_factor));
            CATransaction::commit();
        }
        if view_bounds.size.width > 0.0 && view_bounds.size.height > 0.0 {
            self.renderer
                .borrow_mut()
                .update_drawable_size(content_size.to_device_pixels(scale_factor));
        }

        let (resized, rescaled, moved) = {
            let mut state = self.state.borrow_mut();
            let resized = state.bounds.size != content_size;
            let rescaled = state.scale_factor != scale_factor;
            let moved = state.bounds.origin != origin;
            state.bounds = Bounds {
                origin,
                size: content_size,
            };
            state.scale_factor = scale_factor;
            (resized, rescaled, moved)
        };
        // The synchronous resize frame must see this layout's safe area and
        // window controls, even when only the controls changed.
        self.safe_area_changed();
        if resized || rescaled {
            call(&self.callbacks.resize, |callback| {
                callback(content_size, scale_factor)
            });
            if rescaled {
                // Glyphs and images in the atlas were rasterized for the old
                // scale, so every view must paint again.
                self.force_frame();
            }
            if resized {
                self.draw_in_layout_transaction();
            }
        }
        if moved {
            call(&self.callbacks.moved, |callback| callback());
        }

        // On iPadOS 18 and later, reading traits during layout also makes
        // UIKit lay the view out again when they change.
        self.traits_changed();
        self.sample_keyboard_layout();
    }

    /// The trait collection changed: the light or dark appearance, or the
    /// display scale when the window moved to another screen.
    pub(super) fn traits_changed(&self) {
        let appearance = convert::appearance(self.sample_interface_style());
        let scale_factor = self.sample_scale_factor();
        let (appearance_changed, rescaled) = {
            let mut state = self.state.borrow_mut();
            if state.closed {
                return;
            }
            let appearance_changed = state.appearance != appearance;
            state.appearance = appearance;
            (appearance_changed, state.scale_factor != scale_factor)
        };
        if rescaled {
            // Layout resizes the drawable and reports the new scale.
            self.view.setNeedsLayout();
        }
        if appearance_changed {
            call(&self.callbacks.appearance_changed, |callback| callback());
        }
    }

    pub(super) fn safe_area_changed(&self) {
        let raw = self.view.safeAreaInsets();
        let safe_area = convert::edges(raw);
        let controls = convert::edges(corner_region::sample(&self.view, raw));
        let (insets, controls_changed) = {
            let mut state = self.state.borrow_mut();
            let stored = (&state.insets.safe_area, &state.window_controls);
            if state.closed || !corner_region::needs_update(stored, (&safe_area, &controls)) {
                return;
            }
            let controls_changed = state.window_controls != controls;
            state.insets.safe_area = safe_area;
            state.window_controls = controls;
            (state.insets.clone(), controls_changed)
        };
        if controls_changed {
            tracing::debug!(?controls, "window controls inset changed");
        }
        call(&self.callbacks.insets_changed, |callback| callback(insets));
        self.force_frame();
    }

    /// The layout guide is UIKit's window-local keyboard geometry. It also
    /// catches a keyboard that was already visible before this view observed
    /// its frame notifications, and scene geometry changes with no keyboard
    /// movement. Offscreen and floating keyboards leave a zero-height guide.
    pub(super) fn sample_keyboard_layout(&self) {
        // SAFETY: UIView returns its UIKeyboardLayoutGuide, a UILayoutGuide
        // subclass. These selectors exist on the minimum supported iOS 17;
        // objc2's typed subclass needs the unused UITrackingLayoutGuide feature.
        let guide: Retained<UILayoutGuide> = unsafe { msg_send![&*self.view, keyboardLayoutGuide] };
        // SAFETY: this is the UIKeyboardLayoutGuide returned immediately above.
        let uses_bottom_safe_area: bool = unsafe { msg_send![&*guide, usesBottomSafeArea] };
        if uses_bottom_safe_area {
            // SAFETY: as above; the setter takes an Objective-C BOOL.
            let _: () = unsafe { msg_send![&*guide, setUsesBottomSafeArea: false] };
        }
        let keyboard = guide.layoutFrame();
        let target = keyboard_geometry::bottom_inset(self.view.bounds(), keyboard) as f32;
        let insets = {
            let mut state = self.state.borrow_mut();
            // The notification's target is animated on display-link ticks;
            // sampling UIKit's final model frame here must not jump ahead.
            if state.closed
                || state.keyboard_animation.is_some()
                || state.insets.ime.bottom == px(target)
            {
                return;
            }
            state.insets.ime.bottom = px(target);
            state.insets.clone()
        };
        tracing::debug!(
            ?keyboard,
            inset = target,
            "keyboard layout guide inset changed"
        );
        call(&self.callbacks.insets_changed, |callback| callback(insets));
        self.force_frame();
    }

    /// A first-responder or input-view transition can leave the keyboard in
    /// place, with no frame notification. Reconcile after UIKit has applied
    /// that transition, even if only GPUI's content changed in the meantime.
    pub(super) fn reconcile_keyboard_layout(&self) {
        if self.state.borrow().closed {
            return;
        }
        self.view.setNeedsLayout();
        self.view.layoutIfNeeded();
        self.sample_keyboard_layout();
        self.force_frame();
    }

    /// `UIKeyboardWillChangeFrameNotification`: the software keyboard (or a
    /// hardware keyboard's shortcuts bar) is about to move. The bottom inset
    /// follows it over the keyboard's own animation duration.
    pub(super) fn keyboard_will_change_frame(&self, notification: &NSNotification) {
        let Some(info) = notification.userInfo() else {
            return;
        };
        // SAFETY: the key is an immutable UIKit string constant.
        let Some(end_frame) = rect_value(&info, unsafe { UIKeyboardFrameEndUserInfoKey }) else {
            return;
        };
        // SAFETY: as above.
        let duration = double_value(&info, unsafe { UIKeyboardAnimationDurationUserInfoKey })
            .filter(|duration| duration.is_finite() && *duration > 0.0)
            .map(Duration::from_secs_f64);
        let ui_window = self.state.borrow().ui_window.clone();
        let Some(ui_window) = ui_window else {
            return;
        };
        let screen = ui_window.screen();
        let relevant = self
            .keyboard_notification_context(notification)
            .is_relevant();
        let screen_matches = notification
            .object()
            .and_then(|object| object.downcast::<UIScreen>().ok())
            .map(|keyboard_screen| ptr::eq(&*keyboard_screen, &*screen));
        // Compare in the screen's coordinate space. The drawing view can be
        // offset in a Stage Manager window, so its local bounds cannot be
        // compared directly with the notification's screen-space rectangle.
        let view_in_screen = self
            .view
            .convertRect_toCoordinateSpace(self.view.bounds(), &screen.coordinateSpace());
        let Some(target) =
            keyboard_geometry::screen_inset(view_in_screen, screen.bounds(), end_frame, relevant)
        else {
            // Other clients' notifications must leave our guide's inset and
            // any animation in progress untouched, rather than animate to zero.
            return;
        };
        let target = target as f32;
        tracing::debug!(
            ?end_frame,
            ?view_in_screen,
            ?screen_matches,
            relevant,
            inset = target,
            ?duration,
            "keyboard frame inset computed"
        );

        let insets = {
            let mut state = self.state.borrow_mut();
            if state.closed {
                return;
            }
            let current = f32::from(state.insets.ime.bottom);
            match duration {
                Some(duration) if state.can_draw() && current != target => {
                    state.keyboard_animation = Some(InsetAnimation::new(
                        current,
                        target,
                        Instant::now(),
                        duration,
                    ));
                    None
                }
                _ => {
                    state.keyboard_animation = None;
                    let bottom = px(target);
                    (state.insets.ime.bottom != bottom).then(|| {
                        state.insets.ime.bottom = bottom;
                        state.insets.clone()
                    })
                }
            }
        };
        match insets {
            Some(insets) => {
                call(&self.callbacks.insets_changed, |callback| callback(insets));
                self.force_frame();
            }
            // The display link steps the animation.
            None => self.request_frame(),
        }
    }

    pub(super) fn keyboard_notification_context(
        &self,
        notification: &NSNotification,
    ) -> KeyboardContext {
        let (active, scene, text_requested, ui_window) = {
            let state = self.state.borrow();
            (
                !state.closed && state.is_active,
                state.scene.clone(),
                state.text_input_focused,
                state.ui_window.clone(),
            )
        };
        // Every window hears of every keyboard. With an external display,
        // one on the other screen covers nothing here. UIKit's headers say
        // the show and hide notifications carry the keyboard's screen and
        // this one carries none. Matching screens still need a local keyboard
        // and this window's active text client.
        let screen_matches = notification
            .object()
            .and_then(|object| object.downcast::<UIScreen>().ok())
            .map(|keyboard_screen| {
                ui_window
                    .as_ref()
                    .is_some_and(|window| ptr::eq(&*keyboard_screen, &*window.screen()))
            });
        // GPUI still owns a text client after the hide key resigns UIKit's
        // responder. Dismissal controls acquisition, not notification ownership.
        // SAFETY: the key is an immutable UIKit string constant. A missing or
        // non-number locality value does not establish ownership.
        let local = notification.userInfo().is_some_and(|info| unsafe {
            bool_value(&info, UIKeyboardIsLocalUserInfoKey) == Some(true)
        });
        KeyboardContext {
            active: active
                && scene.is_some_and(|scene| {
                    scene.activationState() == UISceneActivationState::ForegroundActive
                }),
            first_responder: self.text_input.view().isFirstResponder(),
            key_window: ui_window.is_some_and(|window| window.isKeyWindow()),
            local,
            screen_matches,
            text_requested,
        }
    }

    pub(super) fn keyboard_resign_context(&self, first_responder: bool) -> ResignContext {
        let (active, focused, keyboard, scene, ui_window) = {
            let state = self.state.borrow();
            (
                !state.closed && state.is_active,
                state.text_input_focused,
                state.keyboard,
                state.scene.clone(),
                state.ui_window.clone(),
            )
        };
        let active = active
            && scene.is_some_and(|scene| {
                scene.activationState() == UISceneActivationState::ForegroundActive
            })
            && ui_window.is_some_and(|window| window.isKeyWindow());
        keyboard.resign_context(active, first_responder, focused)
    }

    /// The scale of the screen the view is on: the trait collection's once
    /// the view is in a window, else the scene's screen, else the last known.
    pub(super) fn sample_scale_factor(&self) -> f32 {
        // SAFETY: trait collections are read on the main thread, which `mtm`
        // proves this is.
        let traits = unsafe { self.view.traitCollection().displayScale() };
        if traits > 0.0 {
            return traits as f32;
        }
        let scene = self.state.borrow().scene.clone();
        match scene {
            Some(scene) => scene.screen().scale() as f32,
            None => self.state.borrow().scale_factor,
        }
    }

    fn sample_interface_style(&self) -> UIUserInterfaceStyle {
        // SAFETY: as in `sample_scale_factor`.
        unsafe { self.view.traitCollection().userInterfaceStyle() }
    }
}

/// A `CGRect` stored as an `NSValue` under `key`.
fn rect_value(info: &NSDictionary, key: &NSString) -> Option<CGRect> {
    let value: Retained<NSValue> = info.objectForKey(key)?.downcast().ok()?;
    // SAFETY: UIKit stores keyboard frames as `NSValue`s holding a CGRect.
    Some(unsafe { value.CGRectValue() })
}

/// A number stored as an `NSNumber` under `key`.
fn double_value(info: &NSDictionary, key: &NSString) -> Option<f64> {
    let number: Retained<NSNumber> = info.objectForKey(key)?.downcast().ok()?;
    Some(number.doubleValue())
}

fn bool_value(info: &NSDictionary, key: &NSString) -> Option<bool> {
    let number: Retained<NSNumber> = info.objectForKey(key)?.downcast().ok()?;
    Some(number.boolValue())
}
