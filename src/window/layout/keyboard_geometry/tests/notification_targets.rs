use super::*;

#[test]
fn an_unrelated_frame_preserves_the_guide_inset_instead_of_targeting_zero() {
    let view = rect(0.0, 0.0, 1032.0, 1376.0);
    let keyboard = rect(0.0, 920.0, 1032.0, 456.0);
    assert_eq!(screen_inset(view, view, keyboard, false), None);
    // An absent target leaves a previous guide sample or animation alone.
    let current_inset = 55.0;
    assert_eq!(
        screen_inset(view, view, keyboard, false).unwrap_or(current_inset),
        current_inset
    );
}

#[test]
fn a_current_hide_explicitly_targets_zero_instead_of_being_ignored() {
    let view = rect(0.0, 0.0, 1032.0, 1376.0);
    let keyboard = rect(0.0, 1376.0, 1032.0, 456.0);
    assert_eq!(screen_inset(view, view, keyboard, true), Some(0.0));
}
