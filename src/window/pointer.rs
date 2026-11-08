//! The pointer between clicks: a `UIHoverGestureRecognizer` reports a
//! trackpad, mouse, or hovering Pencil moving over the view, and a
//! `UIPanGestureRecognizer` that accepts only scroll events (no touch types)
//! reports two-finger trackpad scrolling and mouse wheels. Clicks and drags
//! are touches, handled in `touch`.

mod cursor;

pub(crate) use cursor::set_cursor_style;
pub(super) use cursor::{Pointer, clear_window};

use std::{mem, rc::Rc};

use gpui::{
    CursorStyle, MouseExitEvent, MouseMoveEvent, PlatformInput, ScrollDelta, ScrollEvent,
    TouchPhase, point, px,
};
use objc2::{MainThreadMarker, MainThreadOnly, runtime::AnyObject, sel};
use objc2_core_foundation::CGPoint;
use objc2_foundation::{NSArray, NSNumber};
use objc2_ui_kit::{
    UIGestureRecognizerState, UIHoverGestureRecognizer, UIPanGestureRecognizer, UIScrollTypeMask,
};

use super::{
    convert,
    state::{Window, call},
    view::GpuiView,
};

/// Adds the hover and scroll recognizers to `view`, targeting its
/// `handleHover:` and `handleScroll:` methods.
#[allow(
    unsafe_code,
    reason = "UIGestureRecognizer's initializer takes an untyped target and selector"
)]
pub(super) fn install_recognizers(view: &GpuiView, mtm: MainThreadMarker) {
    let target: &AnyObject = view;

    // SAFETY: `GpuiView` implements `handleHover:` taking the recognizer.
    // Recognizers do not retain their target; the view owns the recognizers
    // it is given, so the target outlives them.
    let hover = unsafe {
        UIHoverGestureRecognizer::initWithTarget_action(
            UIHoverGestureRecognizer::alloc(mtm),
            Some(target),
            Some(sel!(handleHover:)),
        )
    };
    hover.setCancelsTouchesInView(false);
    view.addGestureRecognizer(&hover);

    // SAFETY: as above, for `handleScroll:`.
    let scroll = unsafe {
        UIPanGestureRecognizer::initWithTarget_action(
            UIPanGestureRecognizer::alloc(mtm),
            Some(target),
            Some(sel!(handleScroll:)),
        )
    };
    scroll.setAllowedScrollTypesMask(UIScrollTypeMask::All);
    // No touch types, so fingers stay raw touches for GPUI's own pan
    // recognition and only scroll events reach this recognizer.
    scroll.setAllowedTouchTypes(&NSArray::<NSNumber>::from_slice(&[]));
    scroll.setCancelsTouchesInView(false);
    view.addGestureRecognizer(&scroll);
}

impl Window {
    pub(super) fn handle_hover(self: &Rc<Self>, recognizer: &UIHoverGestureRecognizer) {
        let position = convert::point(recognizer.locationInView(Some(&self.view)));
        let flags = recognizer.modifierFlags();
        let modifiers = convert::modifiers(flags);
        let ended = match recognizer.state() {
            UIGestureRecognizerState::Began | UIGestureRecognizerState::Changed => false,
            UIGestureRecognizerState::Ended
            | UIGestureRecognizerState::Cancelled
            | UIGestureRecognizerState::Failed => true,
            _ => return,
        };
        let (entered, exit_pending) = {
            let mut state = self.state.borrow_mut();
            state.mouse_position = position;
            state.modifiers = modifiers;
            state.capslock = convert::capslock(flags);
            // Moving again cancels an exit still pending.
            state.hover_ending = ended && state.hovered && state.pressed_button.is_none();
            (
                !ended && !mem::replace(&mut state.hovered, true),
                state.hover_ending,
            )
        };
        if ended {
            // Pressing a button also ends hovering, though the pointer stays:
            // the press continues as touches until released. The hover action
            // and the press's first touch may come in either order, so the
            // exit waits a main-queue turn for a press to cancel it.
            if exit_pending {
                self.defer(Self::finish_hover_exit);
            }
            return;
        }
        cursor::entered(self);
        if self.state.borrow().closed {
            return;
        }
        if entered {
            call(&self.callbacks.hover_status, |callback| callback(true));
        }
        self.dispatch(PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button: None,
            modifiers,
        }));
    }

    /// Hovering ended a turn ago and no press has begun since, so the
    /// pointer left.
    fn finish_hover_exit(&self) {
        let pending = {
            let mut state = self.state.borrow_mut();
            mem::take(&mut state.hover_ending) && state.pressed_button.is_none()
        };
        if pending {
            self.pointer_left();
        }
    }

    /// The pointer left the view: GPUI hears of it once.
    pub(super) fn pointer_left(&self) {
        let (exited, position, modifiers) = {
            let mut state = self.state.borrow_mut();
            state.hover_ending = false;
            state.cursor_style = CursorStyle::Arrow;
            (
                mem::replace(&mut state.hovered, false),
                state.mouse_position,
                state.modifiers,
            )
        };
        clear_window(self);
        if exited {
            self.pointer.invalidate();
            call(&self.callbacks.hover_status, |callback| callback(false));
            self.dispatch(PlatformInput::MouseExited(MouseExitEvent {
                position,
                pressed_button: None,
                modifiers,
            }));
        }
    }

    pub(super) fn handle_scroll(&self, recognizer: &UIPanGestureRecognizer) {
        let touch_phase = match recognizer.state() {
            UIGestureRecognizerState::Began => TouchPhase::Started,
            UIGestureRecognizerState::Changed => TouchPhase::Moved,
            UIGestureRecognizerState::Ended => TouchPhase::Ended,
            UIGestureRecognizerState::Cancelled | UIGestureRecognizerState::Failed => {
                TouchPhase::Cancelled
            }
            _ => return,
        };
        // Reading the translation and zeroing it turns UIKit's running total
        // into per-event deltas. Both follow the fingers, the direction GPUI's
        // pixel scroll deltas use on macOS.
        let translation = recognizer.translationInView(Some(&self.view));
        recognizer.setTranslation_inView(CGPoint::ZERO, Some(&self.view));
        let position = convert::point(recognizer.locationInView(Some(&self.view)));
        let flags = recognizer.modifierFlags();
        let modifiers = convert::modifiers(flags);
        {
            let mut state = self.state.borrow_mut();
            state.mouse_position = position;
            state.modifiers = modifiers;
            state.capslock = convert::capslock(flags);
        }
        self.dispatch(PlatformInput::Scroll(ScrollEvent {
            position,
            delta: ScrollDelta::Pixels(point(px(translation.x as f32), px(translation.y as f32))),
            modifiers,
            touch_phase,
            source: gpui::ScrollSource::Pointer,
        }));
    }
}
