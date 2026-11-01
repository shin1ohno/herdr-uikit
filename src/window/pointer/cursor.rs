//! UIKit owns one pointer interaction per window. Text cursors use native
//! beams; crosshairs and resize cursors use small closed vector paths.
//! Directional resize cursors show both directions on the requested axis.
//! UIKit has no desktop hand, forbidden, copy, link, or context-menu cursor,
//! so those styles and Arrow use its ordinary system pointer.
//!
//! The window owns the delegate because UIPointerInteraction keeps it weakly.
//! The delegate and the current hover target keep windows weakly. Closing or
//! hiding a window disables its interaction and clears only its own target.

mod delegate;
mod geometry;
#[cfg(test)]
mod tests;

use super::super::{state::Window, view::GpuiView};
use delegate::PointerDelegate;
use gpui::CursorStyle;
use objc2::{MainThreadMarker, MainThreadOnly, Message, rc::Retained, runtime::ProtocolObject};
use objc2_ui_kit::UIPointerInteraction;
use std::{
    cell::{Cell, RefCell},
    ptr,
    rc::{Rc, Weak},
};

thread_local! {
    static TARGET: RefCell<Weak<Window>> = const { RefCell::new(Weak::new()) };
}

pub(in crate::window) struct Pointer {
    /// UIPointerInteraction does not retain its delegate.
    _delegate: Retained<PointerDelegate>,
    attached: Cell<bool>,
    interaction: Retained<UIPointerInteraction>,
    view: Retained<GpuiView>,
}

impl Pointer {
    pub(in crate::window) fn new(
        view: &GpuiView,
        owner: Weak<Window>,
        mtm: MainThreadMarker,
    ) -> Self {
        let delegate = PointerDelegate::new(owner, mtm);
        let interaction = UIPointerInteraction::initWithDelegate(
            UIPointerInteraction::alloc(mtm),
            Some(ProtocolObject::from_ref(&*delegate)),
        );
        interaction.setEnabled(false);
        view.addInteraction(ProtocolObject::from_ref(&*interaction));
        Self {
            _delegate: delegate,
            attached: Cell::new(true),
            interaction,
            view: view.retain(),
        }
    }

    pub(in crate::window) fn set_visible(&self, visible: bool) {
        if self.attached.get() {
            self.interaction.setEnabled(visible);
            self.interaction.invalidate();
        }
    }

    pub(in crate::window) fn invalidate(&self) {
        if self.attached.get() {
            self.interaction.invalidate();
        }
    }

    pub(in crate::window) fn detach(&self) {
        if self.attached.replace(false) {
            self.interaction.setEnabled(false);
            self.view
                .removeInteraction(ProtocolObject::from_ref(&*self.interaction));
        }
    }
}

impl Drop for Pointer {
    fn drop(&mut self) {
        self.detach();
    }
}

pub(super) fn entered(window: &Rc<Window>) {
    let previous = TARGET.with_borrow_mut(|target| replace_target(target, window));
    // UIKit can enter the next window before the previous window's deferred
    // hover exit runs. Settle that exit now so its next frame cannot set the
    // new window's cursor through GPUI's app-wide platform method.
    if let Some(previous) = previous {
        let closed = previous.state.borrow().closed;
        if !closed {
            previous.pointer_left();
        }
    }
}

fn replace_target<T>(target: &mut Weak<T>, window: &Rc<T>) -> Option<Rc<T>> {
    let previous = target.upgrade();
    *target = Rc::downgrade(window);
    previous.filter(|previous| !Rc::ptr_eq(previous, window))
}

pub(in crate::window) fn clear_window(window: &Window) {
    TARGET.with_borrow_mut(|target| clear_if_owner(target, window));
}

fn clear_if_owner<T>(target: &mut Weak<T>, owner: &T) {
    if ptr::eq(target.as_ptr(), owner) {
        *target = Weak::new();
    }
}

pub(crate) fn set_cursor_style(style: CursorStyle, _mtm: MainThreadMarker) {
    let Some(window) = TARGET.with_borrow(Weak::upgrade) else {
        return;
    };
    let changed = {
        let mut state = window.state.borrow_mut();
        if state.closed || !state.visibility.is_visible() || !state.hovered {
            return;
        }
        std::mem::replace(&mut state.cursor_style, style) != style
    };
    // Invalidation may synchronously ask the delegate for a region or style.
    // No window state borrow or GPUI callback remains on the stack here.
    if changed {
        window.pointer.invalidate();
    }
}
