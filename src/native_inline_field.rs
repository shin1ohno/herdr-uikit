//! A native one-line field laid over a rectangle of the window, for editing
//! a short name in place (a row's or a tab's label).
//!
//! A clear full-screen view controller is presented over the window without
//! animation; inside it, a `UITextField` sits over the given rectangle and a
//! one-line hint below it. Nothing is dimmed: GPUI keeps drawing everything
//! around the field. A tap anywhere outside the field closes it and reaches
//! nothing else.
//!
//! The field shares the window's native modal gate with the native search
//! panel (`docs/native-search.md`): from `present_native_inline_field` until
//! `InputRestored`, GPUI's views do not take first responder, and no key,
//! composition, Paste, menu command or held key's release reaches GPUI. The
//! search panel and the field are never up together: whichever comes second
//! gets `AlreadyPresented`.
//!
//! A session sends, in order: `Opened` (the field is first responder); at
//! most one `Committed` (Return with a changed name); exactly one
//! `Dismissed`; and, after the handle is dropped and a GPUI view is first
//! responder again, one `InputRestored`. A field that never became first
//! responder sends `Dismissed(PresentationFailed(_))` without `Opened`.
//! Return while composing only commits the composition; Escape while
//! composing only cancels it. Every call is main-thread only.

use std::{
    cell::RefCell,
    marker::PhantomData,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

use gpui::{AnyWindowHandle, Bounds, Pixels, Rgba};
use objc2::MainThreadMarker;

use crate::window::native_inline_field::{self as inline, Bounded};
pub use crate::window::native_inline_field::{NativeInlineDismissReason, NativeInlineFieldEvent};
use crate::window::native_modal::SessionId;

/// The field's look. Sizes are points; `font_weight` is UIKit's
/// `UIFontWeight` (0 regular, 0.3 semibold, 0.4 bold).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeInlineFieldStyle {
    pub font_size: f32,
    pub font_weight: f32,
    pub text: Rgba,
    pub placeholder: Rgba,
    pub background: Rgba,
    pub border: Rgba,
    pub border_width: f32,
    pub corner_radius: f32,
    /// Text inset from the field's edges.
    pub padding_h: f32,
    pub padding_v: f32,
    /// Caret and selection color.
    pub tint: Rgba,
    pub hint_color: Rgba,
    pub hint_size: f32,
    /// Space between the field and the hint below it.
    pub hint_gap: f32,
    /// How far the field extends past the label on each side.
    pub outset_h: f32,
    pub outset_v: f32,
    pub min_width: f32,
}

impl Default for NativeInlineFieldStyle {
    fn default() -> Self {
        let rgb = |hex: u32| Rgba {
            r: ((hex >> 16) & 0xff) as f32 / 255.0,
            g: ((hex >> 8) & 0xff) as f32 / 255.0,
            b: (hex & 0xff) as f32 / 255.0,
            a: 1.0,
        };
        Self {
            font_size: 14.0,
            font_weight: 0.0,
            text: rgb(0xd8dee9),
            placeholder: rgb(0x6b7080),
            background: rgb(0x101419),
            border: rgb(0x88c0d0),
            border_width: 1.0,
            corner_radius: 4.0,
            padding_h: 4.0,
            padding_v: 2.0,
            tint: rgb(0x88c0d0),
            hint_color: rgb(0xbf616a),
            hint_size: 12.0,
            hint_gap: 4.0,
            outset_h: 4.0,
            outset_v: 2.0,
            min_width: 120.0,
        }
    }
}

/// The accessibility identifiers of the field, the clear overlay and the
/// hint, so a consumer's QA can press them by its own names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeInlineFieldIdentifiers {
    pub field: String,
    pub overlay: String,
    pub hint: String,
}

impl Default for NativeInlineFieldIdentifiers {
    fn default() -> Self {
        Self {
            field: "native-inline-field".into(),
            overlay: "native-inline-field-overlay".into(),
            hint: "native-inline-field-hint".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativeInlineFieldRequest {
    /// The label the field covers, in the drawing view's points.
    pub frame: Bounds<Pixels>,
    /// The visible part of the label's scroll area; outside it the field
    /// hides (and keeps editing).
    pub clip: Option<Bounds<Pixels>>,
    pub text: String,
    /// Select the whole text on open, so typing replaces it.
    pub select_all: bool,
    pub placeholder: String,
    pub accessibility_label: String,
    /// Whether Return may commit an empty name (`Committed("")`); when not,
    /// an empty Return keeps the field and shows `empty_hint`.
    pub allow_empty: bool,
    /// The most characters (Rust `char`s) the name may have; longer input is
    /// cut once it is not composing, with `length_hint` shown.
    pub max_chars: usize,
    pub empty_hint: String,
    pub length_hint: String,
    pub style: NativeInlineFieldStyle,
    pub identifiers: NativeInlineFieldIdentifiers,
}

impl Default for NativeInlineFieldRequest {
    fn default() -> Self {
        Self {
            frame: Bounds::default(),
            clip: None,
            text: String::new(),
            select_all: true,
            placeholder: String::new(),
            accessibility_label: String::new(),
            allow_empty: false,
            max_chars: 160,
            empty_hint: String::new(),
            length_hint: String::new(),
            style: NativeInlineFieldStyle::default(),
            identifiers: NativeInlineFieldIdentifiers::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NativeInlineFieldError {
    #[error("the native inline field must be called on the main thread")]
    NotMainThread,
    #[error("the window is closed or not in a scene")]
    WindowUnavailable,
    #[error("the window already presents a native search panel or inline field")]
    AlreadyPresented,
    #[error("a text input retirement is in progress for this window")]
    RetirementInProgress,
    #[error("the session has ended")]
    Ended,
}

pub(crate) type InlineQueue = Rc<RefCell<Bounded<NativeInlineFieldEvent>>>;

/// The session's events, in order. A `futures::Stream` that ends after
/// `InputRestored`.
pub struct NativeInlineFieldEvents {
    queue: InlineQueue,
}

impl NativeInlineFieldEvents {
    /// The next event, if one is waiting.
    pub fn try_next(&mut self) -> Option<NativeInlineFieldEvent> {
        self.queue.borrow_mut().pop()
    }
}

impl futures::Stream for NativeInlineFieldEvents {
    type Item = NativeInlineFieldEvent;
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

/// The consumer's hold on a session. Dropping it ends the session: the field
/// is dismissed if still up, and the gate opens once a GPUI view is first
/// responder again (`InputRestored`). Drop it only after acting on
/// `Committed`, so the terminal never takes input in between.
///
/// Main-thread only (neither `Send` nor `Sync`), like the search handle.
pub struct NativeInlineFieldHandle {
    window: AnyWindowHandle,
    session: SessionId,
    _main_thread: PhantomData<Rc<()>>,
}

impl NativeInlineFieldHandle {
    pub fn session(&self) -> u64 {
        self.session.0
    }

    /// Moves the field over `frame` (the label's new rectangle) inside
    /// `clip`, on the next main-queue turn: at most one frame late.
    pub fn set_frame(
        &self,
        frame: Bounds<Pixels>,
        clip: Option<Bounds<Pixels>>,
    ) -> Result<(), NativeInlineFieldError> {
        MainThreadMarker::new().ok_or(NativeInlineFieldError::NotMainThread)?;
        inline::set_frame(self.window, self.session, frame, clip)
    }

    /// Closes the field with `Dismissed(Requested)`; nothing is committed.
    pub fn dismiss(&self) -> Result<(), NativeInlineFieldError> {
        MainThreadMarker::new().ok_or(NativeInlineFieldError::NotMainThread)?;
        inline::dismiss(self.window, self.session)
    }
}

impl Drop for NativeInlineFieldHandle {
    fn drop(&mut self) {
        if MainThreadMarker::new().is_some() {
            crate::window::native_modal::release(self.window, self.session);
        }
    }
}

/// Presents the field over `window`. The gate shuts now; UIKit is called on
/// the next main-queue turn, outside GPUI's update.
pub fn present_native_inline_field(
    window: AnyWindowHandle,
    request: NativeInlineFieldRequest,
) -> Result<(NativeInlineFieldHandle, NativeInlineFieldEvents), NativeInlineFieldError> {
    MainThreadMarker::new().ok_or(NativeInlineFieldError::NotMainThread)?;
    let queue: InlineQueue = Rc::default();
    let session = inline::present(window, request, queue.clone())?;
    Ok((
        NativeInlineFieldHandle {
            window,
            session,
            _main_thread: PhantomData,
        },
        NativeInlineFieldEvents { queue },
    ))
}

/// Simulator QA: the field's observable state.
#[cfg(all(target_abi = "sim", feature = "test-support"))]
pub use crate::window::native_inline_field::{
    NativeInlineFieldState, native_inline_field_press, native_inline_field_state,
};

#[cfg(test)]
mod tests {
    use super::NativeInlineFieldHandle;

    /// Compiles only while the handle is not `Send`.
    #[test]
    fn handle_stays_on_the_main_thread() {
        trait AmbiguousIfSend<A> {
            fn some_item() {}
        }
        impl<T: ?Sized> AmbiguousIfSend<()> for T {}
        impl<T: ?Sized + Send> AmbiguousIfSend<u8> for T {}
        <NativeInlineFieldHandle as AmbiguousIfSend<_>>::some_item();
    }
}
