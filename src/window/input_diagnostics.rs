//! Static codes that say why hardware text input reaches, or misses, the
//! text input view, and the decisions they describe. A consumer that
//! subscribes to the `herdr_uikit::diagnostics` tracing target receives one
//! `code` field per event, always one of [`CODES`]: an enum value, never a
//! key, character, text, host or terminal content.
//!
//! The module is std only, so its decisions run in host tests
//! (`tests/input_diagnostics.rs`) as well as on iOS.

use super::keyboard::Dismissal;

/// The tracing target the codes are emitted on.
pub const TARGET: &str = "herdr_uikit::diagnostics";

/// Every code this crate emits on [`TARGET`]. A consumer's allowlist must
/// contain each of these to record them.
pub const CODES: &[&str] = &[
    // keep-sorted start
    "dismissal_application",
    "dismissal_none_did_show",
    "dismissal_none_focus",
    "dismissal_none_press",
    "dismissal_none_show",
    "dismissal_user_resign",
    "dismissal_user_will_hide",
    "keyboard_will_hide",
    "press_dropped_no_text",
    "press_recovered_text",
    "responder_drawing_app_dismissed",
    "responder_drawing_inactive",
    "responder_drawing_unfocused",
    "responder_drawing_user_dismissed",
    "responder_text",
    "text_focus_gained",
    "text_focus_lost",
    // keep-sorted end
];

/// Why the drawing view is first responder instead of the text input view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DrawingCause {
    /// The scene is not active.
    Inactive,
    /// GPUI's focused element does not take text (`FocusLost`).
    Unfocused,
    /// The user dismissed the software keyboard.
    UserDismissed,
    /// The application dismissed the software keyboard.
    AppDismissed,
}

/// Which view `update_first_responder` makes first responder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Responder {
    Text,
    Drawing(DrawingCause),
}

impl Responder {
    /// The text input view takes first responder while the focused element
    /// takes text, the scene is active and no dismissal holds the keyboard
    /// back. The first failing condition, in that order, names the cause.
    pub(super) fn choose(text_input_focused: bool, is_active: bool, dismissal: Dismissal) -> Self {
        if !is_active {
            Self::Drawing(DrawingCause::Inactive)
        } else if !text_input_focused {
            Self::Drawing(DrawingCause::Unfocused)
        } else {
            match dismissal {
                Dismissal::None => Self::Text,
                Dismissal::User => Self::Drawing(DrawingCause::UserDismissed),
                Dismissal::Application => Self::Drawing(DrawingCause::AppDismissed),
            }
        }
    }

    pub(super) fn code(self) -> &'static str {
        match self {
            Self::Text => "responder_text",
            Self::Drawing(DrawingCause::Inactive) => "responder_drawing_inactive",
            Self::Drawing(DrawingCause::Unfocused) => "responder_drawing_unfocused",
            Self::Drawing(DrawingCause::UserDismissed) => "responder_drawing_user_dismissed",
            Self::Drawing(DrawingCause::AppDismissed) => "responder_drawing_app_dismissed",
        }
    }
}

/// What changed the keyboard's dismissal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DismissalCause {
    /// `UIKeyboardDidShowNotification` for the text input view.
    DidShow,
    /// GPUI's text focus moved.
    FocusChange,
    /// `PlatformWindow::hide_soft_keyboard`.
    Hide,
    /// A printable hardware press took the text input view back.
    Press,
    /// The text input view resigned without the window asking.
    Resign,
    /// `PlatformWindow::show_soft_keyboard`.
    Show,
    /// `UIKeyboardWillHideNotification` nobody asked for.
    WillHide,
}

/// The code for a dismissal transition, or none when it did not change.
pub(super) fn dismissal_code(
    before: Dismissal,
    after: Dismissal,
    cause: DismissalCause,
) -> Option<&'static str> {
    if before == after {
        return None;
    }
    Some(match (after, cause) {
        (Dismissal::Application, _) => "dismissal_application",
        (Dismissal::User, DismissalCause::Resign) => "dismissal_user_resign",
        (Dismissal::User, _) => "dismissal_user_will_hide",
        (Dismissal::None, DismissalCause::DidShow) => "dismissal_none_did_show",
        (Dismissal::None, DismissalCause::FocusChange) => "dismissal_none_focus",
        (Dismissal::None, DismissalCause::Press) => "dismissal_none_press",
        (Dismissal::None, _) => "dismissal_none_show",
    })
}

pub(super) fn text_focus_code(focused: bool) -> &'static str {
    if focused {
        "text_focus_gained"
    } else {
        "text_focus_lost"
    }
}

/// A hardware press that reached the text input view, as far as the
/// recovery decision needs it. No key or character is kept.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Press {
    /// It produces text: a character that is not a control character, with
    /// neither Command nor Control held.
    pub(super) printable: bool,
    /// The text input view is first responder; otherwise the press came
    /// from the drawing view.
    pub(super) text_first_responder: bool,
    /// GPUI's focused element takes text (`FocusGained`).
    pub(super) text_input_focused: bool,
    pub(super) is_active: bool,
    /// A native modal (search panel, inline field) holds the responder.
    pub(super) modal: bool,
}

/// What to do with a printable press that reached the text input view while
/// the drawing view is first responder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Recovery {
    /// Nothing to recover: the text input view is first responder, the press
    /// is not text, or the window is inactive or behind a native modal.
    None,
    /// GPUI's focused element takes text: take the text input view back.
    TakeText,
    /// No element takes text (a palette or a sheet is open); leave it.
    NoTextHandler,
}

impl Recovery {
    pub(super) fn decide(press: Press) -> Self {
        if press.text_first_responder || !press.printable || !press.is_active || press.modal {
            Self::None
        } else if press.text_input_focused {
            Self::TakeText
        } else {
            Self::NoTextHandler
        }
    }
}

/// Whether a key's text and modifiers make a printable press.
pub(super) fn is_printable(key_char: Option<&str>, command: bool, control: bool) -> bool {
    !command
        && !control
        && key_char.is_some_and(|text| !text.is_empty() && !text.chars().any(char::is_control))
}

/// Codes are emitted only when they change what a reader learns: a repeated
/// responder choice, or more than one dropped press while the same drawing
/// view interval lasts, is not emitted again.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Dedup {
    responder: Option<Responder>,
    dropped_reported: bool,
}

impl Dedup {
    /// The code for `choice`, if it differs from the last one emitted.
    pub(super) fn responder(&mut self, choice: Responder) -> Option<&'static str> {
        if self.responder == Some(choice) {
            return None;
        }
        self.responder = Some(choice);
        self.dropped_reported = false;
        Some(choice.code())
    }

    /// The code for `recovery`, at most one dropped press per interval.
    pub(super) fn recovery(&mut self, recovery: Recovery) -> Option<&'static str> {
        match recovery {
            Recovery::None => None,
            Recovery::TakeText => Some("press_recovered_text"),
            Recovery::NoTextHandler if self.dropped_reported => None,
            Recovery::NoTextHandler => {
                self.dropped_reported = true;
                Some("press_dropped_no_text")
            }
        }
    }
}
