//! The phase script of one injected held press: which phases it may still
//! send, and to which view. Std only, so the host tests
//! (`tests/held_press_script.rs`) compile this file without UIKit.
//!
//! Rules, checked before any selector is sent:
//! - `Began` and `Changed` go only to the window's current text view.
//! - Each view sees the press begin at most once. A second `Began` on a new
//!   current view models UIKit delivering the held press again after the
//!   old view retired.
//! - `Changed`, `Ended` and `Cancelled` go only to a view that saw this press
//!   begin. A retired view qualifies when it saw the `Began` before retiring.
//! - `Ended` or `Cancelled` finishes the press: nothing more is sent with it.

use std::ffi::CStr;

/// The four `UIPress` phases a script may send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeldProbePhase {
    Began,
    Changed,
    Ended,
    Cancelled,
}

impl HeldProbePhase {
    pub const ALL: [Self; 4] = [Self::Began, Self::Changed, Self::Ended, Self::Cancelled];

    /// The UIResponder selector this phase is sent through.
    pub fn selector(self) -> &'static CStr {
        match self {
            Self::Began => c"pressesBegan:withEvent:",
            Self::Changed => c"pressesChanged:withEvent:",
            Self::Ended => c"pressesEnded:withEvent:",
            Self::Cancelled => c"pressesCancelled:withEvent:",
        }
    }

    /// The raw `UIPressPhase` the injected press reports (`UIPress.h`:
    /// Began 0, Changed 1, Stationary 2, Ended 3, Cancelled 4).
    pub fn uikit_phase(self) -> isize {
        match self {
            Self::Began => 0,
            Self::Changed => 1,
            Self::Ended => 3,
            Self::Cancelled => 4,
        }
    }

    /// Whether this phase finishes the press.
    pub fn finishes(self) -> bool {
        matches!(self, Self::Ended | Self::Cancelled)
    }
}

/// The key an injected press carries: its HID usage and the characters it
/// reports, with no modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeldProbeKeySpec {
    pub usage: isize,
    pub characters: &'static str,
}

impl HeldProbeKeySpec {
    /// `UIKeyboardHIDUsageKeyboardEscape`, which the SDK keymap names
    /// "escape" by usage and by character.
    pub const ESCAPE: Self = Self {
        usage: 41,
        characters: "\u{1b}",
    };
}

/// Where a phase is sent: the window's current text view, or a captured
/// text view that has since retired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeldProbeDestination {
    Current,
    Retired,
}

/// Why a phase was not sent. Nothing reaches UIKit when a step is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeldProbeScriptError {
    /// The press already ended or was cancelled.
    Finished,
    /// `Began` or `Changed` addressed to a retired view.
    NotCurrent,
    /// This view already saw the press begin.
    AlreadyBegan,
    /// `Changed`, `Ended` or `Cancelled` to a view that never saw it begin.
    NotBegun,
}

/// The history of one injected press, keyed by native client identity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeldProbeScript {
    began: Vec<u64>,
    finished: bool,
}

impl HeldProbeScript {
    /// Check `phase` to `destination` (whose client identity is `view`) and,
    /// when allowed, record it. A refused step changes nothing.
    pub fn step(
        &mut self,
        phase: HeldProbePhase,
        destination: HeldProbeDestination,
        view: u64,
    ) -> Result<(), HeldProbeScriptError> {
        if self.finished {
            return Err(HeldProbeScriptError::Finished);
        }
        let began = self.began.contains(&view);
        match phase {
            HeldProbePhase::Began | HeldProbePhase::Changed
                if destination != HeldProbeDestination::Current =>
            {
                Err(HeldProbeScriptError::NotCurrent)
            }
            HeldProbePhase::Began if began => Err(HeldProbeScriptError::AlreadyBegan),
            HeldProbePhase::Began => {
                self.began.push(view);
                Ok(())
            }
            _ if !began => Err(HeldProbeScriptError::NotBegun),
            _ => {
                self.finished = phase.finishes();
                Ok(())
            }
        }
    }

    /// Whether `view` saw this press begin.
    pub fn began_on(&self, view: u64) -> bool {
        self.began.contains(&view)
    }

    /// Whether the press ended or was cancelled.
    pub fn finished(&self) -> bool {
        self.finished
    }
}
