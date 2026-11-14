//! Issued-tag receipts are independent of short-lived display eligibility.
use super::{SystemNotificationResponse, presentation};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

const LIMIT: usize = 64;
// Allows a late refusal beyond the consumer's action window. Never grants action.
const RETENTION: Duration = Duration::from_secs(600);
struct Receipt {
    tag: String,
    ticket: Weak<presentation::Ticket>,
    issued: Instant,
}
pub(super) struct Registry(Mutex<VecDeque<Receipt>>);
impl Registry {
    pub(super) const fn new() -> Self {
        Self(Mutex::new(VecDeque::new()))
    }
    pub(super) fn issue(&self, tag: &str, ticket: &Arc<presentation::Ticket>, now: Instant) {
        let mut entries = self.0.lock().unwrap_or_else(|e| e.into_inner());
        entries.retain(|entry| now.saturating_duration_since(entry.issued) < RETENTION);
        if entries.len() == LIMIT {
            entries.pop_front();
        }
        entries.push_back(Receipt {
            tag: tag.to_owned(),
            ticket: Arc::downgrade(ticket),
            issued: now,
        });
    }
    pub(super) fn forget(&self, tag: &str, ticket: &Arc<presentation::Ticket>) {
        let mut entries = self.0.lock().unwrap_or_else(|e| e.into_inner());
        entries.retain(|entry| entry.tag != tag || !entry.ticket.ptr_eq(&Arc::downgrade(ticket)));
    }
    pub(super) fn owns(&self, tag: &str, now: Instant) -> bool {
        let mut entries = self.0.lock().unwrap_or_else(|e| e.into_inner());
        entries.retain(|entry| now.saturating_duration_since(entry.issued) < RETENTION);
        entries.iter().any(|entry| entry.tag == tag)
    }
}

/// The delegate's main-queue boundary, also exercised by the isolated fixture.
/// A callback conveys a response, never permission to focus: the consumer must
/// consume its own record once and validate the current context before acting.
pub(super) fn dispatch(
    presentations: &presentation::Registry,
    responses: &Registry,
    tag: &str,
    action: &str,
    now: Instant,
    callback: impl FnOnce(SystemNotificationResponse),
) -> bool {
    if tag.is_empty() || tag.len() > 128 || action.len() > 128 {
        tracing::warn!("notification response rejected: InvalidPayload");
        return false;
    }
    if action == "com.apple.UNNotificationDismissActionIdentifier" {
        return false;
    }
    if !responses.owns(tag, now) {
        tracing::warn!("notification response rejected: UnownedOrExpired");
        return false;
    }
    // Do not call Ticket::current here: its display-age window is not the
    // consumer's response lifetime. Retain issuance for duplicate/stale status.
    presentations.remove(&[tag.to_owned()]);
    callback(SystemNotificationResponse {
        tag: tag.to_owned().into(),
        action_id: (action != "com.apple.UNNotificationDefaultActionIdentifier")
            .then(|| action.to_owned().into()),
    });
    true
}
