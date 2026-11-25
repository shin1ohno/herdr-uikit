//! Host tests for the inline field's std-only decisions and its use of the
//! shared native modal gate: `scripts/native_inline_field_mutations.py host`
//! removes each guarded decision in turn and requires the named test to fail.
#![allow(dead_code)]

#[path = "../src/window/native_modal/gate.rs"]
mod gate;
#[path = "../src/window/native_inline_field/policy.rs"]
mod policy;

use gate::{DismissReason as GateReason, Effect, Gate, Note, PresentFailure, Responder};
use policy::{
    Bounded, Decision, EVENT_CAPACITY, NativeInlineDismissReason as Reason,
    NativeInlineFieldEvent as Event, Rect, clamp, commit_decision, dismiss_reason, field_rect,
    visible,
};

#[test]
fn commit_unchanged_is_unchanged() {
    assert_eq!(
        commit_decision("a", "  a ", false, 160),
        Decision::Unchanged
    );
    assert_eq!(commit_decision("a", "a", true, 160), Decision::Unchanged);
    assert_eq!(
        commit_decision("a", "b", false, 160),
        Decision::Commit("b".into())
    );
}

#[test]
fn empty_without_allow_is_hint() {
    assert_eq!(commit_decision("x", "", false, 160), Decision::Hint);
    assert_eq!(commit_decision("x", "   ", false, 160), Decision::Hint);
}

#[test]
fn empty_with_allow_commits_empty() {
    assert_eq!(
        commit_decision("x", "", true, 160),
        Decision::Commit(String::new())
    );
    assert_eq!(
        commit_decision("x", " \t", true, 160),
        Decision::Commit(String::new())
    );
}

#[test]
fn commit_trims_and_limits() {
    let long: String = "x".repeat(161);
    let Decision::Commit(name) = commit_decision("a", &long, false, 160) else {
        panic!("a changed name commits");
    };
    assert_eq!(name.chars().count(), 160);
    // A base letter and a combining mark are two `char`s.
    let marks: String = "e\u{301}".repeat(81);
    let Decision::Commit(name) = commit_decision("a", &marks, false, 160) else {
        panic!("a changed name commits");
    };
    assert_eq!(name.chars().count(), 160);
    assert_eq!(
        commit_decision("a", "  ぷろでぶ  ", false, 160),
        Decision::Commit("ぷろでぶ".into())
    );
}

#[test]
fn clamp_never_cuts_marked() {
    let long: String = "あ".repeat(170);
    assert_eq!(clamp(&long, 160, true), (long.clone(), false));
}

#[test]
fn clamp_cuts_after_composition() {
    let long: String = "あ".repeat(170);
    let (kept, cut) = clamp(&long, 160, false);
    assert!(cut);
    assert_eq!(kept.chars().count(), 160);
    assert_eq!(clamp("abc", 160, false), ("abc".into(), false));
}

#[test]
fn rect_outset_and_min_width() {
    let narrow = Rect {
        x: 10.0,
        y: 20.0,
        w: 50.0,
        h: 20.0,
    };
    assert_eq!(
        field_rect(narrow, 4.0, 2.0, 120.0),
        Rect {
            x: 6.0,
            y: 18.0,
            w: 120.0,
            h: 24.0
        }
    );
    let wide = Rect { w: 200.0, ..narrow };
    assert_eq!(
        field_rect(wide, 4.0, 2.0, 120.0),
        Rect {
            x: 6.0,
            y: 18.0,
            w: 208.0,
            h: 24.0
        }
    );
}

#[test]
fn visible_hidden_when_clip_empty() {
    let frame = Rect {
        x: 0.0,
        y: 100.0,
        w: 120.0,
        h: 24.0,
    };
    assert_eq!(visible(frame, None), (frame, false));
    let above = Rect {
        x: 0.0,
        y: 0.0,
        w: 300.0,
        h: 50.0,
    };
    assert!(visible(frame, Some(above)).1, "a disjoint clip hides");
    let partial = Rect {
        x: 0.0,
        y: 0.0,
        w: 300.0,
        h: 110.0,
    };
    assert_eq!(
        visible(frame, Some(partial)),
        (
            Rect {
                x: 0.0,
                y: 100.0,
                w: 120.0,
                h: 10.0
            },
            false
        )
    );
}

/// The gate a window keeps: one per window, for either kind. A test-local
/// switch stands in for "a gate per kind" (`KIND_GATES`), which the mutation
/// turns on.
const KIND_GATES: bool = false;

struct Window {
    search: Gate,
    inline: Gate,
}

impl Window {
    fn gate(&mut self, inline: bool) -> &mut Gate {
        if KIND_GATES && inline {
            &mut self.inline
        } else {
            &mut self.search
        }
    }
}

#[test]
fn gate_shared_with_search() {
    let mut window = Window {
        search: Gate::default(),
        inline: Gate::default(),
    };
    let search = window.gate(false).request();
    assert!(search.is_ok(), "the search panel opens");
    assert!(
        window.gate(true).request().is_err(),
        "the inline field is refused while the search panel is up"
    );
    let mut window = Window {
        search: Gate::default(),
        inline: Gate::default(),
    };
    assert!(window.gate(true).request().is_ok(), "the field opens");
    assert!(
        window.gate(false).request().is_err(),
        "the search panel is refused while the field is up"
    );
}

#[test]
fn events_bounded_8_in_order() {
    let mut queue = Bounded::default();
    assert_eq!(EVENT_CAPACITY, 8);
    for _ in 0..EVENT_CAPACITY - 2 {
        assert!(queue.push(Event::Opened).is_ok());
    }
    assert!(queue.push(Event::Committed("build".into())).is_ok());
    assert!(queue.push(Event::Dismissed(Reason::Committed)).is_ok());
    assert!(queue.push(Event::Opened).is_err(), "the ninth is refused");
    let order: Vec<_> = std::iter::from_fn(|| queue.pop()).collect();
    let committed = order
        .iter()
        .position(|e| *e == Event::Committed("build".into()));
    let dismissed = order
        .iter()
        .position(|e| *e == Event::Dismissed(Reason::Committed));
    assert!(committed.is_some_and(|c| dismissed.is_some_and(|d| c + 1 == d)));
    queue.close();
    assert!(queue.push(Event::Opened).is_ok() && queue.pop().is_none());
}

fn emitted(effects: &[Effect]) -> Vec<Note> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Emit(note) => Some(*note),
            _ => None,
        })
        .collect()
}

/// The consumer's event for a note, as the window maps it.
fn event(note: Note, decision: Option<&Decision>) -> Event {
    match note {
        Note::Opened => Event::Opened,
        Note::Dismissed(reason) => Event::Dismissed(dismiss_reason(reason, decision)),
        Note::Failed(failure) => Event::Dismissed(Reason::PresentationFailed(failure)),
        Note::Restored(responder) => Event::InputRestored(responder),
    }
}

#[test]
fn seven_closers_one_dismissed() {
    let commit = Decision::Commit("build".into());
    let closers: [(&str, Reason); 7] = [
        ("return-commit", Reason::Committed),
        ("return-unchanged", Reason::Unchanged),
        ("escape", Reason::Escape),
        ("outside-tap", Reason::OutsideTap),
        ("requested", Reason::Requested),
        ("window-closed", Reason::WindowClosed),
        (
            "failed",
            Reason::PresentationFailed(PresentFailure::FieldRefused),
        ),
    ];
    for (name, expected) in closers {
        let mut gate = Gate::default();
        let id = gate
            .request()
            .unwrap_or_else(|_| panic!("{name}: accepted"));
        let mut notes = Vec::new();
        let decision = match name {
            "return-commit" => Some(&commit),
            "return-unchanged" => Some(&Decision::Unchanged),
            _ => None,
        };
        if name == "failed" {
            notes.extend(emitted(&gate.presented(id, false)));
            notes.extend(emitted(&gate.failed(id, PresentFailure::FieldRefused)));
        } else {
            notes.extend(emitted(&gate.presented(id, true)));
            let reason = match name {
                "return-commit" | "return-unchanged" => GateReason::Return,
                "escape" => GateReason::Escape,
                "outside-tap" => GateReason::OutsideTap,
                _ => GateReason::Requested,
            };
            if name == "window-closed" {
                notes.extend(emitted(&gate.window_closed()));
            } else {
                notes.extend(emitted(&gate.dismiss(id, reason)));
                // The controller's completion, and a second tap's dismissal.
                notes.extend(emitted(&gate.dismissed(id, GateReason::Requested)));
                notes.extend(emitted(&gate.dismiss(id, GateReason::OutsideTap)));
                notes.extend(emitted(&gate.release(id)));
                notes.extend(emitted(&gate.restored(id, Responder::Text)));
            }
        }
        if name == "failed" {
            notes.extend(emitted(&gate.restored(id, Responder::Text)));
        }
        let events: Vec<Event> = notes.iter().map(|n| event(*n, decision)).collect();
        let dismissed: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, Event::Dismissed(_)))
            .collect();
        assert_eq!(
            dismissed,
            vec![&Event::Dismissed(expected)],
            "{name}: exactly one Dismissed, with its reason ({events:?})"
        );
        let restored = events
            .iter()
            .filter(|e| matches!(e, Event::InputRestored(_)))
            .count();
        assert_eq!(restored, 1, "{name}: then one InputRestored ({events:?})");
        assert!(
            matches!(events.last(), Some(Event::InputRestored(_))),
            "{name}: InputRestored comes last"
        );
        assert!(!gate.blocks(), "{name}: the gate opened");
    }
}

#[test]
fn search_only_reasons_are_requested() {
    for reason in [
        GateReason::Cancel,
        GateReason::Selected(3),
        GateReason::SwipedDown,
        GateReason::System,
        GateReason::Requested,
    ] {
        assert_eq!(
            dismiss_reason(reason, None),
            Reason::Requested,
            "{reason:?}"
        );
    }
    assert_eq!(dismiss_reason(GateReason::Return, None), Reason::Unchanged);
}
