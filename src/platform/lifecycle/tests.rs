use gpui::AppLifecyclePhase::{Active, Background, Foreground, Inactive};

use super::*;

mod delivery;

const FIRST: usize = 0x1000;
const SECOND: usize = 0x2000;

fn phases(
    tracker: &mut LifecycleTracker,
    scene: usize,
    state: Option<SceneState>,
) -> Vec<AppLifecyclePhase> {
    tracker.update(scene, state).into_vec()
}

#[test]
fn launching_one_scene_reports_foreground_then_active() {
    let mut tracker = LifecycleTracker::default();

    assert_eq!(
        phases(&mut tracker, FIRST, Some(SceneState::Background)),
        []
    );
    assert_eq!(
        phases(&mut tracker, FIRST, Some(SceneState::Foreground)),
        [Foreground]
    );
    assert_eq!(
        phases(&mut tracker, FIRST, Some(SceneState::Active)),
        [Active]
    );
}

#[test]
fn leaving_and_returning_reports_each_phase_once() {
    let mut tracker = LifecycleTracker::default();
    phases(&mut tracker, FIRST, Some(SceneState::Active));

    assert_eq!(
        phases(&mut tracker, FIRST, Some(SceneState::Foreground)),
        [Inactive]
    );
    assert_eq!(
        phases(&mut tracker, FIRST, Some(SceneState::Background)),
        [Background]
    );
    assert_eq!(
        phases(&mut tracker, FIRST, Some(SceneState::Foreground)),
        [Foreground]
    );
    assert_eq!(
        phases(&mut tracker, FIRST, Some(SceneState::Active)),
        [Active]
    );
}

#[test]
fn a_second_scene_in_the_background_leaves_the_app_active() {
    let mut tracker = LifecycleTracker::default();
    phases(&mut tracker, FIRST, Some(SceneState::Active));
    phases(&mut tracker, SECOND, Some(SceneState::Active));

    assert_eq!(
        phases(&mut tracker, SECOND, Some(SceneState::Foreground)),
        []
    );
    assert_eq!(
        phases(&mut tracker, SECOND, Some(SceneState::Background)),
        []
    );
    assert_eq!(
        phases(&mut tracker, FIRST, Some(SceneState::Foreground)),
        [Inactive]
    );
}

#[test]
fn disconnecting_the_last_foreground_scene_backgrounds_the_app() {
    let mut tracker = LifecycleTracker::default();
    phases(&mut tracker, FIRST, Some(SceneState::Background));
    phases(&mut tracker, SECOND, Some(SceneState::Active));

    assert_eq!(phases(&mut tracker, SECOND, None), [Inactive, Background]);
    assert_eq!(phases(&mut tracker, FIRST, None), []);
}

#[test]
fn every_transition_ends_in_the_phase_it_names() {
    let states = [
        SceneState::Background,
        SceneState::Foreground,
        SceneState::Active,
    ];
    for from in states {
        for to in states {
            let last = transition(from, to).last().copied();
            let expected = match (from, to) {
                (from, to) if from == to => None,
                (_, SceneState::Background) => Some(Background),
                (_, SceneState::Active) => Some(Active),
                (SceneState::Background, SceneState::Foreground) => Some(Foreground),
                (_, SceneState::Foreground) => Some(Inactive),
            };
            assert_eq!(last, expected, "{from:?} -> {to:?}");
        }
    }
}
