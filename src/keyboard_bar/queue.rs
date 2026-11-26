//! The window's shortcuts bar presses, oldest first, for one consumer stream.
//! std only, so host tests can run it (`tests/keyboard_bar_queue.rs`).

use std::{collections::VecDeque, task::Waker};

/// At most this many unread presses are kept; a press beyond it is dropped
/// (a consumer that reads none for this long has stopped reading).
pub const CAPACITY: usize = 64;

#[derive(Debug, Default)]
pub struct PressQueue {
    presses: VecDeque<u32>,
    waker: Option<Waker>,
    closed: bool,
    dropped: u64,
}

impl PressQueue {
    /// Queues a press of item `id` and wakes the reader. Returns false when
    /// the press was dropped: the queue is closed or full.
    pub fn push(&mut self, id: u32) -> bool {
        if self.closed || self.presses.len() >= CAPACITY {
            self.dropped += 1;
            return false;
        }
        self.presses.push_back(id);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
        true
    }

    pub fn pop(&mut self) -> Option<u32> {
        self.presses.pop_front()
    }

    /// The window closed: no more presses, and the reader's stream ends
    /// after the unread ones.
    pub fn close(&mut self) {
        self.closed = true;
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// The reader waits; the next push or close wakes it. A second reader
    /// replaces the first one's waker.
    pub fn register(&mut self, waker: &Waker) {
        self.waker = Some(waker.clone());
    }

    /// Presses dropped since the queue was made.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}
