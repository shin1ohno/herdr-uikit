//! Geometry, traits, and insets. UIKit reports changes to the drawing view
//! (`layoutSubviews`, `traitCollectionDidChange:`, `safeAreaInsetsDidChange`,
//! and the keyboard's frame notification); each is sampled here once and
//! cached, so GPUI's getters stay consistent through a frame and never call
//! into UIKit.

#![allow(
    unsafe_code,
    reason = "reads UIKit's extern string constants, trait getters objc2 marks unsafe for \
              thread safety, and NSValue's untyped CGRect payload"
)]

use std::{
    ptr,
    time::{Duration, Instant},
};

use gpui::{Bounds, px, size};
use objc2::rc::Retained;
use objc2_core_foundation::{CGFloat, CGRect};
use objc2_foundation::{NSDictionary, NSNotification, NSNumber, NSString, NSValue};
use objc2_quartz_core::CATransaction;
use objc2_ui_kit::{
    NSValueUIGeometryExtensions, UICoordinateSpace, UIKeyboardAnimationDurationUserInfoKey,
    UIKeyboardFrameEndUserInfoKey, UIScreen, UITraitEnvironment, UIUserInterfaceStyle,
};

use super::{
    convert,
    frame::InsetAnimation,
    state::{Window, call},
};

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
        if resized || rescaled {
            call(&self.callbacks.resize, |callback| {
                callback(content_size, scale_factor)
            });
            if rescaled {
                // Glyphs and images in the atlas were rasterized for the old
                // scale, so every view must paint again.
                self.force_frame();
            }
        }
        if moved {
            call(&self.callbacks.moved, |callback| callback());
        }

        // On iPadOS 18 and later, reading traits during layout also makes
        // UIKit lay the view out again when they change.
        self.traits_changed();
        self.safe_area_changed();
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
        let safe_area = convert::edges(self.view.safeAreaInsets());
        let insets = {
            let mut state = self.state.borrow_mut();
            if state.closed || state.insets.safe_area == safe_area {
                return;
            }
            state.insets.safe_area = safe_area;
            state.insets.clone()
        };
        call(&self.callbacks.insets_changed, |callback| callback(insets));
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
        // Every window hears of every keyboard. With an external display,
        // one on the other screen covers nothing here. UIKit's headers say
        // the show and hide notifications carry the keyboard's screen and
        // this one carries none; a screen, when given, decides.
        let elsewhere = notification
            .object()
            .and_then(|object| object.downcast::<UIScreen>().ok())
            .is_some_and(|keyboard_screen| !ptr::eq(&*keyboard_screen, &*screen));
        let target = if elsewhere {
            0.0
        } else {
            // The keyboard's frame is in its screen's coordinate space.
            let keyboard = self
                .view
                .convertRect_fromCoordinateSpace(end_frame, &screen.coordinateSpace());
            convert::keyboard_overlap(self.view.bounds(), keyboard) as f32
        };

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
            }
            // The display link steps the animation.
            None => self.request_frame(),
        }
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
