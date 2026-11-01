//! UIKit sees GPUI's focused text client as a text-entry accessibility
//! element. Its screen frame follows the caret while the underlying view
//! has visible layout bounds for native accessibility discovery. Its hit-test
//! override leaves touches to GPUI.

#![allow(
    unsafe_code,
    reason = "UIKit accessibility notification FFI is called only on the main thread"
)]

use std::ops::Range;

use gpui::{Bounds, Pixels, UTF16Selection, point, px, size};
use objc2::{DefinedClass, rc::Retained};
use objc2_core_foundation::CGRect;
use objc2_foundation::NSString;
use objc2_ui_kit::{UIAccessibilityLayoutChangedNotification, UIAccessibilityPostNotification};

use super::geometry;
use super::view_class::GPUITextInputView;

impl GPUITextInputView {
    pub(super) fn accessibility_text_entry(&self) -> bool {
        // The element must exist while UIKit acquires its responder, so AX
        // can associate keyboard focus with the custom text client then.
        self.editing_enabled()
    }

    pub(super) fn accessibility_text_entry_acquired(&self) {
        if !self.accessibility_text_entry() {
            return;
        }
        // SAFETY: this is the main-thread text element whose responder was
        // acquired. LayoutChanged accepts the element AX should now focus.
        unsafe {
            UIAccessibilityPostNotification(UIAccessibilityLayoutChangedNotification, Some(self));
        }
    }

    pub(super) fn accessibility_label(&self) -> Retained<NSString> {
        // GPUI's platform input handler exposes geometry and text, but no
        // accessibility label. The window title is already localized by the app.
        NSString::from_str(&self.ivars().target.accessibility_label())
    }

    pub(super) fn accessibility_frame(&self) -> CGRect {
        // AX can query before GPUI invalidates its IME coordinates. Read the
        // selected caret through the same guarded handler as UITextInput,
        // retaining the last reported geometry if no live handler is available.
        let bounds = self
            .with_handler(|handler| {
                let selection = handler.selected_text_range(false)?;
                handler.bounds_for_range(selection_caret(selection))
            })
            .flatten()
            .or(self.ivars().caret.get());
        let rect = caret_frame(bounds, self.ivars().target.scale_factor());
        let Some(window) = self.window() else {
            return rect;
        };
        let in_window = self.convertRect_toView(rect, None);
        window.convertRect_toWindow(in_window, None)
    }
}

fn selection_caret(selection: UTF16Selection) -> Range<usize> {
    let offset = if selection.reversed {
        selection.range.start
    } else {
        selection.range.end
    };
    offset..offset
}

fn caret_frame(bounds: Option<Bounds<Pixels>>, scale_factor: f32) -> CGRect {
    let mut bounds =
        bounds.unwrap_or_else(|| Bounds::new(point(px(0.0), px(0.0)), size(px(2.0), px(1.0))));
    if !bounds.origin.x.as_f32().is_finite() {
        bounds.origin.x = px(0.0);
    }
    if !bounds.origin.y.as_f32().is_finite() {
        bounds.origin.y = px(0.0);
    }
    if !bounds.size.height.as_f32().is_finite() || bounds.size.height < px(1.0) {
        bounds.size.height = px(1.0);
    }
    geometry::caret(bounds, scale_factor)
}

#[cfg(test)]
mod tests;
