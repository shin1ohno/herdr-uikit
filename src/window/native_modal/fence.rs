//! Presses that must not reach GPUI: keys already held when a window's gate
//! shut, and keys that began while it was shut. Each stays fenced, in every
//! phase (began, changed, ended, cancelled), until its own release has been
//! delivered or UIKit stops reporting it held, whichever comes first.
//!
//! Keyed by HID usage, per window. Modifiers are not fenced: GPUI adopts the
//! modifier flags every key reports. std only: `tests/native_modal.rs`
//! includes this file.

use std::collections::BTreeSet;

/// More distinct keys than a keyboard reports at once; past it every
/// non-modifier key is fenced until nothing is held.
pub const CAPACITY: usize = 32;

/// HID keyboard modifiers (0xE0-0xE7) and caps lock (0x39), never fenced.
pub fn is_modifier(usage: isize) -> bool {
    (0xE0..=0xE7).contains(&usage) || usage == 0x39
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Fence {
    usages: BTreeSet<isize>,
    overflow: bool,
}

impl Fence {
    pub fn hold(&mut self, usage: isize) {
        if self.usages.len() >= CAPACITY && !self.usages.contains(&usage) {
            self.overflow = true;
            return;
        }
        self.usages.insert(usage);
    }

    /// Whether a press of `usage` must be kept from GPUI.
    pub fn blocks(&self, usage: isize) -> bool {
        self.overflow || self.usages.contains(&usage)
    }

    /// The press's release was delivered (wherever it went).
    pub fn released(&mut self, usage: isize) {
        self.usages.remove(&usage);
    }

    /// UIKit's view of what is physically down now (a key event's
    /// `allPresses` that have not ended). A fenced key UIKit no longer
    /// reports lost its release; overflow ends once nothing is held.
    pub fn retain_held(&mut self, held: &[isize]) {
        self.usages.retain(|usage| held.contains(usage));
        if held.is_empty() {
            self.overflow = false;
        }
    }

    #[allow(dead_code, reason = "read by Simulator QA and the host tests")]
    pub fn len(&self) -> usize {
        self.usages.len()
    }

    #[allow(dead_code, reason = "read by Simulator QA and the host tests")]
    pub fn overflowed(&self) -> bool {
        self.overflow
    }
}
