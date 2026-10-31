#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{
    sync::{Arc, mpsc},
    time::Instant,
};

use gpui::BackgroundExecutor;

use super::*;

/// Long enough for a loaded simulator, short enough that a lost task fails
/// the test rather than hanging it.
const WAIT: Duration = Duration::from_secs(10);

fn executor() -> BackgroundExecutor {
    BackgroundExecutor::new(Arc::new(IosDispatcher::new()))
}

#[test]
fn priorities_map_to_the_matching_global_queues() {
    assert_eq!(
        queue_priority(Priority::High),
        DispatchQueueGlobalPriority::High
    );
    assert_eq!(
        queue_priority(Priority::Medium),
        DispatchQueueGlobalPriority::Default
    );
    assert_eq!(
        queue_priority(Priority::Low),
        DispatchQueueGlobalPriority::Low
    );
}

#[test]
fn stray_realtime_work_runs_at_high_priority_instead_of_panicking() {
    assert_eq!(
        queue_priority(Priority::RealtimeAudio),
        DispatchQueueGlobalPriority::High
    );
}

#[test]
fn timer_delays_convert_to_nanoseconds() {
    assert_eq!(delay_nanos(Duration::ZERO), 0);
    assert_eq!(delay_nanos(Duration::from_millis(1_500)), 1_500_000_000);
}

#[test]
fn timer_delays_saturate_below_dispatch_time_forever() {
    let cap = Duration::from_nanos(MAX_DELAY_NANOS.unsigned_abs());
    assert_eq!(delay_nanos(cap), MAX_DELAY_NANOS);
    assert_eq!(delay_nanos(cap + Duration::from_nanos(1)), MAX_DELAY_NANOS);
    assert_eq!(delay_nanos(Duration::MAX), MAX_DELAY_NANOS);
}

#[test]
fn background_tasks_run_on_a_gcd_queue() {
    let executor = executor();
    let (sender, receiver) = mpsc::channel();
    executor
        .spawn(async move {
            sender.send(std::thread::current().id()).unwrap();
        })
        .detach();

    let ran_on = receiver.recv_timeout(WAIT).unwrap();
    assert_ne!(ran_on, std::thread::current().id());
}

#[test]
fn every_priority_reaches_its_queue() {
    let executor = executor();
    let (sender, receiver) = mpsc::channel();
    for priority in [Priority::High, Priority::Medium, Priority::Low] {
        let sender = sender.clone();
        executor
            .spawn_with_priority(priority, async move {
                sender.send(priority).unwrap();
            })
            .detach();
    }

    let mut ran: Vec<_> = (0..3)
        .map(|_| receiver.recv_timeout(WAIT).unwrap())
        .collect();
    ran.sort_by_key(|priority| *priority as u8);
    assert_eq!(ran, [Priority::High, Priority::Medium, Priority::Low]);
}

#[test]
fn timers_fire_after_their_delay() {
    let executor = executor();
    let delay = Duration::from_millis(30);
    let (sender, receiver) = mpsc::channel();
    let started = Instant::now();
    let timer = executor.timer(delay);
    executor
        .spawn(async move {
            timer.await;
            sender.send(Instant::now()).unwrap();
        })
        .detach();

    let fired = receiver.recv_timeout(WAIT).unwrap();
    assert!(fired.duration_since(started) >= delay);
}

#[test]
fn background_threads_are_not_the_main_thread() {
    let executor = executor();
    let (sender, receiver) = mpsc::channel();
    executor
        .spawn(async move {
            sender.send(IosDispatcher::new().is_main_thread()).unwrap();
        })
        .detach();

    assert!(!receiver.recv_timeout(WAIT).unwrap());
}
