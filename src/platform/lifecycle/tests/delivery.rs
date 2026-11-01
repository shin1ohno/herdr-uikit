//! A background report holds a task from the scene event until GPUI's
//! callback returned, and other reports hold nothing.
use std::{cell::RefCell, rc::Rc};

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    Begin,
    Phase(AppLifecyclePhase),
    End,
}

type Log = Rc<RefCell<Vec<Event>>>;

/// Stands in for `BackgroundTask`: logs its end when dropped.
struct Hold(Log);

impl Drop for Hold {
    fn drop(&mut self) {
        self.0.borrow_mut().push(Event::End);
    }
}

fn begin(log: &Log) -> impl FnOnce() -> Option<Hold> {
    let log = Rc::clone(log);
    move || {
        log.borrow_mut().push(Event::Begin);
        Some(Hold(log))
    }
}

fn report(log: &Log) -> impl FnMut(AppLifecyclePhase) {
    let log = Rc::clone(log);
    move |phase| log.borrow_mut().push(Event::Phase(phase))
}

#[test]
fn the_task_begins_at_the_scene_event_and_ends_after_the_callback() {
    let log = Log::default();
    let mut tracker = LifecycleTracker::default();
    tracker.update(FIRST, Some(SceneState::Active));

    let delivery = Delivery::new(
        tracker.update(FIRST, Some(SceneState::Background)),
        begin(&log),
    );
    // UIKit may suspend the app before the queued report runs.
    assert_eq!(*log.borrow(), [Event::Begin]);
    delivery.report(report(&log));

    assert_eq!(
        *log.borrow(),
        [
            Event::Begin,
            Event::Phase(Inactive),
            Event::Phase(Background),
            Event::End,
        ]
    );
}

#[test]
fn reports_that_stay_on_screen_hold_no_task() {
    let log = Log::default();
    let mut tracker = LifecycleTracker::default();
    tracker.update(FIRST, Some(SceneState::Active));

    for phases in [
        tracker.update(SECOND, Some(SceneState::Background)),
        tracker.update(FIRST, Some(SceneState::Foreground)),
        tracker.update(FIRST, Some(SceneState::Active)),
    ] {
        Delivery::new(phases, begin(&log)).report(report(&log));
    }

    assert_eq!(
        *log.borrow(),
        [Event::Phase(Inactive), Event::Phase(Active)]
    );
}

#[test]
fn a_refused_task_still_reports_the_background() {
    let log = Log::default();
    let delivery = Delivery::new(smallvec![Background], || None::<Hold>);
    delivery.report(report(&log));

    assert_eq!(*log.borrow(), [Event::Phase(Background)]);
}

#[test]
fn a_delivery_dropped_unreported_ends_its_task() {
    let log = Log::default();
    drop(Delivery::new(smallvec![Background], begin(&log)));

    assert_eq!(*log.borrow(), [Event::Begin, Event::End]);
}
