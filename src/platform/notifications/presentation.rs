//! Bounded lifetime of the predicate used at the final native display decision.
use super::NotificationError;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(super) struct Ticket {
    valid: AtomicBool,
    current: Arc<dyn Fn() -> bool + Send + Sync>,
}
impl Ticket {
    pub(super) fn current(&self) -> bool {
        // Check again after calling foreign code: it may itself invalidate us.
        self.valid.load(Ordering::Acquire) && (self.current)() && self.valid.load(Ordering::Acquire)
    }
    fn invalidate(&self) {
        self.valid.store(false, Ordering::Release);
    }
}

pub(super) struct Registry(Mutex<Vec<(String, Arc<Ticket>)>>);
impl Registry {
    pub(super) const fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }
    pub(super) fn insert(
        &self,
        tag: &str,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<Arc<Ticket>, NotificationError> {
        let mut entries = self.0.try_lock().map_err(|_| NotificationError::Busy)?;
        if entries.len() >= 64 || entries.iter().any(|(id, _)| id == tag) {
            return Err(NotificationError::Busy);
        }
        let ticket = Arc::new(Ticket {
            valid: AtomicBool::new(true),
            current,
        });
        entries.push((tag.to_owned(), ticket.clone()));
        Ok(ticket)
    }
    pub(super) fn get(&self, tag: &str) -> Option<Arc<Ticket>> {
        self.0
            .try_lock()
            .ok()?
            .iter()
            .find_map(|(id, ticket)| (id == tag).then(|| ticket.clone()))
    }
    pub(super) fn remove(&self, tags: &[String]) {
        // This mutex protects at most 64 small entries and never calls user code.
        // Invalidation completes before scheduling native removal on the main queue.
        let mut entries = self.0.lock().unwrap_or_else(|e| e.into_inner());
        entries.retain(|(id, ticket)| {
            if tags.iter().take(64).any(|tag| tag == id) {
                ticket.invalidate();
                false
            } else {
                true
            }
        });
    }
    pub(super) fn finish(&self, tag: &str, ticket: &Arc<Ticket>) {
        ticket.invalidate();
        let mut entries = self.0.lock().unwrap_or_else(|e| e.into_inner());
        entries.retain(|(id, current)| id != tag || !Arc::ptr_eq(current, ticket));
    }
}
