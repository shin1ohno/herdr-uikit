//! GCD-backed `gpui::PlatformDispatcher`, as on macOS: the main queue for
//! foreground tasks, global queues by priority for background tasks, and
//! `dispatch_after` for timers. Ported from `gpui-pre-macos`'s dispatcher
//! (NOTICE.md).
//!
//! Upstream hands GCD the raw `Runnable` pointer and a C trampoline. This port
//! submits a closure that owns the `Runnable` through dispatch2's safe API
//! instead, which costs one small allocation per task and needs no `unsafe`:
//! a `Runnable` is `Send`, and GCD runs each closure exactly once.

#[cfg(test)]
mod tests;

use std::time::Duration;

use dispatch2::{DispatchQueue, DispatchQueueGlobalPriority, DispatchTime, GlobalQueueIdentifier};
use gpui::{PlatformDispatcher, Priority, RunnableVariant};
use objc2::MainThreadMarker;

/// Longest timer delay handed to GCD, about 146 years. `dispatch_time`
/// returns `DISPATCH_TIME_FOREVER` when a deadline overflows, and
/// `dispatch_after` is undefined for that value, so longer delays are capped.
const MAX_DELAY_NANOS: i64 = 1 << 62;

/// Holds no state: GCD owns the queues and the threads behind them.
pub(crate) struct IosDispatcher {}

impl IosDispatcher {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the platform module constructs it; that module is still a Wave 0 stub"
        )
    )]
    pub(crate) fn new() -> Self {
        Self {}
    }
}

// `prevent_app_nap` keeps the trait's no-op: App Nap is macOS's. iPadOS
// suspends a background app through the scene lifecycle, which an activity
// assertion does not prevent.
impl PlatformDispatcher for IosDispatcher {
    fn is_main_thread(&self) -> bool {
        MainThreadMarker::new().is_some()
    }

    fn dispatch(&self, runnable: RunnableVariant, priority: Priority) {
        let queue = GlobalQueueIdentifier::Priority(queue_priority(priority));
        DispatchQueue::global_queue(queue).exec_async(move || run(runnable));
    }

    fn dispatch_on_main_thread(&self, runnable: RunnableVariant, _priority: Priority) {
        DispatchQueue::main().exec_async(move || run(runnable));
    }

    fn dispatch_after(&self, duration: Duration, runnable: RunnableVariant) {
        let queue = DispatchQueue::global_queue(GlobalQueueIdentifier::Priority(
            DispatchQueueGlobalPriority::High,
        ));
        let when = DispatchTime::NOW.time(delay_nanos(duration));
        if let Err(error) = queue.after(when, move || run(runnable)) {
            tracing::error!(
                ?error,
                ?duration,
                "GCD refused a timer, so its task never runs"
            );
        }
    }

    /// A dedicated thread, as on macOS, but at normal priority: upstream raises
    /// it through Mach thread policies, which this crate does not link. iOS
    /// apps get realtime audio from Core Audio's own I/O threads, and Herdr UI
    /// plays no audio through GPUI.
    fn spawn_realtime(&self, f: Box<dyn FnOnce() + Send>) {
        let spawned = std::thread::Builder::new()
            .name("gpui-realtime".to_owned())
            .spawn(f);
        if let Err(error) = spawned {
            tracing::error!(%error, "could not start a thread for a realtime GPUI task");
        }
    }
}

/// The GCD global queue for a GPUI priority. GPUI's executor sends realtime
/// work to `spawn_realtime`, never here; upstream panics if it arrives anyway,
/// and this runs it at high priority instead.
fn queue_priority(priority: Priority) -> DispatchQueueGlobalPriority {
    match priority {
        Priority::RealtimeAudio | Priority::High => DispatchQueueGlobalPriority::High,
        Priority::Medium => DispatchQueueGlobalPriority::Default,
        Priority::Low => DispatchQueueGlobalPriority::Low,
    }
}

/// `duration` as the nanosecond delta `dispatch_time` takes, saturating at
/// [`MAX_DELAY_NANOS`].
fn delay_nanos(duration: Duration) -> i64 {
    i64::try_from(duration.as_nanos()).map_or(MAX_DELAY_NANOS, |nanos| nanos.min(MAX_DELAY_NANOS))
}

/// Runs a task inside GPUI's profiler bookkeeping, as upstream's trampoline
/// does.
fn run(runnable: RunnableVariant) {
    let metadata = runnable.metadata();
    gpui::profiler::update_running_task(metadata.spawned, metadata.location);
    runnable.run();
    gpui::profiler::save_task_timing();
}
