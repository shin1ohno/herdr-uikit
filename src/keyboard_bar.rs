//! Keys over the software keyboard, in two places a consumer chooses
//! between: the keyboard's shortcuts bar (`UITextInputAssistantItem`'s
//! leading and trailing groups on the window's text input view,
//! [`set_keyboard_bar`]) and a full-width row in the text input view's
//! `inputAccessoryView` ([`set_keyboard_accessory`]). Also where the keyboard
//! is ([`keyboard_placement`]).
//!
//! UIKit shows the bar on a docked keyboard (and, by Apple's design, on a
//! split one), but not on a floating keyboard: there only the predictive
//! row is drawn. The accessory row shows with every software keyboard: above
//! a docked or split one, and full width at the screen's bottom edge below a
//! floating one (`docs/keyboard-bar.md`, measured on the iPadOS 27.2
//! Simulator).
//!
//! A press arrives on the window's [`KeyboardBarPresses`] stream as the
//! item's `id` (or a menu entry's, or an arrow's), after UIKit handled the
//! touch. Neither the bar nor the row takes first responder. Every call is
//! main-thread only.

pub(crate) mod arrow;
mod queue;

use std::{
    cell::RefCell,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

use gpui::AnyWindowHandle;
use objc2::MainThreadMarker;

pub use crate::window::KeyboardPlacement;
pub(crate) use queue::PressQueue;

/// One entry of an item's menu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyboardBarMenuEntry {
    /// Sent on [`KeyboardBarPresses`] when the entry is chosen.
    pub id: u32,
    pub title: String,
    /// An SF Symbol name shown beside the title, if UIKit has it.
    pub symbol: Option<String>,
    /// The entry's accessibility identifier (for UI tests).
    pub identifier: String,
}

/// One button on the bar.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyboardBarItem {
    /// Sent on [`KeyboardBarPresses`] when the button is tapped. Unused for
    /// an item with a `menu`: its tap opens the menu instead.
    pub id: u32,
    /// Shown when `symbol` is `None` or UIKit has no such symbol.
    pub title: String,
    /// An SF Symbol name to show instead of the title.
    pub symbol: Option<String>,
    pub accessibility_label: String,
    /// The button's accessibility identifier (for UI tests).
    pub identifier: String,
    /// Drawn selected, as a latched modifier.
    pub selected: bool,
    /// Entries of a menu the button opens on tap; empty for a plain button.
    pub menu: Vec<KeyboardBarMenuEntry>,
    /// Draws the item as one flick arrow key instead of a button: the arrow
    /// comes from where the finger is relative to the key's centre, the
    /// centre sends nothing, the arrow is sent when the finger lifts, and a
    /// held arrow repeats, faster farther from the centre
    /// (`keyboard_bar/arrow.rs`). `id`, `title`, `symbol` and `menu` are
    /// unused for it.
    pub arrows: Option<KeyboardArrows>,
}

/// The ids an arrow key sends on [`KeyboardBarPresses`], one per arrow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyboardArrows {
    pub up: u32,
    pub down: u32,
    pub left: u32,
    pub right: u32,
}

impl KeyboardArrows {
    pub(crate) fn id(&self, arrow: arrow::Arrow) -> u32 {
        match arrow {
            arrow::Arrow::Up => self.up,
            arrow::Arrow::Down => self.down,
            arrow::Arrow::Left => self.left,
            arrow::Arrow::Right => self.right,
        }
    }
}

/// The bar's buttons. Each inner list is one group; UIKit keeps a group's
/// buttons together. An empty bar shows no buttons at all, the system's
/// (undo, redo, paste) included, so a keyboard whose language shows no
/// candidates drops its top row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyboardBar {
    pub leading: Vec<Vec<KeyboardBarItem>>,
    pub trailing: Vec<Vec<KeyboardBarItem>>,
}

impl KeyboardBar {
    pub fn is_empty(&self) -> bool {
        self.leading.iter().chain(&self.trailing).all(Vec::is_empty)
    }
}

/// Puts `bar` on `window`'s shortcuts bar, replacing what was there. The
/// same bar again is a no-op, so a consumer may call this every frame. A
/// bar that differs only in `selected` updates its buttons in place, so a
/// touch on one (a held arrow key) is not cut off. Returns false off the
/// main thread or if the window no longer exists.
pub fn set_keyboard_bar(window: AnyWindowHandle, bar: &KeyboardBar) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    crate::window::set_keyboard_bar(window, bar, mtm)
}

/// A full-width row of equal-width keys over the software keyboard: a
/// keyboard-style `UIInputView` as the text input view's
/// `inputAccessoryView`. An item's `selected` is drawn as a latched key, a
/// `menu` opens on tap, `arrows` makes the flick arrow key. An empty row
/// collapses the accessory view to zero height. Presses arrive on
/// [`keyboard_bar_presses`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyboardAccessory {
    pub items: Vec<KeyboardBarItem>,
}

impl KeyboardAccessory {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Puts `row` in `window`'s `inputAccessoryView`, replacing what was there.
/// The same row again is a no-op, so a consumer may call this every frame.
/// A row whose keys keep their identifiers and kinds updates them in place,
/// so a touch on one (a held arrow key) is not cut off. The first non-empty
/// row installs the accessory view and reloads the text input view's input
/// views (`reloadInputViews`), but not while text is being composed: that
/// reload waits for the composition to end, so marked text is not
/// disturbed. From then on the view stays installed: an empty row collapses
/// it to zero height and a later row expands it, both without a reload.
/// Returns false off the main thread or if the window no longer exists.
pub fn set_keyboard_accessory(window: AnyWindowHandle, row: &KeyboardAccessory) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    crate::window::set_keyboard_accessory(window, row, mtm)
}

/// `window`'s bar and accessory row presses, oldest first. A
/// `futures::Stream` that ends when
/// the window closes. One reader per window: a second stream takes the
/// wake-ups from the first. `None` off the main thread or for a closed
/// window.
pub fn keyboard_bar_presses(window: AnyWindowHandle) -> Option<KeyboardBarPresses> {
    let mtm = MainThreadMarker::new()?;
    let queue = crate::window::keyboard_bar_queue(window, mtm)?;
    Some(KeyboardBarPresses { queue })
}

/// Where `window`'s software keyboard was last reported: `Hidden` off the
/// main thread, for a closed window, or before any keyboard showed.
pub fn keyboard_placement(window: AnyWindowHandle) -> KeyboardPlacement {
    let Some(mtm) = MainThreadMarker::new() else {
        return KeyboardPlacement::Hidden;
    };
    crate::window::keyboard_placement(window, mtm)
}

/// Simulator QA: the placement and what it was made from (the frame
/// notifications seen and taken, the last end frame, the probe guide's
/// frame), as one line. `None` off the main thread or for a closed window.
#[cfg(target_abi = "sim")]
pub fn keyboard_placement_sample(window: AnyWindowHandle) -> Option<String> {
    MainThreadMarker::new()?;
    crate::window::keyboard_placement_sample(window)
}

/// Simulator QA: the primary language of the input mode the window's text
/// input view has (`ja-JP`, `en-US`), `None` off the main thread, for a
/// closed window or before UIKit gave it one.
#[cfg(target_abi = "sim")]
pub fn text_input_language(window: AnyWindowHandle) -> Option<String> {
    MainThreadMarker::new()?;
    crate::window::text_input_language(window)
}

/// See [`keyboard_bar_presses`].
pub struct KeyboardBarPresses {
    queue: Rc<RefCell<PressQueue>>,
}

impl KeyboardBarPresses {
    /// The next press, if one is waiting.
    pub fn try_next(&mut self) -> Option<u32> {
        self.queue.borrow_mut().pop()
    }
}

impl futures::Stream for KeyboardBarPresses {
    type Item = u32;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<u32>> {
        let mut queue = self.queue.borrow_mut();
        if let Some(id) = queue.pop() {
            return Poll::Ready(Some(id));
        }
        if queue.is_closed() {
            return Poll::Ready(None);
        }
        queue.register(cx.waker());
        Poll::Pending
    }
}
