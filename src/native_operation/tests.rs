//! Scope/continuation controls, not native or OS delivery evidence.
use std::rc::Weak;

use super::*;
use crate::NativeRetirementRequestError as Error;

fn origin(client: u64) -> NativeInputOrigin {
    NativeInputOrigin {
        window: Weak::new(),
        client,
    }
}

#[test]
fn only_explicit_edit_capability_or_tagged_task_admits() {
    let old = origin(7);
    assert_eq!(admission(&old), Err(Error::UnprotectedEntry));
    dispatcher_task(|| {
        assert_eq!(admission(&old), Ok(()), "tagged-task-positive");
        {
            let _raw = refused();
            assert_eq!(
                admission(&old),
                Err(Error::UnsupportedNativeEntry),
                "raw-ingress-does-not-inherit-task"
            );
        }
        assert_eq!(admission(&old), Ok(()), "raw-entry-restores-outer-task");
        dispatcher_task(|| assert_eq!(admission(&old), Err(Error::UnsupportedReentrancy)));
    });
    event(false, || {
        NativeOperation::edit(|op| {
            assert_eq!(admission(&old), Err(Error::UnsupportedNativeEntry));
            for phase in [Phase::EditDispatch, Phase::PasteFallback] {
                op.phase(old.clone(), phase, || {
                    assert_eq!(admission(&old), Ok(()), "classified-edit-positive");
                    assert_eq!(admission(&origin(8)), Err(Error::UnsupportedNativeEntry));
                    let _same_object_query = refused();
                    assert_eq!(
                        admission(&old),
                        Err(Error::UnsupportedNativeEntry),
                        "nested-origin-classification"
                    );
                });
            }
            op.phase(old.clone(), Phase::ObserveSnapshot, || {
                let _handler = loan();
                assert_eq!(
                    admission(&old),
                    Err(Error::UnsupportedNativeEntry),
                    "query-loan-is-not-admission"
                );
            });
        })
    });
    event(true, || {
        NativeOperation::edit(|op| {
            op.phase(old.clone(), Phase::EditDispatch, || {
                assert!(hardware_event());
                assert_eq!(
                    admission(&old),
                    Err(Error::UnsupportedHardwareContext),
                    "hardware-entry-never-admitted"
                );
            })
        })
    });
    assert!(state::quiescent());
}

#[test]
fn body_loans_and_scope_unwind_before_any_new_completion() {
    let old = origin(2);
    assert!(state::quiescent());
    {
        let _handler = loan();
        assert!(!state::quiescent(), "native-prepare-no-borrow");
        assert_eq!(loan_count_for_test(), 1);
    }
    assert!(state::quiescent());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        dispatcher_task(|| {
            let _callback = loan();
            let _raw = refused();
            assert_eq!(admission(&old), Err(Error::UnsupportedNativeEntry));
            panic!("scope cleanup control");
        });
    }));
    assert!(result.is_err());
    assert!(state::quiescent(), "scope-cleanup-on-unwind");
    assert_eq!(loan_count_for_test(), 0);
    dispatcher_task(|| assert_eq!(admission(&old), Ok(())));
}
