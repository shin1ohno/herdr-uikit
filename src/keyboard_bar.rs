//! Keys on the software keyboard's shortcuts bar: the row above the keys
//! (`UITextInputAssistantItem`'s leading and trailing groups on the window's
//! text input view), and where the keyboard is.
//!
//! UIKit shows the bar on a docked keyboard (and, by Apple's design, on a
//! split one), but not on a floating keyboard: there only the predictive
//! row is drawn (`docs/keyboard-bar.md`, measured on the iPadOS 27.2
//! Simulator). A consumer that needs its keys in every state reads
//! [`keyboard_placement`] and draws the same keys itself while it is
//! [`KeyboardPlacement::Floating`].
//!
//! A press arrives on the window's [`KeyboardBarPresses`] stream as the
//! item's `id` (or a menu entry's), after UIKit handled the touch. The bar
//! never takes first responder. Every call is main-thread only.

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
}

/// The bar's buttons. Each inner list is one group; UIKit keeps a group's
/// buttons together. An empty bar shows no consumer buttons.
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
/// same bar again is a no-op, so a consumer may call this every frame.
/// Returns false off the main thread or if the window no longer exists.
pub fn set_keyboard_bar(window: AnyWindowHandle, bar: &KeyboardBar) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    crate::window::set_keyboard_bar(window, bar, mtm)
}

/// `window`'s bar presses, oldest first. A `futures::Stream` that ends when
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
