//! Keeping the iPad awake. iOS gives apps one control over sleep, the idle
//! timer that locks the screen; once the screen locks, the app is suspended.
//! `prevent_idle_sleep` therefore disables the idle timer for as long as any
//! guard is alive.

use std::cell::Cell;

use dispatch2::DispatchQueue;
use gpui::ActivityGuard;
use objc2::MainThreadMarker;
use objc2_ui_kit::UIApplication;

thread_local! {
    /// Live guards. Only touched on the main thread.
    static HOLDS: Cell<usize> = const { Cell::new(0) };
}

/// Disables the idle timer until the returned guard and every other live
/// one are dropped. `reason` is kept in the log, as the system keeps no
/// record of it.
pub(super) fn hold(reason: &str, mtm: MainThreadMarker) -> ActivityGuard {
    let holds = HOLDS.with(|holds| {
        holds.set(holds.get() + 1);
        holds.get()
    });
    tracing::debug!(reason, holds, "keeping the screen from locking");
    if holds == 1 {
        UIApplication::sharedApplication(mtm).setIdleTimerDisabled(true);
    }
    // A guard can be dropped on any thread, and the count lives on the main
    // one.
    ActivityGuard::new(|| DispatchQueue::main().exec_async(release))
}

fn release() {
    let Some(mtm) = MainThreadMarker::new() else {
        tracing::error!("the idle timer guard was released off the main thread");
        return;
    };
    let holds = HOLDS.with(|holds| {
        holds.set(holds.get().saturating_sub(1));
        holds.get()
    });
    if holds == 0 {
        UIApplication::sharedApplication(mtm).setIdleTimerDisabled(false);
    }
}
