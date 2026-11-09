//! Queries before the application exists have no connected scenes.
pub(crate) fn scenes_or_empty<A, S>(
    application: Option<A>,
    scenes: impl FnOnce(A) -> Vec<S>,
) -> Vec<S> {
    application.map(scenes).unwrap_or_default()
}
