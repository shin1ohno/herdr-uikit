//! Receipts of input as it arrives, for Simulator QA only.
//!
//! A test that synthesizes a touch or picks a menu command can only tell a
//! lost input from a slow one if the app says where the input stopped. Each
//! receipt is one `tracing` event at level INFO with the target
//! `herdr_uikit::qa_input`; a consumer's QA build subscribes to that target and records
//! the events next to its own observations. The module and every call into
//! it are compiled only for the iOS Simulator (`target_abi = "sim"`), so a
//! device build carries none of it and behaves exactly as before.
//!
//! Every event has a `receipt` field naming its kind:
//!
//! - `touch`: a direct or pointer touch reached the drawing view's
//!   `touchesBegan`/`Ended`/`Cancelled` callback (moves are left out, as a
//!   drag would flood the record). Fields: `stage` (`began`, `ended`,
//!   `cancelled`), `kind` (`direct`, `pointer`), `x` and `y` in the window's
//!   points, and `touches`, the number of touches in that callback.
//! - `menu-perform`: UIKit called `herdrMenuAction:` on the drawing view or
//!   the application delegate. Fields: `outcome` (`no_sender`, `no_tag` or
//!   `performed`), and for a tagged command its `generation` and `index`, the
//!   menus' `current` generation (`-1` when the platform is not running), and
//!   `keyed`, whether a key command carrying a key input performed it.
//! - `menu-deliver`: the command's action was handed, or not, to GPUI's menu
//!   callback on the main queue. Fields: `outcome` (`stale_generation`,
//!   `no_callback`, `callback_running` or `delivered`), `generation`,
//!   `index` and `current`.
//!
//! - `frame`: a window's draw came to a different outcome than the draw
//!   before it (each draw would flood the record). Fields: `outcome`
//!   (`presented`, `zero_size`, `paths_not_ready`, `no_drawable`,
//!   `render_failed`), `previous`, the outcome of the run it ends (`none` for
//!   a window's first draw), and `previous_count`, how many draws that run had.
//!
//! A `menu-perform` that is `performed` and keyed with no `menu-deliver`
//! after it was handled as a key shortcut by the text input view.

use crate::{menu::MenuTag, renderer::FrameOutcome};

/// The `tracing` target of every receipt.
#[cfg(test)]
pub(crate) const TARGET: &str = "herdr_uikit::qa_input";

/// Which touch callback delivered a touch, as the receipt names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TouchStage {
    Began,
    Ended,
    Cancelled,
}

impl TouchStage {
    fn name(self) -> &'static str {
        match self {
            Self::Began => "began",
            Self::Ended => "ended",
            Self::Cancelled => "cancelled",
        }
    }
}

/// A touch reached the drawing view.
pub(crate) fn touch(stage: TouchStage, pointer: bool, x: f32, y: f32, touches: usize) {
    tracing::info!(
        target: "herdr_uikit::qa_input",
        receipt = "touch",
        stage = stage.name(),
        kind = if pointer { "pointer" } else { "direct" },
        x = f64::from(x),
        y = f64::from(y),
        touches = touches as u64,
    );
}

/// UIKit performed `herdrMenuAction:` with a sender. `named` is the tag the
/// sender carries, `None` when it is no command or carries no tag.
pub(crate) fn menu_perform(
    sender: bool,
    named: Option<MenuTag>,
    current: Option<u32>,
    keyed: bool,
) {
    let current = current.map_or(-1, i64::from);
    match (sender, named) {
        (false, _) => tracing::info!(
            target: "herdr_uikit::qa_input",
            receipt = "menu-perform",
            outcome = "no_sender",
            current,
        ),
        (true, None) => tracing::info!(
            target: "herdr_uikit::qa_input",
            receipt = "menu-perform",
            outcome = "no_tag",
            current,
        ),
        (true, Some(tag)) => tracing::info!(
            target: "herdr_uikit::qa_input",
            receipt = "menu-perform",
            outcome = "performed",
            generation = u64::from(tag.generation),
            index = u64::from(tag.index),
            current,
            keyed,
        ),
    }
}

/// What delivering a performed command's action to GPUI came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Delivery {
    /// The tag names no command of the current menus.
    StaleGeneration,
    /// GPUI registered no menu callback.
    NoCallback,
    /// GPUI's menu callback was already running further up the stack.
    CallbackRunning,
    /// GPUI's menu callback ran with the action.
    Delivered,
}

impl Delivery {
    fn name(self) -> &'static str {
        match self {
            Self::StaleGeneration => "stale_generation",
            Self::NoCallback => "no_callback",
            Self::CallbackRunning => "callback_running",
            Self::Delivered => "delivered",
        }
    }
}

/// A performed command's action reached, or did not reach, GPUI.
pub(crate) fn menu_deliver(tag: MenuTag, current: u32, delivery: Delivery) {
    tracing::info!(
        target: "herdr_uikit::qa_input",
        receipt = "menu-deliver",
        outcome = delivery.name(),
        generation = u64::from(tag.generation),
        index = u64::from(tag.index),
        current = i64::from(current),
    );
}

impl FrameOutcome {
    fn name(self) -> &'static str {
        match self {
            Self::Presented => "presented",
            Self::ZeroSize => "zero_size",
            Self::PathsNotReady => "paths_not_ready",
            Self::NoDrawable => "no_drawable",
            Self::RenderFailed => "render_failed",
        }
    }
}

/// One window's draws, folded into runs of one outcome.
#[derive(Debug, Default)]
pub(crate) struct FrameRuns {
    current: Option<FrameOutcome>,
    count: u64,
}

impl FrameRuns {
    /// Counts `outcome`. When it starts a new run, returns the run it ended:
    /// that run's outcome (`None` before a window's first draw) and length.
    pub(crate) fn note(&mut self, outcome: FrameOutcome) -> Option<(Option<FrameOutcome>, u64)> {
        if self.current == Some(outcome) {
            self.count = self.count.saturating_add(1);
            return None;
        }
        let ended = (self.current.replace(outcome), self.count);
        self.count = 1;
        Some(ended)
    }
}

/// A window's draws changed outcome (`FrameRuns::note`).
pub(crate) fn frame(outcome: FrameOutcome, previous: Option<FrameOutcome>, previous_count: u64) {
    tracing::info!(
        target: "herdr_uikit::qa_input",
        receipt = "frame",
        outcome = outcome.name(),
        previous = previous.map_or("none", FrameOutcome::name),
        previous_count,
    );
}

#[cfg(test)]
pub(super) mod tests;
