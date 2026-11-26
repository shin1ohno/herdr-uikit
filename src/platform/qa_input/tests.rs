//! The receipts' fields, and that every call into the module is compiled
//! for the Simulator only.

use std::{
    collections::BTreeMap,
    fmt::Debug,
    sync::{Arc, Mutex, PoisonError},
};

use tracing::{
    Event, Metadata, Subscriber,
    field::{Field, Visit},
    span,
};

use super::*;

/// One receipt's fields, by name; strings without quotes.
pub(in crate::platform) type Receipt = BTreeMap<String, String>;

/// A subscriber keeping the receipts emitted while it is the default.
#[derive(Clone, Default)]
pub(in crate::platform) struct Capture(Arc<Mutex<Vec<Receipt>>>);

impl Capture {
    /// Runs `work` with this subscriber as the thread's default.
    pub(in crate::platform) fn during<R>(&self, work: impl FnOnce() -> R) -> R {
        tracing::subscriber::with_default(self.clone(), work)
    }

    /// The receipts kept so far, which are then forgotten.
    pub(in crate::platform) fn take(&self) -> Vec<Receipt> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

struct Fields(Receipt);

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
}

impl Subscriber for Capture {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
        span::Id::from_u64(1)
    }

    fn record(&self, _: &span::Id, _: &span::Record<'_>) {}

    fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}

    fn event(&self, event: &Event<'_>) {
        if event.metadata().target() != TARGET {
            return;
        }
        let mut fields = Fields(Receipt::new());
        event.record(&mut fields);
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(fields.0);
    }

    fn enter(&self, _: &span::Id) {}

    fn exit(&self, _: &span::Id) {}
}

/// A receipt with these fields.
pub(in crate::platform) fn receipt(fields: &[(&str, &str)]) -> Receipt {
    fields
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn a_touch_receipt_names_its_stage_kind_position_and_count() {
    let capture = Capture::default();
    capture.during(|| {
        touch(TouchStage::Began, false, 333.0, 910.5, 1);
        touch(TouchStage::Ended, true, 12.0, 4.0, 2);
        touch(TouchStage::Cancelled, false, 0.0, 0.0, 1);
    });
    assert_eq!(
        capture.take(),
        [
            receipt(&[
                ("receipt", "touch"),
                ("stage", "began"),
                ("kind", "direct"),
                ("x", "333.0"),
                ("y", "910.5"),
                ("touches", "1"),
            ]),
            receipt(&[
                ("receipt", "touch"),
                ("stage", "ended"),
                ("kind", "pointer"),
                ("x", "12.0"),
                ("y", "4.0"),
                ("touches", "2"),
            ]),
            receipt(&[
                ("receipt", "touch"),
                ("stage", "cancelled"),
                ("kind", "direct"),
                ("x", "0.0"),
                ("y", "0.0"),
                ("touches", "1"),
            ]),
        ]
    );
}

#[test]
fn a_menu_perform_receipt_tells_a_missing_sender_and_tag_from_a_command() {
    let capture = Capture::default();
    let tag = MenuTag::new(7, 3);
    assert!(tag.is_some());
    capture.during(|| {
        menu_perform(false, None, Some(7), false);
        menu_perform(true, None, None, false);
        menu_perform(true, tag, Some(8), true);
    });
    assert_eq!(
        capture.take(),
        [
            receipt(&[
                ("receipt", "menu-perform"),
                ("outcome", "no_sender"),
                ("current", "7"),
            ]),
            receipt(&[
                ("receipt", "menu-perform"),
                ("outcome", "no_tag"),
                ("current", "-1"),
            ]),
            receipt(&[
                ("receipt", "menu-perform"),
                ("outcome", "performed"),
                ("generation", "7"),
                ("index", "3"),
                ("current", "8"),
                ("keyed", "true"),
            ]),
        ]
    );
}

#[test]
fn a_menu_deliver_receipt_names_each_outcome() {
    let Some(tag) = MenuTag::new(2, 0) else {
        panic!("no tag");
    };
    let capture = Capture::default();
    capture.during(|| {
        for delivery in [
            Delivery::StaleGeneration,
            Delivery::NoCallback,
            Delivery::CallbackRunning,
            Delivery::Delivered,
        ] {
            menu_deliver(tag, 3, delivery);
        }
    });
    let outcomes: Vec<_> = capture
        .take()
        .into_iter()
        .map(|receipt| {
            assert_eq!(
                receipt.get("receipt").map(String::as_str),
                Some("menu-deliver")
            );
            assert_eq!(receipt.get("generation").map(String::as_str), Some("2"));
            assert_eq!(receipt.get("index").map(String::as_str), Some("0"));
            assert_eq!(receipt.get("current").map(String::as_str), Some("3"));
            receipt.get("outcome").cloned().unwrap_or_default()
        })
        .collect();
    assert_eq!(
        outcomes,
        [
            "stale_generation",
            "no_callback",
            "callback_running",
            "delivered"
        ]
    );
}

#[test]
fn other_targets_are_not_receipts() {
    let capture = Capture::default();
    capture.during(|| tracing::info!(target: "herdr_uikit::other", receipt = "touch"));
    assert!(capture.take().is_empty());
}

/// The sources that call into this module.
const CALLERS: [(&str, &str); 3] = [
    ("src/window/touch.rs", include_str!("../../window/touch.rs")),
    ("src/platform/menus.rs", include_str!("../menus.rs")),
    ("src/renderer/mod.rs", include_str!("../../renderer/mod.rs")),
];

const SIMULATOR_ONLY: &str = r#"#[cfg(all(target_os = "ios", target_abi = "sim"))]"#;

/// The touch callbacks reach `Window::handle_touches`, which needs a live
/// window, so its receipt is checked in the source: removing the call, or
/// recording after the touch is dispatched, fails here. The menu receipts
/// are exercised with real commands in `menus::tests::commands`.
#[test]
fn every_touch_records_its_receipt_before_it_is_dispatched() {
    let (_, touch) = CALLERS[0];
    let handle = touch
        .split("fn handle_touches(")
        .nth(1)
        .and_then(|rest| rest.split("\n    fn ").next());
    let Some(handle) = handle else {
        panic!("no handle_touches");
    };
    let receipt = handle.find("self.receipt(&touch, stage, touches.count());");
    let dispatch = handle.find("self.pointer_touch(");
    assert!(
        receipt.is_some() && receipt < dispatch,
        "handle_touches records each touch first"
    );
    assert!(touch.contains("qa_input::touch("));
}

/// A device build compiles none of this: the module and every statement or
/// item that names it or the helpers around it carry the Simulator `cfg`.
#[test]
fn every_receipt_is_compiled_for_the_simulator_only() {
    let module = include_str!("../mod.rs");
    assert!(module.contains(&format!("{SIMULATOR_ONLY}\npub(crate) mod qa_input;")));
    for (path, source) in CALLERS {
        let lines: Vec<&str> = source.lines().collect();
        for (number, line) in lines.iter().enumerate() {
            let code = line.trim_start();
            let names_receipt = code.starts_with("fn receipt(")
                || code.starts_with("fn current_generation(")
                || code.starts_with("self.receipt(")
                || code.starts_with("receipt(")
                || code.starts_with("super::qa_input::menu_perform(")
                || code.starts_with("frame_runs:")
                || code.starts_with("if let Some((previous, count)) = self.frame_runs.note(");
            if !names_receipt {
                continue;
            }
            let guard = lines[..number]
                .iter()
                .rev()
                .map(|line| line.trim())
                .find(|line| !line.starts_with("///"));
            assert_eq!(guard, Some(SIMULATOR_ONLY), "{path}:{}", number + 1);
        }
    }
}

#[test]
fn frame_runs_report_only_a_change_of_outcome_with_the_run_it_ended() {
    let mut runs = FrameRuns::default();
    assert_eq!(
        runs.note(FrameOutcome::ZeroSize),
        Some((None, 0)),
        "the first draw"
    );
    assert_eq!(runs.note(FrameOutcome::ZeroSize), None);
    assert_eq!(
        runs.note(FrameOutcome::Presented),
        Some((Some(FrameOutcome::ZeroSize), 2))
    );
    for _ in 0..59 {
        assert_eq!(
            runs.note(FrameOutcome::Presented),
            None,
            "a steady outcome stays quiet"
        );
    }
    assert_eq!(
        runs.note(FrameOutcome::NoDrawable),
        Some((Some(FrameOutcome::Presented), 60))
    );
    assert_eq!(
        runs.note(FrameOutcome::Presented),
        Some((Some(FrameOutcome::NoDrawable), 1))
    );
}

#[test]
fn a_frame_receipt_names_its_outcome_and_the_run_before_it() {
    let capture = Capture::default();
    capture.during(|| {
        frame(FrameOutcome::PathsNotReady, None, 0);
        frame(
            FrameOutcome::Presented,
            Some(FrameOutcome::PathsNotReady),
            3,
        );
        frame(
            FrameOutcome::RenderFailed,
            Some(FrameOutcome::Presented),
            120,
        );
        frame(FrameOutcome::ZeroSize, Some(FrameOutcome::RenderFailed), 1);
        frame(FrameOutcome::NoDrawable, Some(FrameOutcome::ZeroSize), 2);
    });
    let names = |outcome: &str, previous: &str, count: &str| {
        receipt(&[
            ("receipt", "frame"),
            ("outcome", outcome),
            ("previous", previous),
            ("previous_count", count),
        ])
    };
    assert_eq!(
        capture.take(),
        [
            names("paths_not_ready", "none", "0"),
            names("presented", "paths_not_ready", "3"),
            names("render_failed", "presented", "120"),
            names("zero_size", "render_failed", "1"),
            names("no_drawable", "zero_size", "2"),
        ]
    );
}

/// Every draw goes through `draw_frame`, whose outcome the Simulator build
/// folds into runs and reports: removing the note, or drawing elsewhere, fails.
#[test]
fn every_draw_notes_its_outcome() {
    let (_, renderer) = CALLERS[2];
    let draw = renderer
        .split("pub(crate) fn draw(&mut self, scene: &Scene) {")
        .nth(1)
        .and_then(|rest| rest.split("\n    fn ").next());
    let Some(draw) = draw else {
        panic!("no Renderer::draw");
    };
    let outcome = draw.find("let outcome = self.draw_frame(scene);");
    let note = draw.find("self.frame_runs.note(outcome)");
    assert!(
        outcome.is_some() && outcome < note,
        "draw notes the outcome of draw_frame"
    );
    assert!(draw.contains("crate::platform::qa_input::frame(outcome, previous, count);"));
}
