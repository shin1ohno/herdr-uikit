//! The app's lifecycle phase, folded from its scenes'. UIKit reports
//! foreground, active, and background per scene, while GPUI's
//! `AppLifecyclePhase` describes the whole app: with several windows open,
//! one going to the background leaves the app in the foreground. The app is
//! as far forward as its furthest scene.

use std::collections::HashMap;

use gpui::AppLifecyclePhase;
use smallvec::{SmallVec, smallvec};

/// How far forward one scene is, in the order UIKit moves it.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum SceneState {
    /// Connected, but not on screen.
    #[default]
    Background,
    /// On screen without input: entering the foreground, or interrupted.
    Foreground,
    /// On screen and receiving input.
    Active,
}

/// The phases one scene event moved the app through, in order.
pub(super) type Phases = SmallVec<[AppLifecyclePhase; 2]>;

/// Scenes by identity (their object address while connected) and the app
/// state last reported for them.
#[derive(Debug, Default)]
pub(super) struct LifecycleTracker {
    app: SceneState,
    scenes: HashMap<usize, SceneState>,
}

impl LifecycleTracker {
    /// Records that `scene` moved to `state`, or disconnected when `state`
    /// is `None`, and returns the phases the app passed through.
    pub(super) fn update(&mut self, scene: usize, state: Option<SceneState>) -> Phases {
        match state {
            Some(state) => {
                self.scenes.insert(scene, state);
            }
            None => {
                self.scenes.remove(&scene);
            }
        }
        let app = self.scenes.values().copied().max().unwrap_or_default();
        let phases = transition(self.app, app);
        self.app = app;
        phases
    }
}

/// Phases on their way to GPUI, and what keeps the app running until GPUI
/// heard them. UIKit may suspend the app as soon as the delegate method that
/// moved its last scene to the background returns, which is before the queued
/// report reaches GPUI, so a background report holds a background task from
/// that delegate call until GPUI's callback for it returned.
pub(super) struct Delivery<H> {
    hold: Option<H>,
    phases: Phases,
}

impl<H> Delivery<H> {
    /// Calls `begin` now when `phases` take the app to the background.
    pub(super) fn new(phases: Phases, begin: impl FnOnce() -> Option<H>) -> Self {
        let hold = phases
            .contains(&AppLifecyclePhase::Background)
            .then(begin)
            .flatten();
        Self { hold, phases }
    }

    /// Reports each phase in order, then lets the hold go.
    pub(super) fn report(self, mut report: impl FnMut(AppLifecyclePhase)) {
        let Self { hold, phases } = self;
        for phase in phases {
            report(phase);
        }
        drop(hold);
    }
}

/// The phases between two app states. Becoming visible from the background
/// is `Foreground`; losing input while still visible is `Inactive`.
fn transition(from: SceneState, to: SceneState) -> Phases {
    match (from, to) {
        (SceneState::Background, SceneState::Foreground) => {
            smallvec![AppLifecyclePhase::Foreground]
        }
        (SceneState::Background, SceneState::Active) => {
            smallvec![AppLifecyclePhase::Foreground, AppLifecyclePhase::Active]
        }
        (SceneState::Foreground, SceneState::Active) => smallvec![AppLifecyclePhase::Active],
        (SceneState::Active, SceneState::Foreground) => smallvec![AppLifecyclePhase::Inactive],
        (SceneState::Active, SceneState::Background) => {
            smallvec![AppLifecyclePhase::Inactive, AppLifecyclePhase::Background]
        }
        (SceneState::Foreground, SceneState::Background) => {
            smallvec![AppLifecyclePhase::Background]
        }
        (SceneState::Background, SceneState::Background)
        | (SceneState::Foreground, SceneState::Foreground)
        | (SceneState::Active, SceneState::Active) => Phases::new(),
    }
}

#[cfg(test)]
mod tests;
