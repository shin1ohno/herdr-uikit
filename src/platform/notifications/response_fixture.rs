//! Source-controlled adapter fixture. No native center, prompt or OS receipt.
use super::*;
use std::time::Instant;

/// Isolated registries calling the exact production response/display boundary.
/// Available only with test-support; injected timestamps never alter production.
pub struct NotificationResponseFixture {
    presentations: presentation::Registry,
    responses: response::Registry,
}
impl Default for NotificationResponseFixture {
    fn default() -> Self {
        Self {
            presentations: presentation::Registry::new(),
            responses: response::Registry::new(),
        }
    }
}
impl NotificationResponseFixture {
    /// Models successful submission after the actual permission/display guards.
    pub fn issue(
        &self,
        tag: &str,
        now: Instant,
        authorization: Authorization,
        current: impl Fn() -> bool + Send + Sync + 'static,
    ) -> NotificationResult {
        require_authorization(|| authorization)?;
        if self.responses.owns(tag, now) {
            return Err(NotificationError::Busy);
        }
        let ticket = self.presentations.insert(tag, Arc::new(current))?;
        if !ticket.current() {
            self.presentations.finish(tag, &ticket);
            return Err(NotificationError::Cancelled);
        }
        self.responses.issue(tag, &ticket, now);
        Ok(())
    }
    /// The same final display decision as willPresent, not actual OS display.
    pub fn presents(&self, tag: &str) -> bool {
        let ticket = self.presentations.get(tag);
        let mut shows = false;
        complete_presentation(ticket.as_deref(), |options| shows = !options.is_empty());
        shows
    }
    /// Synchronous display invalidation; no native removal is performed.
    pub fn remove(&self, tag: &str) {
        self.presentations.remove(&[tag.to_owned()]);
    }
    /// Calls the delegate's production dispatch with an injected receipt time.
    pub fn respond(
        &self,
        tag: &str,
        action: &str,
        now: Instant,
        callback: impl FnOnce(SystemNotificationResponse),
    ) -> bool {
        response::dispatch(
            &self.presentations,
            &self.responses,
            tag,
            action,
            now,
            callback,
        )
    }
}
