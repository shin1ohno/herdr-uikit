//! Touches from `touchesBegan:withEvent:` and its siblings. Fingers and the
//! Pencil become raw `PlatformInput::Touch` events, from which GPUI's core
//! recognizes taps, pans, and long presses itself. Trackpad and mouse clicks
//! arrive as touches of type `UITouchTypeIndirectPointer` and become mouse
//! events instead, as a pointer would on a desktop.

use gpui::{
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, PlatformInput, Point, TouchEvent,
    TouchId, TouchPhase,
};
use objc2::{Message, rc::Retained};
use objc2_foundation::NSSet;
use objc2_ui_kit::{UIEvent, UITouch, UITouchType};

use super::{convert, state::Window};

/// Which of UIKit's four touch callbacks delivered a set of touches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TouchStage {
    Began,
    Moved,
    Ended,
    Cancelled,
}

impl Window {
    pub(super) fn handle_touches(
        &self,
        touches: &NSSet<UITouch>,
        event: Option<&UIEvent>,
        stage: TouchStage,
    ) {
        for touch in touches.iter() {
            if touch.r#type() == UITouchType::IndirectPointer {
                self.pointer_touch(&touch, event, stage);
            } else {
                self.direct_touch(&touch, event, stage);
            }
        }
    }

    fn direct_touch(&self, touch: &UITouch, event: Option<&UIEvent>, stage: TouchStage) {
        let key = TouchKey::new(touch);
        let position = self.location(touch);
        let (id, phase) = {
            let mut state = self.state.borrow_mut();
            let touches = &mut state.touches;
            if stage != TouchStage::Began && touches.get(&key).is_none() {
                return;
            }
            // A touch that began before this view saw it (in another view, or
            // before the window attached) has no `Started` in GPUI, so its
            // later phases are dropped rather than invented.
            let (id, phase) = match stage {
                TouchStage::Began => (touches.start(key), TouchPhase::Started),
                TouchStage::Moved => match touches.get(&key) {
                    Some(id) => (id, TouchPhase::Moved),
                    None => return,
                },
                TouchStage::Ended => match touches.end(&key) {
                    Some(id) => (id, TouchPhase::Ended),
                    None => return,
                },
                TouchStage::Cancelled => match touches.end(&key) {
                    Some(id) => (id, TouchPhase::Cancelled),
                    None => return,
                },
            };
            (id, phase)
        };
        let predicted_position = match (stage, event) {
            (TouchStage::Moved, Some(event)) => self.predicted_position(touch, event),
            _ => None,
        };
        let maximum_force = touch.maximumPossibleForce();
        let force =
            (maximum_force > 0.0).then(|| (touch.force() / maximum_force).clamp(0.0, 1.0) as f32);
        self.dispatch(PlatformInput::Touch(TouchEvent {
            id,
            phase,
            position,
            predicted_position,
            force,
        }));
    }

    /// Where UIKit predicts the touch will be about a frame from now.
    fn predicted_position(&self, touch: &UITouch, event: &UIEvent) -> Option<Point<Pixels>> {
        let predicted = event.predictedTouchesForTouch(touch)?.lastObject()?;
        Some(self.location(&predicted))
    }

    fn pointer_touch(&self, touch: &UITouch, event: Option<&UIEvent>, stage: TouchStage) {
        let location = touch.locationInView(Some(&self.view));
        let position = convert::point(location);
        let click_count = touch.tapCount();
        let input = {
            let mut state = self.state.borrow_mut();
            if let Some(event) = event {
                let flags = event.modifierFlags();
                state.modifiers = convert::modifiers(flags);
                state.capslock = convert::capslock(flags);
            }
            state.mouse_position = position;
            let modifiers = state.modifiers;
            match stage {
                TouchStage::Began => {
                    let button = event
                        .map(|event| convert::mouse_button(event.buttonMask()))
                        .unwrap_or_default();
                    state.pressed_button = Some(button);
                    // The press ended hovering; the pointer has not left.
                    state.hover_ending = false;
                    PlatformInput::MouseDown(MouseDownEvent {
                        button,
                        position,
                        modifiers,
                        click_count,
                        first_mouse: false,
                    })
                }
                TouchStage::Moved => PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    pressed_button: state.pressed_button,
                    modifiers,
                }),
                // GPUI has no cancelled click; releasing where the pointer is
                // ends any drag the press started.
                TouchStage::Ended | TouchStage::Cancelled => PlatformInput::MouseUp(MouseUpEvent {
                    button: state.pressed_button.take().unwrap_or_default(),
                    position,
                    modifiers,
                    click_count,
                }),
            }
        };
        let released = matches!(input, PlatformInput::MouseUp(_));
        self.dispatch(input);
        // Released over the view, the pointer hovers again and the hover
        // recognizer says so. Released outside it, nothing will: it left.
        if released && !convert::contains(self.view.bounds(), location) {
            self.pointer_left();
        }
    }

    /// A touch's position in the drawing view, which fills the window. The
    /// precise location is the Pencil's sub-point position, and equals the
    /// plain location for a finger.
    fn location(&self, touch: &UITouch) -> Point<Pixels> {
        convert::point(touch.preciseLocationInView(Some(&self.view)))
    }
}

/// A `UITouch` by identity. UIKit keeps one object per touch for the touch's
/// whole life, and retaining it here keeps its address from being reused by a
/// later touch while it is still in the table.
pub(super) struct TouchKey(Retained<UITouch>);

impl TouchKey {
    pub(super) fn new(touch: &UITouch) -> Self {
        Self(touch.retain())
    }
}

impl PartialEq for TouchKey {
    fn eq(&self, other: &Self) -> bool {
        Retained::as_ptr(&self.0) == Retained::as_ptr(&other.0)
    }
}

/// GPUI's ids for the touches in progress. Every touch gets a fresh id from
/// a counter that only grows, so an id is never reused for a later touch even
/// when UIKit reuses the touch object's memory.
#[derive(Debug)]
pub(super) struct TouchIds<K> {
    live: Vec<(K, TouchId)>,
    next: u64,
}

impl<K> Default for TouchIds<K> {
    fn default() -> Self {
        Self {
            live: Vec::new(),
            next: 1,
        }
    }
}

impl<K: PartialEq> TouchIds<K> {
    /// Assigns a fresh id to a touch that began.
    pub(super) fn start(&mut self, key: K) -> TouchId {
        let id = TouchId(self.next);
        self.next += 1;
        self.live.retain(|(live, _)| *live != key);
        self.live.push((key, id));
        id
    }

    pub(super) fn get(&self, key: &K) -> Option<TouchId> {
        self.live
            .iter()
            .find_map(|(live, id)| (live == key).then_some(*id))
    }

    /// Forgets a touch that ended or was cancelled, returning its id.
    pub(super) fn end(&mut self, key: &K) -> Option<TouchId> {
        let index = self.live.iter().position(|(live, _)| live == key)?;
        Some(self.live.swap_remove(index).1)
    }

    /// Forgets every touch, as when the window leaves its scene. Ids keep
    /// counting from where they were.
    pub(super) fn clear(&mut self) {
        self.live.clear();
    }
}
