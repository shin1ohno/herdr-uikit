//! Origin of UIKit editing commands during synchronous GPUI key dispatch.

use std::cell::Cell;

/// The native editing command that synthesized the current GPUI key event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeEditAction {
    /// UIKit's `copy:` command (Command-C).
    Copy,
    /// UIKit's `cut:` command (Command-X).
    Cut,
    /// UIKit's `paste:` command (Command-V).
    Paste,
    /// UIKit's `selectAll:` command (Command-A).
    SelectAll,
}

thread_local! {
    static CURRENT: Cell<Option<NativeEditAction>> = const { Cell::new(None) };
}

/// Returns the native editing command during its synthesized key-down and
/// key-up callbacks on this thread, or `None` outside that synchronous scope.
///
/// Read this inside the input callback to distinguish a native Paste from a
/// hardware Command-V with the same [`gpui::Keystroke`]. Ordinary physical key
/// dispatch has no native origin. Modifier restoration and the unhandled-paste
/// fallback run after the previous origin has been restored. Nested native
/// dispatch restores the enclosing origin, even
/// when a callback unwinds. The value is not carried into deferred tasks.
pub fn native_edit_action() -> Option<NativeEditAction> {
    CURRENT.get()
}

pub(crate) struct NativeEditActionGuard(Option<NativeEditAction>);

impl NativeEditActionGuard {
    pub(crate) fn enter(action: Option<NativeEditAction>) -> Self {
        Self(CURRENT.replace(action))
    }
}

impl Drop for NativeEditActionGuard {
    fn drop(&mut self) {
        CURRENT.set(self.0);
    }
}
