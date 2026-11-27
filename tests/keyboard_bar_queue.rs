//! Host tests for the shortcuts bar's press queue (std only): the order a
//! consumer reads presses in, the bound, and the end of the stream.
#[path = "../src/keyboard_bar/queue.rs"]
mod queue;

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Wake, Waker},
};

use queue::{CAPACITY, PressQueue};

struct Count(AtomicUsize);

impl Wake for Count {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn waker() -> (Arc<Count>, Waker) {
    let count = Arc::new(Count(AtomicUsize::new(0)));
    (count.clone(), Waker::from(count))
}

#[test]
fn presses_are_read_oldest_first() {
    let mut queue = PressQueue::default();
    assert!(queue.push(3));
    assert!(queue.push(1));
    assert!(queue.push(3));
    assert_eq!(queue.pop(), Some(3));
    assert_eq!(queue.pop(), Some(1));
    assert_eq!(queue.pop(), Some(3));
    assert_eq!(queue.pop(), None);
}

#[test]
fn a_press_wakes_the_waiting_reader_once() {
    let mut queue = PressQueue::default();
    let (count, waker) = waker();
    queue.register(&waker);
    queue.push(7);
    queue.push(8);
    // The reader registers again after it drained the queue.
    assert_eq!(count.0.load(Ordering::SeqCst), 1);
}

#[test]
fn the_queue_is_bounded() {
    let mut queue = PressQueue::default();
    for id in 0..CAPACITY {
        assert!(queue.push(u32::try_from(id).unwrap_or(u32::MAX)));
    }
    assert!(!queue.push(99));
    assert_eq!(queue.dropped(), 1);
    assert_eq!(queue.pop(), Some(0));
    assert!(queue.push(99));
}

#[test]
fn closing_wakes_the_reader_and_refuses_presses() {
    let mut queue = PressQueue::default();
    queue.push(1);
    let (count, waker) = waker();
    queue.register(&waker);
    queue.close();
    assert_eq!(count.0.load(Ordering::SeqCst), 1);
    assert!(queue.is_closed());
    assert!(!queue.push(2));
    // Unread presses are still read before the stream ends.
    assert_eq!(queue.pop(), Some(1));
    assert_eq!(queue.pop(), None);
}
