//! Simulator QA (test-support only): what the inline field and the gate
//! show now, sampled without querying GPUI's input handler.

use gpui::AnyWindowHandle;
use objc2::{DefinedClass, MainThreadMarker};

use super::controller::Hint;
use super::{Session, gate::Phase};
use crate::window::native_modal::{marked, window};

#[derive(Clone, Debug, PartialEq)]
pub struct NativeInlineFieldState {
    /// The gate's phase name ("Presenting", "Presented", ...), or "None".
    pub phase: String,
    pub session: Option<u64>,
    /// UIKit reports the field as first responder.
    pub field_first_responder: bool,
    pub gpui_text_first_responder: bool,
    pub gpui_drawing_first_responder: bool,
    /// The field holds marked (composing) text.
    pub marked: bool,
    /// The field's input mode language (`textInputMode.primaryLanguage`).
    pub primary_language: Option<String>,
    pub text: String,
    /// The field's box, in the drawing view's points (x, y, width, height).
    pub frame: (f32, f32, f32, f32),
    /// Nothing of the field is inside the clip; editing goes on.
    pub hidden: bool,
    /// The hint below the field: "", "empty" or "length".
    pub hint: String,
    /// `shouldChangeCharactersInRange:` calls while the field was marked.
    pub should_change_while_marked: u32,
    /// Touches the clear overlay took (outside taps).
    pub overlay_taps: u32,
    /// Input dropped at GPUI's side while the gate was shut.
    pub late_drops: u64,
}

pub fn native_inline_field_state(handle: AnyWindowHandle) -> Option<NativeInlineFieldState> {
    MainThreadMarker::new()?;
    let window = window(handle)?;
    let controller = window
        .native_search
        .borrow()
        .as_ref()
        .and_then(Session::inline)
        .and_then(|s| s.controller.clone());
    let state = window.state.borrow();
    let (session, phase) = match state.native_modal.phase() {
        Some((id, phase)) => (Some(id.0), phase_name(phase)),
        None => (None, "None"),
    };
    let field = controller.as_ref().map(|c| c.field.clone());
    let frame = controller.as_ref().map_or((0.0, 0.0, 0.0, 0.0), |c| {
        let (rect, _) = c.placed.get();
        // The overlay covers the window as the drawing view does.
        let shown = window.view.convertRect_fromView(
            objc2_core_foundation::CGRect::new(
                objc2_core_foundation::CGPoint::new(f64::from(rect.x), f64::from(rect.y)),
                objc2_core_foundation::CGSize::new(f64::from(rect.w), f64::from(rect.h)),
            ),
            Some(&c.overlay),
        );
        (
            shown.origin.x as f32,
            shown.origin.y as f32,
            shown.size.width as f32,
            shown.size.height as f32,
        )
    });
    Some(NativeInlineFieldState {
        phase: phase.into(),
        session,
        field_first_responder: field.as_ref().is_some_and(|f| f.isFirstResponder()),
        gpui_text_first_responder: window.text_input.view().isFirstResponder(),
        gpui_drawing_first_responder: window.view.isFirstResponder(),
        marked: field.as_ref().is_some_and(|f| marked(f)),
        primary_language: field
            .as_ref()
            .and_then(|f| f.textInputMode())
            .and_then(|mode| mode.primaryLanguage())
            .map(|language| language.to_string()),
        text: field
            .as_ref()
            .and_then(|f| f.text())
            .map(|t| t.to_string())
            .unwrap_or_default(),
        frame,
        hidden: controller.as_ref().is_some_and(|c| c.placed.get().1),
        hint: match controller.as_ref().map(|c| c.hint_shown()) {
            Some(Hint::Empty) => "empty",
            Some(Hint::Length) => "length",
            Some(Hint::None) | None => "",
        }
        .into(),
        should_change_while_marked: controller
            .as_ref()
            .map_or(0, |c| c.target.ivars().changes_while_marked.get()),
        overlay_taps: controller
            .as_ref()
            .map_or(0, |c| c.overlay.ivars().taps.get()),
        late_drops: state.native_modal.late_drops(),
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

/// Simulator QA: the field's Escape decision and close for presses with
/// `usages`, as its `pressesBegan:withEvent:` makes them before UITextField
/// sees the press. XCUITest's Escape never reaches a presented field, and an
/// injected press object cannot go on to UIKit's keyboard, so the press
/// itself is the device's to prove. Returns whether the field closed.
pub fn native_inline_field_press(handle: AnyWindowHandle, usages: &[isize]) -> bool {
    let Some(window) = MainThreadMarker::new().and(window(handle)) else {
        return false;
    };
    let field = window
        .native_search
        .borrow()
        .as_ref()
        .and_then(Session::inline)
        .and_then(|s| s.controller.as_ref().map(|c| c.field.clone()));
    let Some(field) = field else {
        return false;
    };
    let closes = field.escape_closes(usages);
    if closes {
        field.close_on_escape();
    }
    closes
}
