//! GPUI's software-keyboard requests while a window's gate is shut, kept for
//! when it opens. The live keyboard state belongs to the panel's field then:
//! its hide key, show and hide notifications never touch GPUI's dismissal or
//! minimized state, and GPUI's `show_soft_keyboard` / `hide_soft_keyboard`
//! take effect only after the restore, last request winning.
//!
//! std only: `tests/native_modal.rs` includes this file.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Request {
    #[default]
    None,
    Show,
    Hide,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Intent {
    request: Request,
    /// Keyboard notifications dropped while shut, for diagnostics.
    pub dropped_notifications: u32,
}

impl Intent {
    pub fn show(&mut self) {
        self.request = Request::Show;
    }
    pub fn hide(&mut self) {
        self.request = Request::Hide;
    }
    pub fn notification(&mut self) {
        self.dropped_notifications = self.dropped_notifications.saturating_add(1);
    }
    /// The request to replay after the restore.
    pub fn take(&mut self) -> Request {
        std::mem::take(&mut self.request)
    }
}
