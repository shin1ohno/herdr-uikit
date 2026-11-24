//! UIKit object contracts of the selection display (issue #139), on the
//! Simulator outside `UIApplicationMain`: the interaction's `textInput` is
//! the view, its cursor view is the transparent one, attaching puts exactly
//! one interaction on the drawing view, detaching removes it (twice is
//! harmless), retirement moves it to the successor, and dropping the window's
//! text input removes it and leaves no cycle. Whether UIKit actually shows
//! the indicator needs a window and real keys: `examples/input_mode_probe`.
#![allow(
    unsafe_code,
    reason = "main-thread Mach-O initializer, as in the retirement contracts"
)]
#![allow(clippy::expect_used)]

use std::sync::atomic::{AtomicU8, Ordering};

use gpui::{Bounds, ElementInputHandler, PlatformInputHandler, TestAppContext};
use objc2::{ClassType, DefinedClass, MainThreadMarker, rc::Weak, runtime::NSObjectProtocol};
use objc2_ui_kit::{UITextSelectionDisplayInteraction, UIView};

use super::composition_commit::Terminal;
use crate::window::{input_target::InputTarget, text_input::TextInputView};

static RESULT: AtomicU8 = AtomicU8::new(0);
#[used]
#[unsafe(link_section = "__DATA,__mod_init_func")]
static CHECK: extern "C" fn() = check;

extern "C" fn check() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let result = std::panic::catch_unwind(|| objc2::rc::autoreleasepool(|_| exercise(mtm)));
    RESULT.store(if result.is_ok() { 1 } else { 2 }, Ordering::Release);
}

fn exercise(mtm: MainThreadMarker) {
    let mut app = TestAppContext::single();
    let (entity, cx) = app.add_window_view(|_, _| Terminal::default());
    let handler = cx.update(|window, cx| {
        PlatformInputHandler::new(
            window.to_async(cx),
            Box::new(ElementInputHandler::new(Bounds::default(), entity.clone())),
        )
    });
    let input = TextInputView::new(mtm, InputTarget::with_test_handler(handler));
    let host = UIView::new(mtm);
    host.addSubview(&input.view());
    assert_eq!(
        input
            .input_mode_indicator_state(&host)
            .interactions_on_drawing_view,
        0,
        "no-interaction-before-attach"
    );

    input.attach_selection_display(&host);
    let state = input.input_mode_indicator_state(&host);
    assert_eq!(state.interactions_on_drawing_view, 1, "attach-adds-one");
    assert!(state.text_input_is_current, "text-input-is-the-view");
    assert_eq!(state.cursor_class, "HerdrTransparentCursorView");
    assert!(!state.activated, "attached-switched-off");

    // Attaching again keeps one interaction.
    input.attach_selection_display(&host);
    assert_eq!(
        input
            .input_mode_indicator_state(&host)
            .interactions_on_drawing_view,
        1,
        "reattach-keeps-one"
    );

    // Retirement: the old interaction is removed, the successor's is on.
    let old = input.current();
    let (retained_old, next, receipt) = input.begin_retirement().expect("retirement starts");
    input.finish_retirement(retained_old, next, &receipt);
    let new = input.current();
    assert!(!std::ptr::eq(&*old, &*new), "successor-is-new");
    assert!(
        old.ivars().selection_display.borrow().is_none(),
        "old-display-removed"
    );
    let state = input.input_mode_indicator_state(&host);
    assert_eq!(
        state.interactions_on_drawing_view, 1,
        "retirement-moves-the-interaction"
    );
    assert!(state.text_input_is_current, "successor-is-text-input");

    // Detaching twice is harmless and leaves nothing.
    new.detach_selection_display();
    new.detach_selection_display();
    assert_eq!(
        input
            .input_mode_indicator_state(&host)
            .interactions_on_drawing_view,
        0,
        "detach-removes"
    );

    // Dropping the window's text input removes the interaction, and the view
    // is freed once it leaves its superview: no view-interaction cycle. The
    // view under test is created inside this pool, so nothing autoreleased
    // earlier keeps it.
    drop((old, new));
    let weak = objc2::rc::autoreleasepool(|_| {
        let (retained_old, next, receipt) = input.begin_retirement().expect("third retirement");
        input.finish_retirement(retained_old, next, &receipt);
        let newest = input.current();
        assert_eq!(selection_interactions(&host), 1, "successor-attached");
        let weak = Weak::from_retained(&newest);
        drop(input);
        assert_eq!(selection_interactions(&host), 0, "drop-detaches");
        newest.removeFromSuperview();
        weak
    });
    assert!(weak.load().is_none(), "no-retain-cycle");
}

fn selection_interactions(host: &UIView) -> usize {
    host.interactions()
        .iter()
        .filter(|i| i.isKindOfClass(UITextSelectionDisplayInteraction::class()))
        .count()
}

#[test]
fn selection_display_attaches_moves_on_retirement_and_detaches() {
    assert_eq!(
        RESULT.load(Ordering::Acquire),
        1,
        "selection display contract initializer failed"
    );
}
