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
