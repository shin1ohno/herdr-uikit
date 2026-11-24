//! The session's event queue: FIFO for everything that orders (selection,
//! dismissal, failure, restore), latest-state for the query, so a burst of
//! typing, pasting or clearing between two consumer turns leaves one pending
//! `QueryChanged` (the newest) in its place in the order. Its length is
//! bounded by the few ordering events a session can produce plus one query.
//!
//! std only (the waker is `std::task::Waker`): `tests/native_modal.rs`
//! includes this file.

use std::collections::VecDeque;
use std::task::Waker;

/// What a queue entry is, for coalescing.
pub trait Coalesce {
    /// Whether this entry is the latest-state query.
    fn is_query(&self) -> bool;
}

#[derive(Debug)]
pub struct Queue<E> {
    events: VecDeque<E>,
    waker: Option<Waker>,
    closed: bool,
    pub coalesced: u64,
}

impl<E> Default for Queue<E> {
    fn default() -> Self {
        Self {
            events: VecDeque::new(),
            waker: None,
            closed: false,
            coalesced: 0,
        }
    }
}

impl<E: Coalesce> Queue<E> {
    /// Appends `event`; a query replaces a query still pending at the back,
    /// so it never overtakes or falls behind an ordering event.
    pub fn push(&mut self, event: E) {
        if self.closed {
            return;
        }
        if event.is_query()
            && let Some(last) = self.events.back_mut()
            && last.is_query()
        {
            *last = event;
            self.coalesced += 1;
        } else {
            self.events.push_back(event);
        }
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    pub fn pop(&mut self) -> Option<E> {
        self.events.pop_front()
    }

    /// No more events will come; a waiting reader is woken to see the end.
    pub fn close(&mut self) {
        self.closed = true;
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn register(&mut self, waker: &Waker) {
        self.waker = Some(waker.clone());
    }
}
