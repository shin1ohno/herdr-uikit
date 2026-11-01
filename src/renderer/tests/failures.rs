use super::*;

use std::error::Error as _;

use crate::renderer::error::SceneStats;

fn allocation(resource: &'static str) -> Error {
    Error::Allocation { resource }
}

fn frame(quads: usize, source: Error) -> Error {
    let mut scene = gpui::Scene::default();
    scene.quads.resize(quads, gpui::Quad::default());
    Error::Frame {
        stats: SceneStats::from(&scene),
        source: Box::new(source),
    }
}

#[test]
fn a_repeated_resource_failure_is_logged_once() {
    let mut log = FailureLog::default();
    assert_eq!(
        log.record(allocation("path texture")),
        Some(&allocation("path texture"))
    );
    assert!(log.record(allocation("path texture")).is_none());
    assert!(log.record(allocation("path texture")).is_none());
}

#[test]
fn a_different_resource_reports_a_new_failure() {
    let mut log = FailureLog::default();
    assert!(log.record(allocation("path texture")).is_some());
    assert_eq!(
        log.record(allocation("command buffer")),
        Some(&allocation("command buffer"))
    );
    assert!(log.record(allocation("command buffer")).is_none());
    assert!(log.record(allocation("path texture")).is_some());
}

#[test]
fn changing_scene_counts_and_frame_wrappers_do_not_repeat_the_cause() {
    let mut log = FailureLog::default();
    assert!(log.record(frame(1, allocation("path texture"))).is_some());
    assert!(log.record(frame(2, allocation("path texture"))).is_none());
    assert!(
        log.record(frame(3, frame(4, allocation("path texture"))))
            .is_none()
    );
    assert!(log.record(allocation("path texture")).is_none());
    assert!(log.record(frame(5, allocation("command buffer"))).is_some());
}

#[test]
fn a_success_rearms_the_same_failure() {
    let mut log = FailureLog::default();
    assert!(log.record(allocation("path texture")).is_some());
    assert!(log.record(allocation("path texture")).is_none());
    log.clear();
    assert!(log.record(allocation("path texture")).is_some());
}

#[test]
fn the_complete_first_error_and_typed_source_chain_are_retained() {
    let mut log = FailureLog::default();
    let error = frame(1, frame(2, allocation("path texture")));
    assert!(log.record(error).is_some());
    assert!(
        log.record(frame(3, frame(4, allocation("path texture"))))
            .is_none()
    );
    let Some(stored) = log.last.as_ref() else {
        panic!("the first diagnostic must remain available");
    };
    assert_eq!(stored, &frame(1, frame(2, allocation("path texture"))));
    let Some(inner) = stored
        .source()
        .and_then(|source| source.downcast_ref::<Box<Error>>())
        .map(Box::as_ref)
    else {
        panic!("the outer frame must retain its typed source");
    };
    assert_eq!(inner, &frame(2, allocation("path texture")));
    assert_eq!(
        inner
            .source()
            .and_then(|source| source.downcast_ref::<Box<Error>>())
            .map(Box::as_ref),
        Some(&allocation("path texture"))
    );
}
