//! Test-only window resolution for the native edit observer. A controlled
//! `TextInputView` stands in for a scene-hosted window's text responder, so a
//! Simulator unit test reaches `arm`, `drain` and the real Objective-C edit
//! selectors without UIApplicationMain, a scene, a renderer or a drawing view.
//! It replaces only the handle lookup in `scene.rs`; every observer, selector
//! and dispatch step after that lookup is production code. It is not native
//! receiving, consumer callback or Drawing-responder evidence.
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use gpui::AnyWindowHandle;

use super::text_input::TextInputView;

thread_local! {
    static HOSTS: RefCell<Vec<(AnyWindowHandle, Weak<TextInputView>)>> =
        const { RefCell::new(Vec::new()) };
}

/// Keeps `handle` resolvable until dropped. Holds no strong reference.
#[must_use = "the handle stops resolving when the guard drops"]
pub(super) struct Hosted(AnyWindowHandle);

pub(super) fn host(handle: AnyWindowHandle, input: &Rc<TextInputView>) -> Hosted {
    HOSTS.with_borrow_mut(|hosts| {
        assert!(
            hosts.iter().all(|(hosted, _)| *hosted != handle),
            "a window handle is hosted at most once"
        );
        hosts.push((handle, Rc::downgrade(input)));
    });
    Hosted(handle)
}

impl Drop for Hosted {
    fn drop(&mut self) {
        let _ = HOSTS.try_with(|hosts| hosts.borrow_mut().retain(|(h, _)| *h != self.0));
    }
}

/// The hosted text input for `handle`, if a test hosts one and it is alive.
pub(super) fn find(handle: AnyWindowHandle) -> Option<Rc<TextInputView>> {
    HOSTS.with_borrow(|hosts| {
        hosts
            .iter()
            .find(|(hosted, _)| *hosted == handle)
            .and_then(|(_, input)| input.upgrade())
    })
}
