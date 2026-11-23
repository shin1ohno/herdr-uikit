//! Simulator QA (test-support only): what the panel and the gate show now,
//! sampled without querying GPUI's input handler.

use gpui::AnyWindowHandle;
use objc2::MainThreadMarker;

use super::{Phase, panel, window};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeSearchState {
    /// The gate's phase name ("Presenting", "Presented", ...), or "None".
    pub phase: String,
    pub session: Option<u64>,
    /// UIKit reports the panel's field as first responder.
    pub field_first_responder: bool,
    pub gpui_text_first_responder: bool,
    pub gpui_drawing_first_responder: bool,
    pub field_text: String,
    pub field_marked: bool,
    /// `UIModalPresentationStyle` of the presented controller (7 popover, 1 page sheet).
    pub presentation_style: Option<isize>,
    /// Input dropped at GPUI's side while the gate was shut.
    pub late_drops: u64,
    pub fenced_keys: usize,
    pub fence_overflow: bool,
    pub keyboard_notifications_dropped: u32,
    /// User dismissals UIKit began on the panel (a swipe, a tap outside),
    /// finished or abandoned.
    pub dismissals_begun: u32,
    /// Presses the panel's field began (all sessions) and the last one's
    /// HID usage (-1 without a key).
    pub field_presses: (u32, isize),
    /// What GPUI's text view checks before it acquires, besides the gate:
    /// the window active, a focused text element, the keyboard dismissal.
    pub text_acquire_inputs: String,
}

pub fn native_search_state(handle: AnyWindowHandle) -> Option<NativeSearchState> {
    MainThreadMarker::new()?;
    let window = window(handle)?;
    let panel = window
        .native_search
        .borrow()
        .as_ref()
        .and_then(|s| s.panel.clone());
    let state = window.state.borrow();
    let (session, phase) = match state.native_modal.phase() {
        Some((id, phase)) => (Some(id.0), phase_name(phase)),
        None => (None, "None"),
    };
    Some(NativeSearchState {
        phase: phase.into(),
        session,
        field_first_responder: panel.as_ref().is_some_and(|p| p.field.isFirstResponder()),
        gpui_text_first_responder: window.text_input.view().isFirstResponder(),
        gpui_drawing_first_responder: window.view.isFirstResponder(),
        field_text: panel
            .as_ref()
            .and_then(|p| p.field.text())
            .map(|t| t.to_string())
            .unwrap_or_default(),
        field_marked: panel.as_ref().is_some_and(|p| panel::marked(&p.field)),
        presentation_style: panel
            .as_ref()
            .filter(|p| p.controller.presentingViewController().is_some())
            .map(|p| p.controller.modalPresentationStyle().0),
        late_drops: state.modal_late_drops,
        fenced_keys: state.press_fence.len(),
        fence_overflow: state.press_fence.overflowed(),
        keyboard_notifications_dropped: state.modal_intent.dropped_notifications,
        dismissals_begun: panel.as_ref().map_or(0, |p| p.dismissals_begun()),
        field_presses: panel::FIELD_PRESSES.get(),
        text_acquire_inputs: format!(
            "active={} focused={} dismissal={:?}",
            state.is_active, state.text_input_focused, state.keyboard.dismissal
        ),
    })
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Presenting { .. } => "Presenting",
        Phase::Presented { focused: false } => "PresentedUnfocused",
        Phase::Presented { focused: true } => "Presented",
        Phase::Dismissing { .. } => "Dismissing",
        Phase::AwaitingConsumer => "AwaitingConsumer",
        Phase::Restoring => "Restoring",
    }
}
