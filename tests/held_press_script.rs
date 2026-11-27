//! Host tests for the held-press phase script. The module is std only; the
//! library compiles it only for the iOS Simulator with `test-support`
//! (`src/window/retirement_probe.rs`), and this test target is never part of
//! a consumer build.
#[path = "../src/window/retirement_probe/held_press/script.rs"]
mod script;

use script::{
    HeldProbeDestination::{Current, Retired},
    HeldProbeKeySpec,
    HeldProbePhase::{self, Began, Cancelled, Changed, Ended},
    HeldProbeScript,
    HeldProbeScriptError::{AlreadyBegan, Finished, NotBegun, NotCurrent},
};

const OLD: u64 = 1;
const NEW: u64 = 2;

#[test]
fn escape_is_hid_usage_41_and_the_escape_character() {
    let escape = HeldProbeKeySpec::ESCAPE;
    assert_eq!((escape.usage, escape.characters), (41, "\u{1b}"));
}

#[test]
fn each_phase_goes_through_its_own_press_selector() {
    let selectors: Vec<_> = HeldProbePhase::ALL
        .iter()
        .map(|phase| phase.selector().to_str().unwrap_or_default())
        .collect();
    assert_eq!(
        selectors,
        [
            "pressesBegan:withEvent:",
            "pressesChanged:withEvent:",
            "pressesEnded:withEvent:",
            "pressesCancelled:withEvent:",
        ]
    );
}

#[test]
fn each_phase_reports_uikits_raw_press_phase() {
    let raw: Vec<_> = HeldProbePhase::ALL
        .iter()
        .map(|phase| phase.uikit_phase())
        .collect();
    assert_eq!(raw, [0, 1, 3, 4]);
    let finishing: Vec<_> = HeldProbePhase::ALL
        .iter()
        .map(|phase| phase.finishes())
        .collect();
    assert_eq!(finishing, [false, false, true, true]);
}

#[test]
fn began_changed_ended_on_the_current_view() {
    let mut script = HeldProbeScript::default();
    assert_eq!(script.step(Began, Current, OLD), Ok(()));
    assert!(script.began_on(OLD));
    assert_eq!(script.step(Changed, Current, OLD), Ok(()));
    assert!(!script.finished());
    assert_eq!(script.step(Ended, Current, OLD), Ok(()));
    assert!(script.finished());
}

#[test]
fn cancelled_finishes_the_press() {
    let mut script = HeldProbeScript::default();
    assert_eq!(script.step(Began, Current, OLD), Ok(()));
    assert_eq!(script.step(Cancelled, Current, OLD), Ok(()));
    assert!(script.finished());
    assert_eq!(script.step(Ended, Current, OLD), Err(Finished));
    assert_eq!(script.step(Began, Current, NEW), Err(Finished));
}

#[test]
fn release_before_began_is_refused_without_change() {
    for phase in [Changed, Ended, Cancelled] {
        let mut script = HeldProbeScript::default();
        assert_eq!(script.step(phase, Current, OLD), Err(NotBegun), "{phase:?}");
        assert_eq!(script, HeldProbeScript::default(), "{phase:?}");
    }
}

#[test]
fn a_view_sees_the_press_begin_once() {
    let mut script = HeldProbeScript::default();
    assert_eq!(script.step(Began, Current, OLD), Ok(()));
    assert_eq!(script.step(Began, Current, OLD), Err(AlreadyBegan));
}

#[test]
fn began_and_changed_never_go_to_a_retired_view() {
    let mut script = HeldProbeScript::default();
    assert_eq!(script.step(Began, Retired, OLD), Err(NotCurrent));
    assert_eq!(script.step(Began, Current, OLD), Ok(()));
    assert_eq!(script.step(Changed, Retired, OLD), Err(NotCurrent));
    assert!(!script.finished());
}

#[test]
fn redelivered_press_released_on_the_retired_view() {
    // The design's X2 order: begin on the old view, the old view retires,
    // UIKit begins the same press on the new view, the release reaches the
    // retired view.
    let mut script = HeldProbeScript::default();
    assert_eq!(script.step(Began, Current, OLD), Ok(()));
    assert_eq!(script.step(Began, Current, NEW), Ok(()));
    assert!(script.began_on(OLD) && script.began_on(NEW));
    assert_eq!(script.step(Ended, Retired, OLD), Ok(()));
    assert!(script.finished());
    assert_eq!(script.step(Ended, Current, NEW), Err(Finished));
}

#[test]
fn retired_view_that_never_saw_the_press_gets_no_release() {
    let mut script = HeldProbeScript::default();
    assert_eq!(script.step(Began, Current, NEW), Ok(()));
    assert_eq!(script.step(Cancelled, Retired, OLD), Err(NotBegun));
    assert!(!script.finished());
    assert_eq!(script.step(Cancelled, Current, NEW), Ok(()));
}
