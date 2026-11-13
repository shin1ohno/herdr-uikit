use crate::{
    native_operation::{admission, dispatcher_task},
    *,
};
use std::{cell::Cell, rc::Weak};
thread_local! {
    static PRESENT: Cell<bool> = const { Cell::new(true) };
    static CALLS: Cell<u32> = const { Cell::new(0) };
}
struct Shared;
fn current() -> Option<Shared> {
    PRESENT.get().then_some(Shared)
}
fn missing_shared() {
    CALLS.set(CALLS.get() + 1);
}
struct Scene;
impl Scene {
    fn into_inner(self, _: MainThreadMarker) -> Self {
        self
    }
}
fn expected_refusal() {
    let origin = NativeInputOrigin {
        window: Weak::new(),
        client: 0,
    };
    assert_eq!(
        admission(&origin),
        Err(NativeRetirementRequestError::UnsupportedNativeEntry),
        "raw-ingress-does-not-inherit-task"
    );
    CALLS.set(CALLS.get() + 1);
}
fn destroy(_: &Scene, _: MainThreadMarker) {
    expected_refusal();
}
fn scene_request_refused(_: u64, _: MainThreadMarker) {
    expected_refusal();
}
fn shared_body(work: impl FnOnce(&Shared)) {
    SHARED_BODY
}
fn destroy_body(scene: Scene) {
    DESTROY_BODY
}
fn refusal_body(id: u64) {
    REFUSAL_BODY
}

#[test]
fn actual_raw_bodies_do_not_inherit_dispatcher_task_and_restore_normal_paths() {
    let origin = NativeInputOrigin {
        window: Weak::new(),
        client: 0,
    };
    for case in 0..3 {
        CALLS.set(0);
        dispatcher_task(|| {
            assert_eq!(admission(&origin), Ok(()));
            match case {
                0 => shared_body(|_| expected_refusal()),
                1 => destroy_body(Scene),
                _ => refusal_body(0),
            }
            assert_eq!(admission(&origin), Ok(()), "raw-scope-restores-task");
        });
        assert_eq!(CALLS.get(), 1, "raw-original-body-once");
    }
    PRESENT.set(false);
    CALLS.set(0);
    dispatcher_task(|| {
        shared_body(|_| panic!("missing shared must not call work"));
        assert_eq!(admission(&origin), Ok(()), "raw-empty-branch-unwinds");
    });
    assert_eq!(CALLS.get(), 1);
    PRESENT.set(true);
    let result = std::panic::catch_unwind(|| {
        dispatcher_task(|| shared_body(|_| panic!("raw unwind control")))
    });
    assert!(result.is_err());
    dispatcher_task(|| assert_eq!(admission(&origin), Ok(()), "raw-panic-scope-unwinds"));
}
