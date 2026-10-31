//! What one window's UIKit callbacks and GPUI's calls share.
//!
//! `IosWindow` owns the only strong `Rc<Window>`. The view's ivars, the
//! display link's callback, the input target, and the scene registry hold
//! `Weak`s, so callbacks UIKit delivers after GPUI closed the window find
//! nothing and do nothing.
//!
//! GPUI's callbacks call back into `PlatformWindow`, and some UIKit calls
//! (layout, first responder changes, `makeKeyAndVisible`) call back into the
//! view synchronously. So no `RefCell` here is borrowed across a callback or
//! such a UIKit call: callers copy what they need out of a short borrow.

use std::cell::RefCell;

use gpui::{
    AnyWindowHandle, Bounds, Capslock, DispatchEventResult, ForegroundExecutor, Modifiers,
    MouseButton, Pixels, PlatformInput, PlatformInputHandler, Point, RequestFrameOptions, Size,
    WindowAppearance, WindowBackgroundAppearance, WindowControlArea, WindowInsets,
    WindowVisibility,
};
use objc2::{MainThreadMarker, rc::Retained};
use objc2_quartz_core::CALayer;
use objc2_ui_kit::{UIViewController, UIWindow, UIWindowScene};

use super::{
    frame::InsetAnimation,
    text_input::TextInputView,
    text_state::TextState,
    touch::{TouchIds, TouchKey},
    view::GpuiView,
};
use crate::{display_link::DisplayLink, renderer::Renderer};

pub(super) struct Window {
    pub(super) callbacks: Callbacks,
    pub(super) controller: Retained<UIViewController>,
    /// `None` only until `open` installs it, and again after close.
    pub(super) display_link: RefCell<Option<DisplayLink>>,
    pub(super) executor: ForegroundExecutor,
    pub(super) handle: AnyWindowHandle,
    /// The renderer's `CAMetalLayer`, retained so its frame and scale can
    /// follow the view's.
    pub(super) metal_layer: Option<Retained<CALayer>>,
    pub(super) mtm: MainThreadMarker,
    pub(super) renderer: RefCell<Renderer>,
    pub(super) state: RefCell<State>,
    pub(super) text_input: TextInputView,
    pub(super) view: Retained<GpuiView>,
}

pub(super) struct State {
    pub(super) appearance: WindowAppearance,
    pub(super) background_appearance: WindowBackgroundAppearance,
    /// The window's frame in its scene, in points; the size is the content
    /// size, as the drawing view fills the window.
    pub(super) bounds: Bounds<Pixels>,
    pub(super) capslock: Capslock,
    /// GPUI dropped the window; nothing may draw or dispatch any more.
    pub(super) closed: bool,
    /// The next frame must bypass GPUI's view cache: the drawable was
    /// resized, rescaled, or may have been discarded in the background.
    pub(super) force_render: bool,
    /// GPUI asked for a frame since the last one ran.
    pub(super) frame_requested: bool,
    /// Hovering ended, and the pointer is taken to have left unless a click
    /// begins first (`pointer`).
    pub(super) hover_ending: bool,
    pub(super) hovered: bool,
    pub(super) input_handler: Option<PlatformInputHandler>,
    pub(super) insets: WindowInsets,
    pub(super) is_active: bool,
    pub(super) keyboard_animation: Option<InsetAnimation>,
    /// The software keyboard was dismissed on request while the focused
    /// element still takes text, so becoming active must not reopen it.
    pub(super) keyboard_dismissed: bool,
    /// The focused element's text state after the last frame, or `None`
    /// while no element takes text (`frame`, `text_state`).
    pub(super) last_text_state: Option<TextState>,
    pub(super) modifiers: Modifiers,
    pub(super) mouse_position: Point<Pixels>,
    /// The window is in a scene or waiting for one. Only a window opened
    /// hidden is not, until GPUI activates it.
    pub(super) placed: bool,
    /// The indirect pointer button held down, if any.
    pub(super) pressed_button: Option<MouseButton>,
    pub(super) scale_factor: f32,
    pub(super) scene: Option<Retained<UIWindowScene>>,
    /// GPUI's focused element takes text input.
    pub(super) text_input_focused: bool,
    pub(super) title: String,
    pub(super) touches: TouchIds<TouchKey>,
    pub(super) ui_window: Option<Retained<UIWindow>>,
    pub(super) visibility: WindowVisibility,
}

impl State {
    pub(super) fn new(bounds: Bounds<Pixels>, scale_factor: f32, title: String) -> Self {
        Self {
            appearance: WindowAppearance::default(),
            background_appearance: WindowBackgroundAppearance::Opaque,
            bounds,
            capslock: Capslock::default(),
            closed: false,
            force_render: true,
            frame_requested: false,
            hover_ending: false,
            hovered: false,
            input_handler: None,
            insets: WindowInsets::default(),
            is_active: false,
            keyboard_animation: None,
            keyboard_dismissed: false,
            last_text_state: None,
            modifiers: Modifiers::default(),
            mouse_position: Point::default(),
            placed: false,
            pressed_button: None,
            scale_factor,
            scene: None,
            text_input_focused: false,
            title,
            touches: TouchIds::default(),
            ui_window: None,
            visibility: WindowVisibility::Hidden,
        }
    }

    /// Whether a frame drawn now would be shown. Metal work submitted while
    /// the scene is in the background is refused, so drawing stops there.
    pub(super) fn can_draw(&self) -> bool {
        !self.closed && self.ui_window.is_some() && self.visibility.is_visible()
    }
}

type Slot<F> = RefCell<Option<Box<F>>>;

/// GPUI's callbacks for one window. Each is taken out of its slot while it
/// runs, so a callback that re-enters the window cannot alias itself.
#[derive(Default)]
pub(super) struct Callbacks {
    pub(super) active_status: Slot<dyn FnMut(bool)>,
    pub(super) appearance_changed: Slot<dyn FnMut()>,
    pub(super) close: Slot<dyn FnOnce()>,
    pub(super) hit_test_window_control: Slot<dyn FnMut() -> Option<WindowControlArea>>,
    pub(super) hover_status: Slot<dyn FnMut(bool)>,
    pub(super) input: Slot<dyn FnMut(PlatformInput) -> DispatchEventResult>,
    pub(super) insets_changed: Slot<dyn FnMut(WindowInsets)>,
    pub(super) moved: Slot<dyn FnMut()>,
    pub(super) request_frame: Slot<dyn FnMut(RequestFrameOptions)>,
    pub(super) resize: Slot<dyn FnMut(Size<Pixels>, f32)>,
    pub(super) should_close: Slot<dyn FnMut() -> bool>,
    pub(super) visibility: Slot<dyn FnMut(WindowVisibility)>,
}

/// Runs the callback in `slot`, if GPUI registered one, with no borrow held.
/// A callback GPUI registered while this one ran replaces it.
pub(super) fn call<F: ?Sized, R>(slot: &Slot<F>, f: impl FnOnce(&mut F) -> R) -> Option<R> {
    let mut callback = slot.borrow_mut().take()?;
    let result = f(&mut callback);
    slot.borrow_mut().get_or_insert(callback);
    Some(result)
}

/// Stores `callback` in `slot`, replacing any earlier one.
pub(super) fn register<F: ?Sized>(slot: &Slot<F>, callback: Box<F>) {
    *slot.borrow_mut() = Some(callback);
}
