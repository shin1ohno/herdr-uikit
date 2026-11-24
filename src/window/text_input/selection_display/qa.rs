//! Simulator QA: what the selection display shows now, sampled without
//! querying GPUI's input handler, so a consumer can read it while it draws.
//! The accessory class names are UIKit's private ones and are read here for
//! QA only; nothing in the SDK's behaviour depends on them.

use gpui::AnyWindowHandle;
use objc2::{ClassType, DefinedClass, MainThreadMarker, rc::Retained, runtime::NSObjectProtocol};
use objc2_core_foundation::CGRect;
use objc2_ui_kit::{UITextSelectionDisplayInteraction, UIView};

use super::SelectionDisplay;
use crate::window::scene;
use crate::window::text_input::{geometry, view_class::GPUITextInputView};

/// UIKit's view for one input mode indicator ("A", "あ", "A かな あ"). Private.
const ACCESSORY_CLASS: &str = "_UICursorAccessoryView";

/// The terminal text view's selection display, for Simulator QA.
#[derive(Clone, Debug, PartialEq)]
pub struct InputModeIndicatorState {
    /// UIKit reports the window's text input view as first responder.
    pub text_first_responder: bool,
    /// The interaction's `isActivated`, as UIKit reports it.
    pub activated: bool,
    /// `UITextSelectionDisplayInteraction`s on the window's drawing view.
    pub interactions_on_drawing_view: usize,
    /// The interaction's `textInput` is the window's current text view.
    pub text_input_is_current: bool,
    /// The class of the interaction's cursor view.
    pub cursor_class: String,
    /// The first visible input mode indicator (non-zero size, not hidden,
    /// alpha above zero up to the drawing view), as x, y, width and height
    /// in drawing view points.
    pub accessory_frame: Option<[f64; 4]>,
    /// The caret GPUI last reported (`update_ime_position`), as
    /// `caretRectForPosition:` returns it when the handler is busy: x, y,
    /// width and height in drawing view points. `None` before GPUI reported
    /// one. The handler is not queried, so this is safe inside a GPUI update.
    pub caret_rect: Option<[f64; 4]>,
    /// How often UIKit was asked to re-read the selection.
    pub needs_update_calls: u64,
}

/// The selection display state of `handle`'s window, or `None` when the
/// window is not live or this is not the main thread.
pub fn input_mode_indicator_state(handle: AnyWindowHandle) -> Option<InputModeIndicatorState> {
    let mtm = MainThreadMarker::new()?;
    let window = scene::live_windows(mtm)
        .into_iter()
        .find(|window| window.handle == handle && !window.state.borrow().closed)?;
    Some(window.text_input.input_mode_indicator_state(&window.view))
}

pub(in crate::window) fn sample(
    view: &GPUITextInputView,
    display: Option<&SelectionDisplay>,
    drawing_view: &UIView,
) -> InputModeIndicatorState {
    let scale_factor = view.ivars().target.scale_factor();
    let interaction = display.map(|display| &display.interaction);
    InputModeIndicatorState {
        text_first_responder: view.isFirstResponder(),
        activated: interaction.is_some_and(|i| i.isActivated()),
        interactions_on_drawing_view: drawing_view
            .interactions()
            .iter()
            .filter(|i| i.isKindOfClass(UITextSelectionDisplayInteraction::class()))
            .count(),
        text_input_is_current: interaction
            .and_then(|i| i.textInput())
            .is_some_and(|input| {
                Retained::as_ptr(&input).cast::<()>() == std::ptr::from_ref(view).cast::<()>()
            }),
        cursor_class: interaction.map_or_else(String::new, |i| {
            i.cursorView().class().name().to_string_lossy().into_owned()
        }),
        accessory_frame: accessory(drawing_view, drawing_view).map(array),
        caret_rect: view
            .ivars()
            .caret
            .get()
            .map(|caret| array(geometry::caret(caret, scale_factor))),
        needs_update_calls: display.map_or(0, |display| display.needs_update_calls.get()),
    }
}

fn array(rect: CGRect) -> [f64; 4] {
    [
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
    ]
}

/// The first visible accessory view under `view`, in `root`'s coordinates.
fn accessory(view: &UIView, root: &UIView) -> Option<CGRect> {
    if view.isHidden() || view.alpha() <= 0.0 {
        return None;
    }
    if view.class().name().to_string_lossy() == ACCESSORY_CLASS {
        let frame = view.convertRect_toView(view.bounds(), Some(root));
        if frame.size.width > 0.0 && frame.size.height > 0.0 {
            return Some(frame);
        }
    }
    view.subviews()
        .iter()
        .find_map(|child| accessory(&child, root))
}
