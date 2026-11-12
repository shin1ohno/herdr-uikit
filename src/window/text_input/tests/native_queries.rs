//! Real Objective-C selectors over a controlled SDK host. This is not P13's
//! field, actual native Cut/Escape, responder readiness or OS delivery evidence.
#![allow(
    unsafe_code,
    reason = "main-thread initializer and scoped native test IMP instrumentation"
)]
#![allow(clippy::expect_used)]

mod host;
mod selectors;

use std::{
    ops::Range,
    sync::atomic::{AtomicU8, Ordering},
};

use gpui::{Bounds, ElementInputHandler, PlatformInputHandler, TestAppContext};
use objc2::MainThreadMarker;

use super::super::{
    TextInputView,
    probe::{NativeClient, NativeQuery, NativeQueryError},
};
use crate::window::input_target::InputTarget;
use host::Document;
use selectors::{NilGetter, Override, setters};

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

fn value<T: std::fmt::Debug>(
    client: &NativeClient,
    result: Result<NativeQuery<T>, NativeQueryError>,
) -> T {
    let sample = result.expect("native query succeeds");
    assert_eq!(
        sample.client,
        client.identity(),
        "p13-captured-client-identity"
    );
    eprintln!(
        "P13 SDK-host client={} value={:?}",
        sample.client, sample.value
    );
    sample.value
}

fn selection(client: &NativeClient) -> Option<Range<usize>> {
    value(client, client.selected_range_utf16())
}

fn no_setters(before: usize) {
    assert_eq!(setters(), before, "p13-query-does-not-set-selection");
}

fn observe(client: &NativeClient, range: Range<usize>, length: usize, from: usize, delta: isize) {
    let count = setters();
    assert_eq!(
        selection(client),
        Some(range.clone()),
        "p13-observed-selection"
    );
    assert_eq!(
        value(client, client.document_length_utf16()),
        Some(length),
        "p13-native-document-end"
    );
    no_setters(count);
    assert_eq!(
        value(client, client.position_offset_utf16(from, delta)),
        Some(length)
    );
    assert_eq!(
        value(client, client.position_offset_utf16(from, delta + 1)),
        None
    );
    assert_eq!(selection(client), Some(range), "p13-observed-selection");
    no_setters(count);
}

fn exercise(mtm: MainThreadMarker) {
    let _monitor = Override::count_setters();
    let mut app = TestAppContext::single();
    let (entity, cx) = app.add_window_view(|_, _| Document::new("abcdef", Some(0..0), None));
    let handler = cx.update(|window, cx| {
        PlatformInputHandler::new(
            window.to_async(cx),
            Box::new(ElementInputHandler::new(Bounds::default(), entity.clone())),
        )
    });
    let input = TextInputView::new(mtm, InputTarget::with_test_handler(handler));
    let client = NativeClient::new(&input);
    let original_id = client.identity();

    // Q01/Q07: setup is the only setter; subsequent observations never repair.
    assert!(value(&client, client.select_utf16(3..3)));
    assert_eq!(setters(), 1, "p13-setup-sets-once");
    observe(&client, 3..3, 6, 3, 3);
    assert_eq!(value(&client, client.position_offset_utf16(3, 1)), Some(4));
    assert_eq!(value(&client, client.position_offset_utf16(7, -1)), None);
    no_setters(1);

    // Q06: invalid setup performs no setter; all signed extremes reach native
    // position bounds without overflow or synthesized from+delta arithmetic.
    let count = setters();
    assert_eq!(
        client.select_utf16(Range { start: 3, end: 2 }),
        Err(NativeQueryError::ReversedRange),
        "p13-reversed-setup-rejected-before-setter"
    );
    assert_eq!(
        client.select_utf16(0..usize::MAX),
        Err(NativeQueryError::IntegerOutOfRange)
    );
    assert!(!value(&client, client.select_utf16(0..7)));
    assert_eq!(
        client.position_offset_utf16(usize::MAX, 0),
        Err(NativeQueryError::IntegerOutOfRange)
    );
    for delta in [isize::MIN, isize::MAX] {
        assert_eq!(value(&client, client.position_offset_utf16(3, delta)), None);
    }
    assert_eq!(selection(&client), Some(3..3));
    no_setters(count);
    {
        let _refused = Override::refuse_selection();
        assert!(
            !value(&client, client.select_utf16(2..2)),
            "p13-setup-observes-refusal"
        );
        assert_eq!(selection(&client), Some(3..3));
        assert_eq!(setters(), count + 1, "p13-setup-does-not-retry");
    }

    // Fault injection is confined to native methods in this main-thread test.
    let count = setters();
    for kind in [
        NilGetter::Beginning,
        NilGetter::RangeStart,
        NilGetter::RangeEnd,
    ] {
        let beginning = matches!(kind, NilGetter::Beginning);
        let _nil = Override::nil_getter(kind);
        assert_eq!(selection(&client), None, "p13-native-nil-preserved");
        if beginning {
            assert_eq!(value(&client, client.document_length_utf16()), None);
            assert_eq!(value(&client, client.position_offset_utf16(0, 0)), None);
            assert!(!value(&client, client.select_utf16(0..0)));
        }
    }
    {
        let _nil = Override::nil_getter(NilGetter::DocumentEnd);
        assert_eq!(value(&client, client.document_length_utf16()), None);
    }
    {
        let _nil = Override::nil_range();
        assert!(!value(&client, client.select_utf16(1..2)));
    }
    {
        let _negative = Override::offsets(false);
        assert_eq!(
            client.selected_range_utf16(),
            Err(NativeQueryError::NegativeOffset)
        );
        assert_eq!(
            client.document_length_utf16(),
            Err(NativeQueryError::NegativeOffset)
        );
        assert_eq!(
            client.position_offset_utf16(0, 0),
            Err(NativeQueryError::NegativeOffset)
        );
    }
    {
        let _reversed = Override::offsets(true);
        assert_eq!(
            client.selected_range_utf16(),
            Err(NativeQueryError::ReversedRange)
        );
    }
    no_setters(count);
    assert_eq!(selection(&client), Some(3..3), "native overrides restored");

    // Q02: exact UTF-16 values, including a surrogate pair inside the mark.
    cx.update(|_, cx| {
        entity.update(cx, |doc, _| {
            *doc = Document::new("pre:変😀中:post", Some(4..4), Some(4..8))
        })
    });
    let count = setters();
    assert!(value(&client, client.select_utf16(5..7)));
    assert_eq!(setters(), count + 1);
    observe(&client, 5..7, 13, 5, 8);

    // Controlled post-edit state ONLY. Actual Cut/Escape belongs to P13 Q03/Q04.
    cx.update(|_, cx| {
        entity.update(cx, |doc, _| {
            *doc = Document::new("pre:変中:post", Some(5..5), Some(4..6))
        })
    });
    observe(&client, 5..5, 11, 5, 6);
    assert_eq!(
        entity.read_with(cx, |doc, _| String::from_utf16(&doc.text)
            .expect("fixture UTF-16")),
        "pre:変中:post"
    );

    // Q05: empty selection is nil; empty document has a measured native zero.
    cx.update(|_, cx| entity.update(cx, |doc, _| *doc = Document::new("", None, None)));
    let count = setters();
    assert_eq!(selection(&client), None);
    assert_eq!(value(&client, client.document_length_utf16()), Some(0));
    assert!(value(&client, client.select_utf16(0..0)));
    assert_eq!(selection(&client), Some(0..0));
    assert_eq!(setters(), count + 1);

    cx.update(|_, cx| {
        entity.update(cx, |doc, _| {
            *doc = Document::new("abcdef", Some(3..3), None)
        })
    });
    let (old, next, receipt) = input.begin_retirement().expect("retirement setup");
    assert_eq!(
        value(&client, client.document_length_utf16()),
        Some(0),
        "p13-retired-native-zero"
    );
    input.finish_retirement(old, next, &receipt);
    let current = NativeClient::new(&input);
    assert_ne!(current.identity(), original_id);
    let count = setters();
    assert_eq!(client.identity(), original_id);
    assert_eq!(selection(&client), None);
    assert_eq!(
        value(&client, client.document_length_utf16()),
        Some(0),
        "p13-retired-native-zero"
    );
    assert_eq!(value(&client, client.position_offset_utf16(0, 0)), Some(0));
    assert_eq!(value(&client, client.position_offset_utf16(0, 1)), None);
    observe(&current, 3..3, 6, 3, 3);
    no_setters(count);
}

#[test]
fn native_queries_observe_captured_client_without_selection_repair() {
    assert_eq!(
        RESULT.load(Ordering::Acquire),
        1,
        "native query initializer failed; inspect its named assertion"
    );
}
