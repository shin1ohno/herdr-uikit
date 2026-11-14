#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use NativeCutCounter as C;
use NativeCutPhase as P;
use NativeCutRefusal as R;

const RECEIVER: usize = 7;

fn armed() -> Armed {
    arm(RECEIVER, 9, FocusId::default()).expect("one run")
}
fn rearm(run: Armed) -> Armed {
    drop(run);
    armed()
}
fn trace() -> NativeCutObservationTrace {
    RUN.with_borrow(|run| run.as_ref().expect("armed").trace.clone())
}
// Every production milestone through the selector return, in order.
fn tail() {
    P::AcceptedCut.record_edit(sel!(cut:));
    for phase in [
        P::DownReturned,
        P::UpReturned,
        P::OriginRestored,
        P::ModifiersRestored,
    ] {
        phase.record();
    }
    drop(P::SnapshotEntered.scope());
    P::SelectorReturned.record();
}
fn only(reason: NativeCutRefusal) -> bool {
    trace().refused == 1 << reason as u32
}

#[test]
fn legal_run_freezes_one_summary_and_disarms() {
    let run = armed();
    tail();
    permit();
    drop(C::QueryEntries.query());
    let (_, frozen, exhausted) = run.finish(Some((5, 5)));
    assert!(!exhausted && frozen.refused == 0 && frozen.len == 11);
    let phases: Vec<_> = frozen
        .events
        .iter()
        .flatten()
        .map(|event| event.phase)
        .collect();
    assert!(phases.is_sorted() && phases.len() == 11, "{phases:?}");
    assert_eq!(
        frozen.events[10].and_then(|event| event.range),
        Some((5, 5))
    );
    assert_eq!(
        (
            frozen.counters[C::QueryEntries as usize],
            frozen.counters[C::QueryExits as usize]
        ),
        (1, 1)
    );
    R::Setter.record();
    assert_eq!(
        phase(RECEIVER),
        None,
        "a late event cannot reach the frozen copy"
    );
}

#[test]
fn edits_getters_and_turns_each_set_only_their_own_bit() {
    let _run = armed();
    P::AcceptedCut.record_edit(sel!(cut:));
    P::AcceptedCut.record_edit(sel!(cut:));
    assert!(only(R::NestedEditAction) && phase(RECEIVER) == Some(P::AcceptedCut));
    let _run = rearm(_run);
    drop(C::QueryEntries.query());
    assert!(only(R::EarlyQuery), "a getter without the permit");
    let _run = rearm(_run);
    tail();
    permit();
    drop(C::QueryEntries.query());
    assert!(trace().refused == 0 && phase(RECEIVER) == Some(P::FirstGetterEntered));
    drop(C::QueryEntries.query());
    assert!(only(R::EarlyQuery), "the permit is used once");
    let _run = rearm(_run);
    R::DeferredScheduled.record();
    assert!(only(R::DeferredScheduled) && trace().counters[C::TurnCrossings as usize] == 0);
    let _run = rearm(_run);
    R::DeferredExecuted.record();
    assert!(only(R::DeferredExecuted) && trace().counters[C::TurnCrossings as usize] == 1);
}

#[test]
fn refusals_are_sticky_and_do_not_move_the_milestone() {
    let unconditional = [
        R::FrameTick,
        R::InputHandlerFlush,
        R::TextInputFlush,
        R::DelegateNotify,
        R::AfterAction,
        R::Resign,
        R::LeavingWindow,
        R::Configuration,
        R::KeyboardShown,
        R::KeyboardHidden,
    ];
    for reason in unconditional.into_iter().chain([R::Setter]) {
        let run = armed();
        P::AcceptedCut.record_edit(sel!(cut:));
        reason.record();
        reason.record();
        assert!(
            only(reason) && phase(RECEIVER) == Some(P::AcceptedCut),
            "{reason:?}"
        );
        assert_eq!(trace().len, 3, "one event per kind: {reason:?}");
        P::DownReturned.record();
        assert!(
            run.finish(None).1.has_refusal(reason),
            "{reason:?} stays set"
        );
    }
}

#[test]
fn focus_handler_and_presentation_rules() {
    let _run = armed();
    R::FocusChange.on_state(&TextInputStateChange::ContentChanged);
    R::FocusChange.on_state(&TextInputStateChange::SelectionChanged);
    assert_eq!(trace().refused, 0);
    R::FocusChange.on_state(&TextInputStateChange::FocusGained);
    assert!(only(R::FocusChange));
    assert!(same_install((1, &Some(3)), (1, &Some(3))));
    assert!(!same_install((1, &Some(3)), (1, &None)), "nil focus");
    assert!(!same_install((1, &Some(3)), (1, &Some(4))), "another focus");
    assert!(
        !same_install((1, &Some(3)), (2, &Some(3))),
        "another window"
    );
    let _run = rearm(_run);
    with_run(|run| run.installed(true));
    assert_eq!(
        (
            trace().refused,
            trace().counters[C::SameFocusInstalls as usize]
        ),
        (0, 1)
    );
    with_run(|run| run.installed(false));
    assert!(only(R::HandlerReplaced));
    let _run = rearm(_run);
    tail();
    drop(C::HandlerRestores.restore_scope());
    C::HandlerRestores.took(true);
    permit();
    drop(C::QueryEntries.query());
    assert!(
        only(R::HandlerUnrestored),
        "a take without a set before the getter"
    );
    let _run = rearm(_run);
    P::AcceptedCut.record_edit(sel!(cut:));
    P::DownReturned.record();
    C::Presentations.presented();
    assert!(trace().refused == 0 && trace().counters[C::Presentations as usize] == 1);
    let _run = rearm(_run);
    tail();
    C::Presentations.presented();
    assert!(only(R::LatePresentation));
}

#[test]
fn order_depth_contention_overflow_and_ownership() {
    let _run = armed();
    P::AcceptedCut.record_edit(sel!(cut:));
    P::UpReturned.record();
    assert!(
        only(R::PhaseOrder) && phase(RECEIVER) == Some(P::AcceptedCut),
        "skipped"
    );
    let _run = rearm(_run);
    P::AcceptedCut.record_edit(sel!(copy:));
    assert!(only(R::NestedEditAction), "not a Cut");
    let _run = rearm(_run);
    let depth = C::DispatchDepth.scope();
    tail();
    drop(depth);
    assert!(only(R::PhaseOrder), "selector returned inside a dispatch");
    let run = rearm(_run);
    RUN.with(|slot| {
        let _held = slot.borrow();
        R::FrameTick.record();
    });
    assert!(run.finish(None).1.has_refusal(R::Contention));
    let _run = armed();
    with_run(|run| run.trace.counters[C::QueryExits as usize] = u16::MAX);
    drop(C::QueryEntries.query());
    assert!(trace().has_refusal(R::CounterOverflow));
    with_run(|run| (0..20).for_each(|_| run.push(None, None)));
    assert!(RUN.with_borrow(|run| run.as_ref().is_some_and(|run| run.exhausted)));
    assert_eq!(
        arm(RECEIVER, 9, FocusId::default()).err(),
        Some(NativeCutProbeError::CallbackLoanActive)
    );
    drop(Armed(u64::MAX));
    assert!(
        phase(RECEIVER).is_some() && phase(RECEIVER + 1).is_none(),
        "other runs stay armed"
    );
    drop(_run);
    NEXT_ID.set(None);
    assert_eq!(
        arm(RECEIVER, 9, FocusId::default()).err(),
        Some(NativeCutProbeError::IdentityExhausted)
    );
}

// Compiles only while NativeClient is !Send: two impls become ambiguous.
trait AmbiguousIfSend<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfSend<()> for T {}
impl<T: ?Sized + Send> AmbiguousIfSend<u8> for T {}
const _: fn() = || <super::super::NativeClient as AmbiguousIfSend<_>>::check();

// SF3: a body ends at the closing brace at the signature's own indent.
fn body<'a>(source: &'a str, anchor: &str) -> &'a str {
    assert_eq!(source.matches(anchor).count(), 1, "unique anchor {anchor}");
    let start = source.find(anchor).unwrap();
    let line = source[..start].rfind('\n').map_or(0, |i| i + 1);
    let indent = source[line..start].len() - source[line..start].trim_start().len();
    let close = format!("\n{}}}", &source[line..line + indent]);
    &source[start..start + source[start..].find(&close).unwrap()]
}

// Tier B: file | function anchor | hook. Removing or moving a hook fails.
const HOOKS: &str = "
actions | fn perform_edit_action(&self | AcceptedCut.record_edit(selector)
actions | fn dispatch_edit_action( | DownReturned.record()
actions | fn dispatch_edit_action( | UpReturned.record()
actions | fn dispatch_edit_action( | OriginRestored.record()
actions | fn dispatch_edit_action( | ModifiersRestored.record()
sync | fn refresh_snapshot(&self) | SnapshotEntered.scope()
sync | fn flush(&self) | TextInputFlush.record()
sync | fn notify(&self | DelegateNotify.record()
view | fn __selected_text_range(&self) | QueryEntries.query()
view | fn __set_selected_text_range(&self | Setter.record()
view | fn __resign_first_responder(&self) | Resign.record()
view | fn __will_move_to_window(&self | LeavingWindow.record()
gpui | fn drain() | AfterAction.record()
frame | fn tick(&self) | FrameTick.record()
frame | fn flush_input_handler_change(&self) | InputHandlerFlush.record()
window | fn set_text_input_configuration(&mut self | Configuration.record()
window | fn set_input_handler(&mut self | HandlerRestores.on_set(&self.window)
window | fn take_input_handler(&mut self) | HandlerRestores.on_take(&self.window)
window | fn draw(&self, scene: &Scene) | Presentations.presented()
window | fn show_soft_keyboard(&self) | KeyboardShown.record()
window | fn hide_soft_keyboard(&self) | KeyboardHidden.record()
window | fn text_input_state_changed(&self | FocusChange.on_state(&change)
target | pub(crate) fn dispatch(&self | DispatchDepth.scope()
target | pub(crate) fn with_input_handler<R>( | HandlerDepth.scope()
target | pub(super) fn with_input_handler<R>( | HandlerRestores.restore_scope()
probe | pub fn next_native_turn( | DeferredScheduled.record()
probe | pub fn next_native_turn( | DeferredExecuted.record()";

#[test]
fn every_passive_hook_is_gated_once_in_its_function() {
    let file = |name| match name {
        "actions" => include_str!("../../text_input/actions.rs"),
        "sync" => include_str!("../../text_input/sync.rs"),
        "view" => include_str!("../../text_input/view_class.rs"),
        "gpui" => include_str!("../../../gpui_call.rs"),
        "frame" => include_str!("../../frame.rs"),
        "window" => include_str!("../../platform_window.rs"),
        "target" => include_str!("../../input_target.rs"),
        "probe" => include_str!("../../retirement_probe.rs"),
        other => panic!("unknown file {other}"),
    };
    let rows: Vec<Vec<&str>> = HOOKS
        .trim()
        .lines()
        .map(|row| row.split(" | ").collect())
        .collect();
    assert_eq!(rows.len(), 27, "every W hook has a row");
    for row in rows {
        let [name, function, hook] = row[..] else {
            panic!("{row:?}")
        };
        let source = file(name);
        assert_eq!(source.matches(hook).count(), 1, "{hook} once per file");
        let body = body(source, function);
        let at = body
            .find(hook)
            .unwrap_or_else(|| panic!("{hook} outside {function}"));
        let gate = body[..body[..at].rfind('\n').unwrap()]
            .lines()
            .last()
            .unwrap();
        // retirement_probe.rs is gated as a whole module (window/mod.rs:42-43).
        let gated = name == "probe" || gate.trim() == "#[cfg(feature = \"test-support\")]";
        assert!(gated, "{hook} gated");
    }
}
