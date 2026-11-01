//! Frames on demand. GPUI wakes the window when it has something to draw
//! (`frame_waker` and `schedule_frame`); the display link then runs, and
//! pauses again once a frame leaves no demand behind. An idle window so costs
//! no frames and lets a ProMotion screen lower its refresh rate, where
//! macOS's always-running link would keep it at 120 Hz.
//!
//! The link also animates the keyboard inset: GPUI's inset contract has iOS
//! interpolate the keyboard's movement on frame ticks.

use std::{
    mem,
    rc::Rc,
    time::{Duration, Instant},
};

use gpui::{RequestFrameOptions, px};

use super::{
    state::{Window, call},
    text_state::{self, TextState},
};
use crate::display_link::DisplayLink;

impl Window {
    /// Creates the paused display link that drives this window's frames.
    pub(super) fn install_display_link(self: &Rc<Self>) -> crate::Result<()> {
        let window = Rc::downgrade(self);
        let link = DisplayLink::new(
            self.mtm,
            Box::new(move || {
                if let Some(window) = window.upgrade() {
                    window.tick();
                }
            }),
        )?;
        *self.display_link.borrow_mut() = Some(link);
        Ok(())
    }

    /// GPUI has something to draw. Runs the link while the window is shown;
    /// otherwise the request waits for the window to become visible.
    pub(super) fn request_frame(&self) {
        let can_draw = {
            let mut state = self.state.borrow_mut();
            state.frame_requested = true;
            state.can_draw()
        };
        if can_draw {
            self.start_display_link();
        }
    }

    /// Requests a frame that redraws every view, not just invalidated ones.
    pub(super) fn force_frame(&self) {
        self.state.borrow_mut().force_render = true;
        self.request_frame();
    }

    pub(super) fn stop_display_link(&self) {
        if let Some(link) = self.display_link.borrow_mut().as_mut() {
            link.stop();
        }
    }

    fn start_display_link(&self) {
        if let Some(link) = self.display_link.borrow_mut().as_mut() {
            link.start();
        }
    }

    /// Paces frames by `screen`, the one the window's scene is on. Naming
    /// the current screen again costs nothing.
    pub(super) fn follow_screen(&self, screen: &objc2_ui_kit::UIScreen) {
        if let Some(link) = self.display_link.borrow_mut().as_mut()
            && let Err(error) = link.set_screen(screen)
        {
            tracing::warn!(%error, "frames keep the previous screen's pace");
        }
    }

    /// Draws the resized window now, inside the layout pass, and presents it
    /// with UIKit's transaction, so a rotation or a Stage Manager resize
    /// shows the new layout in step with the window's own animation rather
    /// than stretching the old frame for a tick. macOS does the same during
    /// a live resize.
    pub(super) fn draw_in_layout_transaction(&self) {
        self.state.borrow_mut().force_render = true;
        self.renderer
            .borrow_mut()
            .set_presents_with_transaction(true);
        self.tick();
        self.renderer
            .borrow_mut()
            .set_presents_with_transaction(false);
    }

    /// One display link tick, on the main thread.
    fn tick(&self) {
        if !self.state.borrow().can_draw() {
            self.stop_display_link();
            return;
        }
        // Updating insets invalidates the entire layout. Sample before taking
        // force_render so this very frame bypasses cached view geometry.
        self.step_keyboard_animation(Instant::now());
        self.sample_keyboard_layout();
        let options = {
            let mut state = self.state.borrow_mut();
            if state.can_draw() {
                state.frame_requested = false;
                Some(RequestFrameOptions {
                    require_presentation: false,
                    force_render: mem::take(&mut state.force_render),
                })
            } else {
                None
            }
        };
        let Some(options) = options else {
            self.stop_display_link();
            return;
        };

        call(&self.callbacks.request_frame, |callback| callback(options));
        self.flush_input_handler_change();

        let idle = {
            let state = self.state.borrow();
            !state.frame_requested && !state.force_render && state.keyboard_animation.is_none()
        };
        if idle {
            self.stop_display_link();
        }
    }

    /// Tells the text input view when the focused element's text state may
    /// have changed (`text_state`), at most once per frame. GPUI re-registers
    /// the handler around every frame and event, so a registration is not a
    /// change. After the frame no GPUI update is on the stack, so the handler
    /// can be read here, and UIKit's synchronous queries back into it can
    /// reach the app.
    fn flush_input_handler_change(&self) {
        let current = self.with_input_handler(TextState::sample);
        let changed = {
            let mut state = self.state.borrow_mut();
            let changed = text_state::changed(state.last_text_state.as_ref(), current.as_ref());
            state.last_text_state = current;
            changed && !state.closed
        };
        if changed {
            self.text_input.input_handler_changed();
        }
    }

    fn step_keyboard_animation(&self, now: Instant) {
        let insets = {
            let mut state = self.state.borrow_mut();
            let Some(animation) = state.keyboard_animation else {
                return;
            };
            let (bottom, finished) = animation.sample(now);
            if finished {
                state.keyboard_animation = None;
            }
            let bottom = px(bottom);
            if state.insets.ime.bottom == bottom {
                return;
            }
            state.insets.ime.bottom = bottom;
            state.force_render = true;
            state.insets.clone()
        };
        call(&self.callbacks.insets_changed, |callback| callback(insets));
    }
}

/// The keyboard's bottom inset moving from `from` to `to` points. UIKit
/// animates the keyboard with a private spring curve; a cubic ease-out
/// follows it closely and lands exactly on `to` when the duration ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct InsetAnimation {
    duration: Duration,
    from: f32,
    start: Instant,
    to: f32,
}

impl InsetAnimation {
    pub(super) fn new(from: f32, to: f32, start: Instant, duration: Duration) -> Self {
        Self {
            duration,
            from,
            start,
            to,
        }
    }

    /// The inset at `now`, and whether the animation has finished.
    pub(super) fn sample(&self, now: Instant) -> (f32, bool) {
        let elapsed = now.saturating_duration_since(self.start);
        if self.duration.is_zero() || elapsed >= self.duration {
            return (self.to, true);
        }
        let progress = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        let eased = 1.0 - (1.0 - progress).powi(3);
        (self.from + (self.to - self.from) * eased, false)
    }
}
