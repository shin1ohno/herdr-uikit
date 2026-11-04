mod hover;
mod menu_commands;
mod scene_requests;
mod text_state;
mod window_chrome;
mod window_kind;

use std::time::{Duration, Instant};

use gpui::MouseButton;
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_ui_kit::{UIEventButtonMask, UIKeyModifierFlags};

use super::{convert, frame::InsetAnimation, touch::TouchIds};

fn rect(x: f64, y: f64, width: f64, height: f64) -> CGRect {
    CGRect::new(CGPoint::new(x, y), CGSize::new(width, height))
}

#[test]
fn a_touch_id_is_never_reused_for_a_later_touch() {
    let mut ids = TouchIds::<u32>::default();
    let first = ids.start(7);
    assert_eq!(ids.end(&7), Some(first));
    // UIKit can hand a later touch the same object address.
    let second = ids.start(7);
    assert_ne!(first, second);
    assert_eq!(ids.get(&7), Some(second));
}

#[test]
fn concurrent_touches_keep_their_own_ids() {
    let mut ids = TouchIds::<u32>::default();
    let a = ids.start(1);
    let b = ids.start(2);
    assert_ne!(a, b);
    assert_eq!(ids.get(&1), Some(a));
    assert_eq!(ids.end(&2), Some(b));
    assert_eq!(ids.get(&1), Some(a));
    assert_eq!(ids.get(&2), None);
}

#[test]
fn a_touch_that_never_began_here_has_no_id() {
    let mut ids = TouchIds::<u32>::default();
    assert_eq!(ids.get(&3), None);
    assert_eq!(ids.end(&3), None);
}

#[test]
fn restarting_a_live_touch_replaces_its_id() {
    let mut ids = TouchIds::<u32>::default();
    let stale = ids.start(1);
    let fresh = ids.start(1);
    assert_ne!(stale, fresh);
    assert_eq!(ids.end(&1), Some(fresh));
    assert_eq!(ids.end(&1), None);
}

#[test]
fn clearing_touches_keeps_ids_counting() {
    let mut ids = TouchIds::<u32>::default();
    let before = ids.start(1);
    ids.clear();
    assert_eq!(ids.get(&1), None);
    assert_ne!(ids.start(1), before);
}

#[test]
fn the_keyboard_inset_animation_lands_on_its_target() {
    let start = Instant::now();
    let animation = InsetAnimation::new(0.0, 300.0, start, Duration::from_millis(250));
    assert_eq!(animation.sample(start), (0.0, false));
    let (halfway, finished) = animation.sample(start + Duration::from_millis(125));
    assert!(!finished);
    // Ease-out covers more than half the distance in half the time.
    assert!(halfway > 150.0 && halfway < 300.0, "{halfway}");
    assert_eq!(
        animation.sample(start + Duration::from_millis(250)),
        (300.0, true)
    );
    assert_eq!(
        animation.sample(start + Duration::from_secs(1)),
        (300.0, true)
    );
}

#[test]
fn the_keyboard_inset_animation_moves_one_way() {
    let start = Instant::now();
    let animation = InsetAnimation::new(320.0, 0.0, start, Duration::from_millis(200));
    let mut previous = f32::INFINITY;
    for step in 0..=20 {
        let (inset, _) = animation.sample(start + Duration::from_millis(step * 10));
        assert!(inset <= previous, "{inset} after {previous}");
        previous = inset;
    }
    assert_eq!(previous, 0.0);
}

#[test]
fn a_keyboard_animation_without_duration_jumps() {
    let start = Instant::now();
    let animation = InsetAnimation::new(10.0, 40.0, start, Duration::ZERO);
    assert_eq!(animation.sample(start), (40.0, true));
}

#[test]
fn a_keyboard_animation_sampled_before_it_starts_has_not_moved() {
    let now = Instant::now();
    let animation = InsetAnimation::new(
        10.0,
        40.0,
        now + Duration::from_secs(1),
        Duration::from_secs(1),
    );
    assert_eq!(animation.sample(now), (10.0, false));
}

#[test]
fn a_docked_keyboard_covers_the_bottom_of_the_view() {
    let view = rect(0.0, 0.0, 1000.0, 800.0);
    assert_eq!(
        convert::keyboard_overlap(view, rect(0.0, 500.0, 1000.0, 300.0)),
        300.0
    );
    // A hardware keyboard leaves only the shortcuts bar.
    assert_eq!(
        convert::keyboard_overlap(view, rect(0.0, 745.0, 1000.0, 55.0)),
        55.0
    );
}

#[test]
fn a_keyboard_below_or_beside_the_view_covers_nothing() {
    let view = rect(0.0, 0.0, 1000.0, 800.0);
    // Hidden: moved below the screen.
    assert_eq!(
        convert::keyboard_overlap(view, rect(0.0, 800.0, 1000.0, 300.0)),
        0.0
    );
    // Floating, away from the bottom edge.
    assert_eq!(
        convert::keyboard_overlap(view, rect(300.0, 200.0, 320.0, 250.0)),
        0.0
    );
    // Docked under a Stage Manager window to the side.
    assert_eq!(
        convert::keyboard_overlap(view, rect(1100.0, 500.0, 400.0, 300.0)),
        0.0
    );
    assert_eq!(
        convert::keyboard_overlap(view, rect(0.0, 500.0, 1000.0, 0.0)),
        0.0
    );
}

#[test]
fn a_keyboard_taller_than_the_view_covers_all_of_it() {
    let view = rect(0.0, 100.0, 500.0, 400.0);
    assert_eq!(
        convert::keyboard_overlap(view, rect(0.0, 0.0, 500.0, 600.0)),
        400.0
    );
}

#[test]
fn key_modifier_flags_become_gpui_modifiers() {
    let modifiers = convert::modifiers(UIKeyModifierFlags::Command | UIKeyModifierFlags::Shift);
    assert!(modifiers.platform && modifiers.shift);
    assert!(!modifiers.control && !modifiers.alt && !modifiers.function);

    let modifiers = convert::modifiers(UIKeyModifierFlags::Control | UIKeyModifierFlags::Alternate);
    assert!(modifiers.control && modifiers.alt);
    assert!(!modifiers.platform && !modifiers.shift);

    assert!(convert::capslock(UIKeyModifierFlags::AlphaShift).on);
    assert!(!convert::capslock(UIKeyModifierFlags::Shift).on);
}

#[test]
fn pointer_button_masks_become_mouse_buttons() {
    assert_eq!(
        convert::mouse_button(UIEventButtonMask::Primary),
        MouseButton::Left
    );
    assert_eq!(
        convert::mouse_button(UIEventButtonMask::Secondary),
        MouseButton::Right
    );
    assert_eq!(
        convert::mouse_button(UIEventButtonMask(1 << 2)),
        MouseButton::Middle
    );
    assert_eq!(
        convert::mouse_button(UIEventButtonMask::empty()),
        MouseButton::Left
    );
}
