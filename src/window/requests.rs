//! New-scene requests the system has not answered yet. UIKit does not say
//! which request a connecting scene answers, so scenes are taken to answer
//! them in the order they were made.

use std::collections::VecDeque;

/// One request, so the system's refusal of it can find it again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RequestId(u64);

/// What a scene that just connected is for.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Answer<W> {
    /// No request is outstanding: the launch scene, one the user opened from
    /// the app switcher, or one the system restored. It waits for a window.
    Unrequested,
    /// The oldest request's window, which the scene hosts.
    Window(W),
    /// The oldest request's window closed before its scene came, so the scene
    /// is closed too rather than left blank.
    Unwanted,
}

impl<W> Answer<W> {
    /// Leaves launch/reopen scenes available, but closes unsolicited extras.
    pub(super) fn with_existing_windows(self, has_windows: bool) -> Self {
        match self {
            Self::Unrequested if has_windows => Self::Unwanted,
            answer => answer,
        }
    }
}

#[derive(Debug)]
pub(super) struct Requests<W> {
    next: u64,
    /// Oldest first. A request whose window closed keeps its place with no
    /// window, so the scene that answers it is not given to a later window.
    pending: VecDeque<(RequestId, Option<W>)>,
}

impl<W> Default for Requests<W> {
    fn default() -> Self {
        Self {
            next: 0,
            pending: VecDeque::new(),
        }
    }
}

impl<W> Requests<W> {
    /// Records a request made for `window`.
    pub(super) fn push(&mut self, window: W) -> RequestId {
        let id = RequestId(self.next);
        self.next += 1;
        self.pending.push_back((id, Some(window)));
        id
    }

    /// A scene connected, answering the oldest request.
    pub(super) fn answer(&mut self) -> Answer<W> {
        match self.pending.pop_front() {
            None => Answer::Unrequested,
            Some((_, Some(window))) => Answer::Window(window),
            Some((_, None)) => Answer::Unwanted,
        }
    }

    /// The system refused request `id`, so no scene will answer it. Returns
    /// its window, if that is still waiting.
    pub(super) fn refuse(&mut self, id: RequestId) -> Option<W> {
        let index = self
            .pending
            .iter()
            .position(|(pending, _)| *pending == id)?;
        self.pending.remove(index)?.1
    }

    /// Forgets the windows `closed` matches. Their requests stay in line, and
    /// the scenes that answer them are unwanted.
    pub(super) fn forget(&mut self, mut closed: impl FnMut(&W) -> bool) {
        for (_, window) in &mut self.pending {
            if window.as_ref().is_some_and(&mut closed) {
                *window = None;
            }
        }
    }
}
