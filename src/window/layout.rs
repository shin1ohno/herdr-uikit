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

use gpui::{Bounds, PlatformDisplay, px, size};
use objc2::{msg_send, rc::Retained};
use objc2_core_foundation::{CGFloat, CGRect};
use objc2_foundation::{NSDictionary, NSNotification, NSNumber, NSString, NSValue};
use objc2_quartz_core::CATransaction;
use objc2_ui_kit::{
    NSValueUIGeometryExtensions, UICoordinateSpace, UIEdgeInsets,
    UIKeyboardAnimationDurationUserInfoKey, UIKeyboardFrameEndUserInfoKey,
    UIKeyboardIsLocalUserInfoKey, UILayoutGuide, UISceneActivationState, UIScreen,
    UITraitEnvironment, UIUserInterfaceStyle,
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
        let screen = ui_window.screen();
        self.follow_screen(&screen);
        let display_id = crate::display::IosDisplay::for_screen(screen, self.mtm).id();
        let previous_display = self.callbacks.display_id.replace(Some(display_id));
        if previous_display.is_some_and(|previous| previous != display_id) {
            call(&self.callbacks.display_changed, |callback| callback());
        }
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
        // A window that fills its screen hides the status bar; the safe area
        // read below already reflects a status bar UIKit has hidden.
        let fill_changed = self
            .controller
            .set_hides_status_bar(self.sample_fills_screen());
        let raw = self.view.safeAreaInsets();
        let title_hidden = crate::mac_titlebar::title_hidden();
        // On a Mac with the content under the hidden title bar, the band
        // UIKit reports as top safe area is the content's top row
        // (`crate::mac_titlebar`).
        let safe_area = convert::edges(UIEdgeInsets {
            top: crate::mac_titlebar::content_top_inset(title_hidden, raw.top),
            ..raw
        });
        let controls = convert::edges(crate::mac_titlebar::clear_traffic_lights(
            title_hidden,
            crate::layout_direction(),
            corner_region::sample(&self.view, raw),
        ));
        let (insets, controls_changed) = {
            let mut state = self.state.borrow_mut();
            let stored = (&state.insets.safe_area, &state.window_controls);
            if state.closed
                || (!fill_changed && !corner_region::needs_update(stored, (&safe_area, &controls)))
            {
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

    /// Whether the window fills its screen now (`screen_fill`): its bounds
    /// against its scene's screen, both in the interface's orientation.
    fn sample_fills_screen(&self) -> bool {
        let Some(ui_window) = self.view.window() else {
            return false;
        };
        let Some(scene) = ui_window.windowScene() else {
            return false;
        };
        let window = ui_window.bounds().size;
        let screen = scene.screen().bounds().size;
        super::screen_fill::fills_screen(
            (window.width, window.height),
            (screen.width, screen.height),
            crate::mac_titlebar::ios_app_on_mac(),
        )
    }

    /// The layout guide is UIKit's window-local keyboard geometry. It also
    /// catches a keyboard that was already visible before this view observed
    /// its frame notifications, and scene geometry changes with no keyboard
    /// movement. The guide follows undocked keyboards too
    /// (`followsUndockedKeyboard`), for `keyboard_placement`; a floating or
    /// undocked keyboard still makes no bottom inset beyond the safe area,
    /// as the guide gave before it followed them, except an undocked one
    /// near the screen's bottom edge (`keyboard_inset::near_bottom`).
    pub(super) fn sample_keyboard_layout(&self) {
        // SAFETY: UIView returns its UIKeyboardLayoutGuide, a UILayoutGuide
        // subclass. These selectors exist on the minimum supported iOS 26;
        // objc2's typed subclass needs the unused UITrackingLayoutGuide feature.
        let guide: Retained<UILayoutGuide> = unsafe { msg_send![&*self.view, keyboardLayoutGuide] };
        // SAFETY: this is the UIKeyboardLayoutGuide returned immediately above.
        let uses_bottom_safe_area: bool = unsafe { msg_send![&*guide, usesBottomSafeArea] };
        if uses_bottom_safe_area {
            // SAFETY: as above; the setter takes an Objective-C BOOL.
            let _: () = unsafe { msg_send![&*guide, setUsesBottomSafeArea: false] };
        }
        // SAFETY: as above (iOS 15 selectors); the setter takes a BOOL.
        let follows: bool = unsafe { msg_send![&*guide, followsUndockedKeyboard] };
        if !follows {
            // SAFETY: as above.
            let _: () = unsafe { msg_send![&*guide, setFollowsUndockedKeyboard: true] };
        }
        let keyboard = guide.layoutFrame();
        let (placement, near_bottom) = self.guide_placement(keyboard);
        let target = match placement {
            // Split, or just unsplit, at the bottom: covers the window's
            // last lines as a docked keyboard does.
            super::KeyboardPlacement::Undocked if near_bottom => {
                keyboard_geometry::near_bottom_inset(self.view.bounds(), keyboard) as f32
            }
            super::KeyboardPlacement::Floating | super::KeyboardPlacement::Undocked => {
                self.view.safeAreaInsets().bottom as f32
            }
            _ => keyboard_geometry::bottom_inset(self.view.bounds(), keyboard) as f32,
        };
        let target = target.max(self.accessory_inset(target));
        self.keyboard_guided(placement);
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

    /// The bottom inset the consumer's accessory row gives this view while
    /// UIKit shows it, with the keyboard alone giving `keyboard_inset`
    /// (`keyboard_inset::accessory_inset`); 0 without a row or a screen.
    fn accessory_inset(&self, keyboard_inset: f32) -> f32 {
        let ui_window = self.state.borrow().ui_window.clone();
        let (Some(row), Some(ui_window)) = (self.text_input.accessory_screen_frame(), ui_window)
        else {
            self.state.borrow_mut().keyboard_row_at_bottom = false;
            return 0.0;
        };
        let screen = ui_window.screen();
        let view = self
            .view
            .convertRect_toCoordinateSpace(self.view.bounds(), &screen.coordinateSpace());
        let inset = super::keyboard_inset::accessory_inset(
            keyboard_geometry::tuple(screen.bounds()),
            keyboard_geometry::tuple(view),
            row,
            f64::from(keyboard_inset),
        );
        let at_bottom = super::keyboard_inset::row_alone_at_bottom(
            keyboard_geometry::tuple(screen.bounds()),
            row,
        );
        {
            let mut state = self.state.borrow_mut();
            state.keyboard_sample.accessory = row;
            state.keyboard_row_at_bottom = at_bottom;
        }
        inset as f32
    }

    /// Where the guide's `keyboard` (in this view's coordinates) is on the
    /// window's screen, and whether it is an undocked keyboard near the
    /// screen's bottom edge (`keyboard_inset::near_bottom`); `Hidden`
    /// before the window has a screen.
    fn guide_placement(&self, keyboard: CGRect) -> (super::KeyboardPlacement, bool) {
        let ui_window = self.state.borrow().ui_window.clone();
        let Some(ui_window) = ui_window else {
            return (super::KeyboardPlacement::Hidden, false);
        };
        let screen = ui_window.screen();
        let rect = self
            .view
            .convertRect_toCoordinateSpace(keyboard, &screen.coordinateSpace());
        let bounds = screen.bounds();
        let tuple = |rect: CGRect| {
            (
                rect.origin.x,
                rect.origin.y,
                rect.size.width,
                rect.size.height,
            )
        };
        self.state.borrow_mut().keyboard_sample.guide = tuple(rect);
        let safe_bottom = self.view.safeAreaInsets().bottom;
        (
            super::keyboard_placement::guide_placement(tuple(bounds), tuple(rect), safe_bottom),
            super::keyboard_inset::near_bottom(tuple(bounds), tuple(rect)),
        )
    }

    /// Records where the keyboard's frame notification puts it, and reports
    /// the placement with the guide's (`keyboard_placement::combine`).
    fn keyboard_placed(&self, screen: CGRect, keyboard: CGRect) {
        let tuple = |rect: CGRect| {
            (
                rect.origin.x,
                rect.origin.y,
                rect.size.width,
                rect.size.height,
            )
        };
        // The accessory row alone (a floating keyboard's) leaves the
        // placement to the guide.
        let row_height = self.text_input.accessory_height();
        let notified = if super::keyboard_placement::accessory_alone(
            tuple(screen),
            tuple(keyboard),
            row_height,
        ) {
            super::KeyboardPlacement::Hidden
        } else {
            super::keyboard_placement::placement(tuple(screen), tuple(keyboard))
        };
        {
            let mut state = self.state.borrow_mut();
            if state.closed {
                return;
            }
            state.keyboard_notified = notified;
            state.keyboard_sample.end = tuple(keyboard);
        }
        tracing::debug!(?notified, ?keyboard, "keyboard frame placement");
        self.report_keyboard_placement();
        // UIKit moves the guide after this notification; lay out on the
        // next turn so the guide's placement is read again.
        let view = self.view.clone();
        self.executor
            .spawn(async move {
                if let Some(window) = view.owner() {
                    window.reconcile_keyboard_layout();
                }
            })
            .detach();
    }

    /// Records the guide's placement from a layout pass and reports.
    fn keyboard_guided(&self, guide: super::KeyboardPlacement) {
        {
            let mut state = self.state.borrow_mut();
            if state.closed {
                return;
            }
            state.keyboard_guide = guide;
        }
        self.report_keyboard_placement();
    }

    /// Combines the last notification with the guide. Only the placement
    /// can change while the inset stays the same (a floating keyboard shows
    /// or hides), so a change draws a frame of its own.
    fn report_keyboard_placement(&self) {
        let (placement, notified, guide, changed) = {
            let mut state = self.state.borrow_mut();
            if state.closed {
                return;
            }
            let (notified, guide) = (state.keyboard_notified, state.keyboard_guide);
            let placement = super::keyboard_placement::combine(notified, guide);
            // The accessory row alone on the screen's bottom edge: the
            // keyboard floats, though the last notification was a docked one.
            let placement =
                if state.keyboard_row_at_bottom && placement == super::KeyboardPlacement::Docked {
                    super::KeyboardPlacement::Floating
                } else {
                    placement
                };
            let changed = std::mem::replace(&mut state.keyboard_placement, placement) != placement;
            (placement, notified, guide, changed)
        };
        if changed {
            tracing::debug!(?placement, ?notified, ?guide, "keyboard placement changed");
            self.force_frame();
        }
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
        let previous_end = {
            let mut state = self.state.borrow_mut();
            state.keyboard_sample.notifications += 1;
            state.keyboard_sample.relevant += u32::from(relevant);
            state.keyboard_sample.end
        };
        // A split keyboard's frame leaves the screen for a moment and comes
        // back; the inset waits so the window does not shrink and grow.
        let hold = relevant
            && super::keyboard_inset::holds_inset(
                keyboard_geometry::tuple(screen.bounds()),
                previous_end,
                keyboard_geometry::tuple(end_frame),
            );
        if relevant {
            self.keyboard_placed(screen.bounds(), end_frame);
        }
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
            hold,
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
                    // A held inset stays at `current` until the wait ends
                    // (`InsetAnimation::sample` before its start), unless a
                    // newer notification replaces the animation first.
                    let wait = if hold {
                        Duration::from_secs_f64(super::keyboard_inset::OFF_SCREEN_HOLD_SECS)
                    } else {
                        Duration::ZERO
                    };
                    state.keyboard_animation = Some(InsetAnimation::new(
                        current,
                        target,
                        Instant::now() + wait,
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
            user_dismissable: !crate::mac_titlebar::ios_app_on_mac(),
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
