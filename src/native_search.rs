//! A native search panel: a UIKit view controller with a `UISearchTextField`
//! and a results table, presented over a window as a popover or a sheet.
//!
//! The panel's field owns the keyboard while it is up. Keys, composition,
//! Paste and dictation go to the field and never to GPUI. Until the session
//! ends, the window's native modal gate keeps GPUI's views from taking first
//! responder and drops whatever would reach GPUI's input handler, its menus
//! or its Edit commands (`docs/native-search.md`).
//!
//! A session sends, in order: `Opened` (the field is first responder) or
//! `Failed`; any number of `QueryChanged` (committed text only, the newest
//! replacing an unread one), `ClearRequested` and `Selected`; one
//! `Dismissed`; and, after the handle is dropped and a GPUI view is first
//! responder again (or no view took it after a few attempts, `None`), one
//! `InputRestored`. `set_query` replaces the field's text, for example after
//! `ClearRequested`, without a `QueryChanged` echoing it. Every call is
//! main-thread only.

use std::{
    cell::RefCell,
    marker::PhantomData,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

use gpui::{AnyWindowHandle, Bounds, Pixels, Rgba};
use objc2::MainThreadMarker;

use crate::window::native_modal::{self as modal, Coalesce, Queue, SessionId};
pub use crate::window::native_modal::{
    DismissReason as NativeSearchDismissReason, PresentFailure as NativeSearchFailure,
    Responder as RestoredResponder,
};

/// Where the panel appears. A consumer picks `Sheet` for narrow windows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NativeSearchPresentation {
    /// A popover anchored at `NativeSearchRequest::anchor`, without arrow,
    /// kept a popover in compact width too.
    #[default]
    Popover,
    /// A page sheet with medium and large detents. Its background is dimmed
    /// at every detent (no `largestUndimmedDetentIdentifier`) and takes no
    /// touches: a tap there neither closes the sheet nor reaches the window.
    Sheet,
}

/// Colors and point sizes; everything else is UIKit's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeSearchStyle {
    pub background: Rgba,
    pub field_background: Rgba,
    pub text: Rgba,
    pub secondary_text: Rgba,
    pub accent: Rgba,
    pub error: Rgba,
    pub title_size: f32,
    pub body_size: f32,
    pub caption_size: f32,
}

impl Default for NativeSearchStyle {
    fn default() -> Self {
        let rgb = |hex: u32| Rgba {
            r: ((hex >> 16) & 0xff) as f32 / 255.0,
            g: ((hex >> 8) & 0xff) as f32 / 255.0,
            b: (hex & 0xff) as f32 / 255.0,
            a: 1.0,
        };
        Self {
            background: rgb(0x2b2933),
            field_background: rgb(0x101419),
            text: rgb(0xd8dee9),
            secondary_text: rgb(0xacb2bb),
            accent: rgb(0x88c0d0),
            error: rgb(0xbf616a),
            title_size: 15.0,
            body_size: 13.0,
            caption_size: 12.0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NativeSearchItem {
    pub id: u64,
    pub title: String,
    pub detail: String,
    pub dot: Option<Rgba>,
    pub accessibility_label: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NativeSearchSection {
    pub title: String,
    pub items: Vec<NativeSearchItem>,
}

/// Shown in place of the table, with a button that sends `ClearRequested`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NativeSearchEmpty {
    pub message: String,
    pub clear_label: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NativeSearchResults {
    /// A line above the results ("3 spaces · 3 agents", or a hint).
    pub status: Option<String>,
    /// A line in the error color ("This space is no longer available.").
    pub message: Option<String>,
    pub sections: Vec<NativeSearchSection>,
    pub empty: Option<NativeSearchEmpty>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativeSearchRequest {
    /// The control the panel belongs to, in the drawing view's points.
    pub anchor: Bounds<Pixels>,
    pub presentation: NativeSearchPresentation,
    pub title: String,
    pub cancel_label: String,
    pub cancel_accessibility_label: String,
    pub placeholder: String,
    /// The field's initial text (a kept filter).
    pub query: String,
    /// Preferred content size in points; UIKit may clamp it.
    pub width: f32,
    pub height: f32,
    pub style: NativeSearchStyle,
    pub results: NativeSearchResults,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeSearchEvent {
    /// The panel is up and its field is first responder.
    Opened,
    /// The field's committed text changed (marked text excluded). Sent again
    /// with the final committed text before `Dismissed` when it differs.
    QueryChanged(String),
    /// The empty state's clear button.
    ClearRequested,
    /// A row was tapped. `revision` is the `update` that showed it (0 for the
    /// request's own results). The panel stays up: the consumer opens the
    /// target and calls `dismiss_selected`, or shows a message with `update`.
    Selected {
        id: u64,
        revision: u64,
    },
    Dismissed(NativeSearchDismissReason),
    /// The panel never became usable; no `Dismissed` follows.
    Failed(NativeSearchFailure),
    /// After the handle was dropped: which GPUI view took first responder.
    InputRestored(RestoredResponder),
}

impl Coalesce for NativeSearchEvent {
    fn is_query(&self) -> bool {
        matches!(self, Self::QueryChanged(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NativeSearchError {
    #[error("native search must be called on the main thread")]
    NotMainThread,
    #[error("the window is closed or not in a scene")]
    WindowUnavailable,
    #[error("the window already presents a native search panel")]
    AlreadyPresented,
    #[error("a text input retirement is in progress for this window")]
    RetirementInProgress,
    #[error("the session has ended")]
    Ended,
}

pub(crate) type SharedQueue = Rc<RefCell<Queue<NativeSearchEvent>>>;

/// The session's events, in order. A `futures::Stream` that ends after
/// `InputRestored`.
pub struct NativeSearchEvents {
    queue: SharedQueue,
}

impl NativeSearchEvents {
    /// The next event, if one is waiting.
    pub fn try_next(&mut self) -> Option<NativeSearchEvent> {
        self.queue.borrow_mut().pop()
    }
}

impl futures::Stream for NativeSearchEvents {
    type Item = NativeSearchEvent;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut queue = self.queue.borrow_mut();
        if let Some(event) = queue.pop() {
            return Poll::Ready(Some(event));
        }
        if queue.is_closed() {
            return Poll::Ready(None);
        }
        queue.register(cx.waker());
        Poll::Pending
    }
}

/// The consumer's hold on a session. Dropping it ends the session: the panel
/// is dismissed if still up, and the gate opens once a GPUI view is first
/// responder again (`InputRestored`). Hold it until a selection's target has
/// opened, so the terminal never takes input in between.
///
/// The handle stays on the main thread (it is neither `Send` nor `Sync`):
/// dropping it releases the session, which only the main thread can do, and
/// a drop elsewhere would leave the gate shut for good.
pub struct NativeSearchHandle {
    window: AnyWindowHandle,
    session: SessionId,
    _main_thread: PhantomData<Rc<()>>,
}

impl NativeSearchHandle {
    pub fn session(&self) -> u64 {
        self.session.0
    }

    /// Replaces the shown results; returns their revision for `Selected`.
    pub fn update(&self, results: NativeSearchResults) -> Result<u64, NativeSearchError> {
        MainThreadMarker::new().ok_or(NativeSearchError::NotMainThread)?;
        modal::update(self.window, self.session, results)
    }

    /// Replaces the field's text, e.g. after `ClearRequested`. The text
    /// becomes the session's last query, so no `QueryChanged` echoes it;
    /// before the panel is up it becomes the field's initial text.
    pub fn set_query(&self, query: &str) -> Result<(), NativeSearchError> {
        MainThreadMarker::new().ok_or(NativeSearchError::NotMainThread)?;
        modal::set_query(self.window, self.session, query)
    }

    /// Closes the panel with `DismissReason::Requested`.
    pub fn dismiss(&self) -> Result<(), NativeSearchError> {
        self.close(NativeSearchDismissReason::Requested)
    }

    /// Closes the panel after the consumer opened the selected `id`.
    pub fn dismiss_selected(&self, id: u64) -> Result<(), NativeSearchError> {
        self.close(NativeSearchDismissReason::Selected(id))
    }

    fn close(&self, reason: NativeSearchDismissReason) -> Result<(), NativeSearchError> {
        MainThreadMarker::new().ok_or(NativeSearchError::NotMainThread)?;
        modal::dismiss(self.window, self.session, reason)
    }
}

impl Drop for NativeSearchHandle {
    fn drop(&mut self) {
        if MainThreadMarker::new().is_some() {
            modal::release(self.window, self.session);
        }
    }
}

/// Presents the panel over `window`. The gate shuts now; UIKit is called on
/// the next main-queue turn, outside GPUI's update.
pub fn present_native_search(
    window: AnyWindowHandle,
    request: NativeSearchRequest,
) -> Result<(NativeSearchHandle, NativeSearchEvents), NativeSearchError> {
    let mtm = MainThreadMarker::new().ok_or(NativeSearchError::NotMainThread)?;
    let queue: SharedQueue = Rc::default();
    let session = modal::present(window, request, queue.clone(), mtm)?;
    Ok((
        NativeSearchHandle {
            window,
            session,
            _main_thread: PhantomData,
        },
        NativeSearchEvents { queue },
    ))
}

/// Whether `window`'s gate is shut (a session is presenting, up, closing or
/// restoring). `None` for a window that is not open.
pub fn native_modal_active(window: AnyWindowHandle) -> Option<bool> {
    modal::active(window, MainThreadMarker::new()?)
}

/// Simulator QA: the panel's and the gate's observable state.
#[cfg(all(target_abi = "sim", feature = "test-support"))]
pub use crate::window::native_modal::{NativeSearchState, native_search_state};

#[cfg(test)]
mod tests {
    use super::NativeSearchHandle;

    /// Compiles only while the handle is not `Send`: with `Send`, both
    /// implementations apply and the call is ambiguous.
    #[test]
    fn handle_stays_on_the_main_thread() {
        trait AmbiguousIfSend<A> {
            fn some_item() {}
        }
        impl<T: ?Sized> AmbiguousIfSend<()> for T {}
        impl<T: ?Sized + Send> AmbiguousIfSend<u8> for T {}
        <NativeSearchHandle as AmbiguousIfSend<_>>::some_item();
    }
}
