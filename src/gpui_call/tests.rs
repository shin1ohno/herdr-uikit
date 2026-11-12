use super::*;

#[test]
fn gpui_runs_while_a_guard_lives_and_guards_nest() {
    assert!(!is_running());
    let outer = InGpui::enter();
    {
        let _inner = InGpui::enter();
        assert!(is_running());
    }
    assert!(is_running());
    drop(outer);
    assert!(!is_running());
}

#[test]
fn native_work_runs_after_outer_update_once_in_order() {
    use std::{cell::RefCell, rc::Rc};
    let seen = Rc::new(RefCell::new(Vec::new()));
    let outer = InGpui::enter();
    {
        let _inner = InGpui::enter();
        for value in [1, 2] {
            let seen = seen.clone();
            after_update(move || {
                assert!(!is_running());
                seen.borrow_mut().push(value);
            });
        }
    }
    assert!(seen.borrow().is_empty());
    drop(outer);
    assert_eq!(*seen.borrow(), [1, 2]);
    let seen2 = seen.clone();
    after_update(move || seen2.borrow_mut().push(3));
    assert_eq!(*seen.borrow(), [1, 2, 3]);
}

#[test]
fn work_scheduled_during_native_completion_does_not_reenter() {
    use std::{cell::RefCell, rc::Rc};
    let seen = Rc::new(RefCell::new(Vec::new()));
    let seen2 = seen.clone();
    after_update(move || {
        let nested = seen2.clone();
        after_update(move || nested.borrow_mut().push(2));
        seen2.borrow_mut().push(1);
    });
    assert_eq!(*seen.borrow(), [1, 2]);
}
