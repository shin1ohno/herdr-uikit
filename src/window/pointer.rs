//! The pointer between clicks: a `UIHoverGestureRecognizer` reports a
//! trackpad, mouse, or hovering Pencil moving over the view, and two
//! `UIPanGestureRecognizer`s that accept only scroll events (no touch types)
//! report two-finger trackpad scrolling, which coasts after the fingers lift
//! (`momentum`), and mouse wheel notches, which arrive as whole lines. Clicks
//! and drags are touches, handled in `touch`.

mod cursor;
pub(super) mod momentum;

pub(crate) use cursor::set_cursor_style;
pub(super) use cursor::{Pointer, clear_window};

use std::{mem, rc::Rc, time::Instant};

use gpui::{
    CursorStyle, Modifiers, MouseExitEvent, MouseMoveEvent, Pixels, PlatformInput, Point,
    ScrollDelta, ScrollEvent, TouchPhase, point, px,
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
/// `handleHover:`, `handleScroll:` (trackpad) and `handleWheel:` (wheel)
/// methods.
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
    scroll.setAllowedScrollTypesMask(UIScrollTypeMask::Continuous);
    // No touch types, so fingers stay raw touches for GPUI's own pan
    // recognition and only scroll events reach this recognizer.
    scroll.setAllowedTouchTypes(&NSArray::<NSNumber>::from_slice(&[]));
    scroll.setCancelsTouchesInView(false);
    view.addGestureRecognizer(&scroll);

    // SAFETY: as above, for `handleWheel:`.
    let wheel = unsafe {
        UIPanGestureRecognizer::initWithTarget_action(
            UIPanGestureRecognizer::alloc(mtm),
            Some(target),
            Some(sel!(handleWheel:)),
        )
    };
    wheel.setAllowedScrollTypesMask(UIScrollTypeMask::Discrete);
    wheel.setAllowedTouchTypes(&NSArray::<NSNumber>::from_slice(&[]));
    wheel.setCancelsTouchesInView(false);
    view.addGestureRecognizer(&wheel);
}

/// The scroll event for one step of a trackpad gesture or its coast.
pub(super) fn trackpad_event(
    position: Point<Pixels>,
    modifiers: Modifiers,
    emit: momentum::Emit,
) -> ScrollEvent {
    let (x, y) = emit.delta;
    ScrollEvent {
        position,
        delta: ScrollDelta::Pixels(point(px(x), px(y))),
        modifiers,
        touch_phase: match emit.phase {
            momentum::Phase::Started => TouchPhase::Started,
            momentum::Phase::Moved => TouchPhase::Moved,
            momentum::Phase::Ended => TouchPhase::Ended,
            momentum::Phase::Cancelled => TouchPhase::Cancelled,
        },
        source: gpui::ScrollSource::Pointer,
    }
}

/// The scroll event for one wheel notch: whole lines, which GPUI scrolls by
/// its line height and yebisu by Herdr's `ui.mouse_scroll_lines`.
pub(super) fn wheel_event(
    position: Point<Pixels>,
    modifiers: Modifiers,
    translation: (f32, f32),
) -> Option<ScrollEvent> {
    let (x, y) = momentum::notch(translation)?;
    Some(ScrollEvent {
        position,
        delta: ScrollDelta::Lines(point(x, y)),
        modifiers,
        touch_phase: TouchPhase::Moved,
        source: gpui::ScrollSource::Pointer,
    })
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
        let state = recognizer.state();
        if !matches!(
            state,
            UIGestureRecognizerState::Began
                | UIGestureRecognizerState::Changed
                | UIGestureRecognizerState::Ended
                | UIGestureRecognizerState::Cancelled
                | UIGestureRecognizerState::Failed
        ) {
            return;
        }
        // Reading the translation and zeroing it turns UIKit's running total
        // into per-event deltas. Both follow the fingers, the direction GPUI's
        // pixel scroll deltas use on macOS.
        let translation = recognizer.translationInView(Some(&self.view));
        recognizer.setTranslation_inView(CGPoint::ZERO, Some(&self.view));
        let delta = (translation.x as f32, translation.y as f32);
        let (position, modifiers) = self.track_scroll_pointer(recognizer);
        let events = {
            let mut window = self.state.borrow_mut();
            let gesture = &mut window.scroll_momentum;
            match state {
                UIGestureRecognizerState::Began => gesture.began(delta),
                UIGestureRecognizerState::Changed => vec![gesture.changed(delta)],
                UIGestureRecognizerState::Ended => {
                    let velocity = recognizer.velocityInView(Some(&self.view));
                    vec![gesture.ended(
                        delta,
                        (velocity.x as f32, velocity.y as f32),
                        Instant::now(),
                    )]
                }
                _ => vec![gesture.cancelled(delta)],
            }
        };
        let coasting = self.state.borrow().scroll_momentum.coasting();
        for emit in events {
            self.dispatch(PlatformInput::Scroll(trackpad_event(
                position, modifiers, emit,
            )));
        }
        if coasting {
            self.request_frame();
        }
    }

    pub(super) fn handle_wheel(&self, recognizer: &UIPanGestureRecognizer) {
        if !matches!(
            recognizer.state(),
            UIGestureRecognizerState::Began | UIGestureRecognizerState::Changed
        ) {
            return;
        }
        let translation = recognizer.translationInView(Some(&self.view));
        recognizer.setTranslation_inView(CGPoint::ZERO, Some(&self.view));
        let (position, modifiers) = self.track_scroll_pointer(recognizer);
        // A wheel ends any trackpad coast first.
        self.stop_scroll_momentum();
        if let Some(event) = wheel_event(
            position,
            modifiers,
            (translation.x as f32, translation.y as f32),
        ) {
            self.dispatch(PlatformInput::Scroll(event));
        }
    }

    /// Ends a trackpad coast, closing its gesture with the deferred `Ended`.
    pub(super) fn stop_scroll_momentum(&self) {
        let (ended, position, modifiers) = {
            let mut state = self.state.borrow_mut();
            (
                state.scroll_momentum.stop(),
                state.mouse_position,
                state.modifiers,
            )
        };
        if let Some(emit) = ended {
            self.dispatch(PlatformInput::Scroll(trackpad_event(
                position, modifiers, emit,
            )));
        }
    }

    /// One display frame of a trackpad coast, at the pointer.
    pub(super) fn step_scroll_momentum(&self, now: Instant) {
        let (emit, position, modifiers) = {
            let mut state = self.state.borrow_mut();
            (
                state.scroll_momentum.frame(now),
                state.mouse_position,
                state.modifiers,
            )
        };
        if let Some(emit) = emit {
            self.dispatch(PlatformInput::Scroll(trackpad_event(
                position, modifiers, emit,
            )));
        }
    }

    /// Records where a scroll recognizer's pointer is and which modifiers it
    /// holds, as every pointer event does.
    fn track_scroll_pointer(
        &self,
        recognizer: &UIPanGestureRecognizer,
    ) -> (Point<Pixels>, Modifiers) {
        let position = convert::point(recognizer.locationInView(Some(&self.view)));
        let flags = recognizer.modifierFlags();
        let modifiers = convert::modifiers(flags);
        let mut state = self.state.borrow_mut();
        state.mouse_position = position;
        state.modifiers = modifiers;
        state.capslock = convert::capslock(flags);
        (position, modifiers)
    }
}
