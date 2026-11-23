//! Host tests for the native modal gate's std-only parts (T-H1 to T-H4 and
//! the event queue): `scripts/check-native-modal-mutations.py` removes each
//! guarded transition in turn and requires the named test to fail.
#![allow(dead_code)]

#[path = "../src/window/native_modal/fence.rs"]
mod fence;
#[path = "../src/window/native_modal/gate.rs"]
mod gate;
#[path = "../src/window/native_modal/intent.rs"]
mod intent;
#[path = "../src/window/native_modal/queue.rs"]
mod queue;

use gate::{DismissReason::*, Effect, Gate, Note, Phase, PresentFailure, Responder, SessionId};

fn emitted(effects: &[Effect]) -> Vec<Note> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Emit(note) => Some(*note),
            _ => None,
        })
        .collect()
}

fn opened(gate: &mut Gate) -> SessionId {
    let id = gate
        .request()
        .unwrap_or_else(|_| panic!("first request accepted"));
    assert_eq!(emitted(&gate.presented(id, true)), vec![Note::Opened]);
    id
}

// T-H1: the full life, and the gate shut from acceptance to the restore.
#[test]
fn h1_lifecycle_keeps_the_gate_shut_until_a_view_is_restored() {
    let mut gate = Gate::default();
    assert!(!gate.blocks());
    let id = gate.request().unwrap_or_else(|_| panic!("accepted"));
    assert!(gate.blocks(), "shut at acceptance, before UIKit is asked");
    assert_eq!(emitted(&gate.presented(id, true)), vec![Note::Opened]);
    assert_eq!(
        gate.dismiss(id, Return),
        vec![Effect::Dismiss { animated: true }]
    );
    assert_eq!(
        emitted(&gate.dismissed(id, Requested)),
        vec![Note::Dismissed(Return)]
    );
    assert_eq!(gate.phase(), Some((id, Phase::AwaitingConsumer)));
    assert!(
        gate.blocks(),
        "still shut while the consumer holds its handle"
    );
    assert_eq!(gate.release(id), vec![Effect::Restore]);
    assert_eq!(gate.phase(), Some((id, Phase::Restoring)));
    assert!(gate.blocks(), "still shut until a responder is back");
    assert_eq!(
        emitted(&gate.restored(id, Responder::Text)),
        vec![Note::Restored(Responder::Text)]
    );
    assert!(!gate.blocks());
}

#[test]
fn h1_a_second_presentation_is_refused_until_the_first_is_restored() {
    let mut gate = Gate::default();
    let id = opened(&mut gate);
    assert!(gate.request().is_err());
    gate.dismiss(id, Cancel);
    gate.dismissed(id, Requested);
    assert!(gate.request().is_err(), "awaiting consumer");
    gate.release(id);
    assert!(gate.request().is_err(), "restoring");
    gate.restored(id, Responder::Drawing);
    assert!(gate.request().is_ok());
}

// F6: no restore before the consumer releases, and the gate opens only on
// an actual responder.
#[test]
fn h1_restore_waits_for_release_and_for_an_actual_responder() {
    let mut gate = Gate::default();
    let id = opened(&mut gate);
    gate.dismiss(id, Selected(7));
    let effects = gate.dismissed(id, Requested);
    assert!(
        !effects.contains(&Effect::Restore),
        "no restore before the handle drops"
    );
    assert!(
        gate.restored(id, Responder::Text).is_empty(),
        "no restore while awaiting"
    );
    assert!(gate.blocks());
    assert_eq!(gate.release(id), vec![Effect::Restore]);
    assert_eq!(
        emitted(&gate.restored(id, Responder::Text)),
        vec![Note::Restored(Responder::Text)]
    );
}

// F6: a restore that settles no responder is asked again, then the gate
// opens with `Responder::None` instead of staying shut for good.
#[test]
fn h1_an_unsettled_restore_retries_then_opens_with_nothing_restored() {
    let mut gate = Gate::default();
    let id = opened(&mut gate);
    gate.dismiss(id, Return);
    gate.dismissed(id, Requested);
    assert!(
        gate.restore_timed_out(id, 0).is_empty(),
        "no retry while the consumer holds its handle"
    );
    assert_eq!(gate.release(id), vec![Effect::Restore]);
    for attempt in 0..gate::RESTORE_ATTEMPTS - 1 {
        assert_eq!(
            gate.restore_timed_out(id, attempt),
            vec![Effect::RetryRestore(attempt + 1)]
        );
        assert!(gate.blocks(), "shut while attempts remain");
    }
    assert_eq!(
        emitted(&gate.restore_timed_out(id, gate::RESTORE_ATTEMPTS - 1)),
        vec![Note::Restored(Responder::None)]
    );
    assert!(!gate.blocks());
    assert!(gate.request().is_ok(), "the next search is accepted");
}

#[test]
fn h1_a_settled_restore_ignores_its_late_check() {
    let mut gate = Gate::default();
    let id = opened(&mut gate);
    gate.release(id);
    gate.dismissed(id, Requested);
    gate.restored(id, Responder::Text);
    assert!(gate.restore_timed_out(id, 0).is_empty());
    let next = gate.request().unwrap_or_else(|_| panic!("accepted"));
    assert!(
        gate.restore_timed_out(id, 2).is_empty(),
        "an old session's check does nothing to the next one"
    );
    assert_eq!(
        gate.phase(),
        Some((next, Phase::Presenting { released: false }))
    );
}

// F5: input reaching GPUI's side is dropped and counted in every phase from
// acceptance to the restore, and goes through before and after.
#[test]
fn f5_late_input_is_dropped_from_acceptance_to_the_restore() {
    let mut gate = Gate::default();
    assert!(!gate.drops_late_input(), "admitted before a search");
    let id = gate.request().unwrap_or_else(|_| panic!("accepted"));
    assert!(gate.drops_late_input(), "presenting");
    gate.presented(id, true);
    assert!(gate.drops_late_input(), "presented");
    gate.dismiss(id, Return);
    assert!(gate.drops_late_input(), "dismissing");
    gate.dismissed(id, Requested);
    assert!(gate.drops_late_input(), "awaiting consumer");
    gate.release(id);
    assert!(gate.drops_late_input(), "restoring");
    assert_eq!(gate.late_drops(), 5);
    gate.restored(id, Responder::Text);
    assert!(!gate.drops_late_input(), "admitted after the restore");
    assert_eq!(gate.late_drops(), 5, "admitted input is not counted");
}

// The field closes the panel on Escape only while nothing is composing.
#[test]
fn escape_closes_only_when_not_composing() {
    let other = 0x04; // A
    assert!(gate::escape_dismisses([gate::ESCAPE_USAGE], false));
    assert!(gate::escape_dismisses([other, gate::ESCAPE_USAGE], false));
    assert!(
        !gate::escape_dismisses([gate::ESCAPE_USAGE], true),
        "Escape cancels the composition instead"
    );
    assert!(!gate::escape_dismisses([other], false));
    assert!(!gate::escape_dismisses([], false));
}

#[test]
fn h1_dropping_the_handle_while_up_dismisses_then_restores() {
    let mut gate = Gate::default();
    let id = opened(&mut gate);
    assert_eq!(gate.release(id), vec![Effect::Dismiss { animated: true }]);
    let effects = gate.dismissed(id, Requested);
    assert_eq!(emitted(&effects), vec![Note::Dismissed(Requested)]);
    assert!(effects.contains(&Effect::Restore));
}

#[test]
fn h1_released_before_uikit_presented_ends_with_one_dismissal() {
    let mut gate = Gate::default();
    let id = gate.request().unwrap_or_else(|_| panic!("accepted"));
    gate.release(id);
    let effects = gate.not_presented(id);
    assert_eq!(emitted(&effects), vec![Note::Dismissed(Requested)]);
    assert!(effects.contains(&Effect::Restore));
    assert!(gate.not_presented(id).is_empty());
}

// F2: failure, OS dismissal, late callbacks and focus taken later.
#[test]
fn f2_presentation_failure_is_reported_once_without_a_dismissal() {
    let mut gate = Gate::default();
    let id = gate.request().unwrap_or_else(|_| panic!("accepted"));
    let effects = gate.failed(id, PresentFailure::PresenterBusy);
    assert_eq!(
        emitted(&effects),
        vec![Note::Failed(PresentFailure::PresenterBusy)]
    );
    assert!(effects.contains(&Effect::Restore));
    assert!(gate.failed(id, PresentFailure::WindowDetached).is_empty());
    assert!(gate.dismissed(id, OutsideTap).is_empty());
}

#[test]
fn f2_opened_only_after_the_field_is_first_responder() {
    let mut gate = Gate::default();
    let id = gate.request().unwrap_or_else(|_| panic!("accepted"));
    assert!(
        emitted(&gate.presented(id, false)).is_empty(),
        "no Opened without the field"
    );
    assert_eq!(emitted(&gate.focused(id)), vec![Note::Opened]);
    assert!(gate.focused(id).is_empty(), "Opened once");
}

#[test]
fn f2_field_refusal_removes_the_panel_and_fails() {
    let mut gate = Gate::default();
    let id = gate.request().unwrap_or_else(|_| panic!("accepted"));
    gate.presented(id, false);
    let effects = gate.failed(id, PresentFailure::FieldRefused);
    assert_eq!(effects[0], Effect::Dismiss { animated: false });
    assert_eq!(
        emitted(&effects),
        vec![Note::Failed(PresentFailure::FieldRefused)]
    );
}

#[test]
fn f2_os_dismissal_without_our_dismiss_is_a_dismissal() {
    for reason in [OutsideTap, SwipedDown, System] {
        let mut gate = Gate::default();
        let id = opened(&mut gate);
        assert_eq!(
            emitted(&gate.dismissed(id, reason)),
            vec![Note::Dismissed(reason)]
        );
        assert_eq!(gate.phase(), Some((id, Phase::AwaitingConsumer)));
        assert!(gate.dismissed(id, reason).is_empty(), "one dismissal");
    }
}

#[test]
fn f2_late_callbacks_of_an_ended_session_do_nothing() {
    let mut gate = Gate::default();
    let old = opened(&mut gate);
    gate.dismissed(old, OutsideTap);
    gate.release(old);
    gate.restored(old, Responder::Text);
    let new = opened(&mut gate);
    assert!(gate.dismissed(old, Requested).is_empty());
    assert!(gate.presented(old, true).is_empty());
    assert!(gate.restored(old, Responder::Text).is_empty());
    assert_eq!(gate.session(), Some(new));
}

#[test]
fn f2_window_close_ends_every_phase_once() {
    type Setup = fn(&mut Gate, SessionId);
    let cases: [(&str, Setup, bool); 5] = [
        ("presenting", |_, _| {}, true),
        ("presented", |g, id| drop(g.presented(id, true)), true),
        (
            "dismissing",
            |g, id| {
                g.presented(id, true);
                g.dismiss(id, Cancel);
            },
            true,
        ),
        (
            "awaiting",
            |g, id| {
                g.presented(id, true);
                g.dismissed(id, OutsideTap);
            },
            false,
        ),
        (
            "restoring",
            |g, id| {
                g.presented(id, true);
                g.dismissed(id, OutsideTap);
                g.release(id);
            },
            false,
        ),
    ];
    for (name, setup, dismissal) in cases {
        let mut gate = Gate::default();
        let id = gate.request().unwrap_or_else(|_| panic!("accepted"));
        setup(&mut gate, id);
        let notes = emitted(&gate.window_closed());
        let mut expected = Vec::new();
        if dismissal {
            expected.push(Note::Dismissed(WindowClosed));
        }
        expected.push(Note::Restored(Responder::WindowClosed));
        assert_eq!(notes, expected, "{name}");
        assert!(!gate.blocks(), "{name}");
        assert!(
            gate.dismissed(id, Requested).is_empty(),
            "{name}: late completion"
        );
    }
}

// T-H2 / F10 (SDK side): GPUI's keyboard requests while shut, last wins.
#[test]
fn h2_keyboard_intent_keeps_the_last_request_and_counts_notifications() {
    let mut intent = intent::Intent::default();
    intent.show();
    intent.hide();
    intent.notification();
    intent.notification();
    assert_eq!(intent.dropped_notifications, 2);
    assert_eq!(intent.take(), intent::Request::Hide);
    assert_eq!(intent.take(), intent::Request::None, "replayed once");
}

// T-H3 / F3: presses fenced by key, released one by one.
#[test]
fn h3_two_held_keys_release_independently() {
    let mut fence = fence::Fence::default();
    fence.hold(40); // Return
    fence.hold(4); // a
    fence.released(40);
    assert!(
        !fence.blocks(40),
        "a new Return after its release reaches GPUI"
    );
    assert!(fence.blocks(4), "the other key stays fenced");
    fence.released(4);
    assert!(!fence.blocks(4));
}

#[test]
fn h3_overflow_fences_everything_until_nothing_is_held() {
    let mut fence = fence::Fence::default();
    for usage in 0..=(fence::CAPACITY as isize) {
        fence.hold(4 + usage);
    }
    assert!(fence.overflowed());
    assert!(fence.blocks(200), "every key while overflowed");
    fence.released(4);
    assert!(fence.blocks(4), "one release does not end an overflow");
    fence.retain_held(&[]);
    assert!(!fence.blocks(4) && !fence.blocks(200));
}

#[test]
fn h3_a_lost_release_ends_when_uikit_stops_reporting_the_key() {
    let mut fence = fence::Fence::default();
    fence.hold(40);
    fence.hold(4);
    fence.retain_held(&[4]);
    assert!(!fence.blocks(40));
    assert!(fence.blocks(4));
}

#[test]
fn h3_modifiers_are_never_fenced() {
    assert!(fence::is_modifier(0xE3), "left command");
    assert!(fence::is_modifier(0x39), "caps lock");
    assert!(!fence::is_modifier(40), "return");
}

// T-H4 / F4: app menu commands while shut, at perform and at delivery.
#[test]
fn h4_menu_commands_never_run_or_show_enabled_while_shut() {
    assert!(gate::command_runs(false, false, false));
    assert!(!gate::command_runs(true, false, false), "fenced key");
    assert!(
        !gate::command_runs(false, true, false),
        "shut when performed"
    );
    assert!(
        !gate::command_runs(false, false, true),
        "shut when the deferred delivery runs"
    );
    assert!(gate::command_enabled(false));
    assert!(!gate::command_enabled(true));
}

// F11: latest-state query, FIFO for the rest.
#[derive(Debug, PartialEq)]
enum Event {
    Query(&'static str),
    Selected(u64),
    Dismissed,
}
impl queue::Coalesce for Event {
    fn is_query(&self) -> bool {
        matches!(self, Event::Query(_))
    }
}

#[test]
fn f11_queries_coalesce_in_place_and_ordering_events_keep_order() {
    let mut queue = queue::Queue::default();
    queue.push(Event::Query("p"));
    queue.push(Event::Query("pr"));
    queue.push(Event::Query("pro"));
    queue.push(Event::Selected(3));
    queue.push(Event::Query("pro2"));
    queue.push(Event::Dismissed);
    let drained: Vec<_> = std::iter::from_fn(|| queue.pop()).collect();
    assert_eq!(
        drained,
        vec![
            Event::Query("pro"),
            Event::Selected(3),
            Event::Query("pro2"),
            Event::Dismissed
        ]
    );
    assert_eq!(queue.coalesced, 2);
    queue.close();
    queue.push(Event::Dismissed);
    assert!(queue.pop().is_none(), "nothing after close");
}
