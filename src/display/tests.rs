#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::rc::{self, Rc};

use objc2_core_foundation::CGPoint;

use super::*;

fn external(width: f64, height: f64) -> ScreenKind {
    ScreenKind::External(CGSize { width, height })
}

/// Stands in for `objc2::rc::Weak<UIScreen>`: a screen is an `Rc`, and
/// dropping it disconnects the screen.
fn id_of(table: &mut IdTable<rc::Weak<()>>, screen: &Rc<()>) -> DisplayId {
    table.id_for(
        |known| known.upgrade().is_some(),
        |known| {
            known
                .upgrade()
                .is_some_and(|known| Rc::ptr_eq(&known, screen))
        },
        || Rc::downgrade(screen),
    )
}

#[test]
fn display_uuids_are_name_based_and_stable() {
    let builtin = ScreenKind::BuiltIn.uuid();
    assert_eq!(builtin, ScreenKind::BuiltIn.uuid());
    assert_eq!(builtin.get_version_num(), 5);
    assert_eq!(external(3840., 2160.).uuid(), external(3840., 2160.).uuid());
}

#[test]
fn display_uuids_tell_screens_apart() {
    let builtin = ScreenKind::BuiltIn.uuid();
    let uhd = external(3840., 2160.).uuid();
    let qhd = external(2560., 1440.).uuid();
    assert_ne!(builtin, uhd);
    assert_ne!(uhd, qhd);
    assert_ne!(external(2160., 3840.).uuid(), uhd);
}

#[test]
fn display_uuids_ignore_sub_pixel_noise() {
    assert_eq!(
        external(3840.2, 2159.8).uuid(),
        external(3840., 2160.).uuid()
    );
}

#[test]
fn ids_start_at_one_and_stay_with_their_screen() {
    let mut table = IdTable::new();
    let builtin = Rc::new(());
    let monitor = Rc::new(());

    let builtin_id = id_of(&mut table, &builtin);
    let monitor_id = id_of(&mut table, &monitor);

    assert_eq!(u64::from(builtin_id), 1);
    assert_ne!(builtin_id, monitor_id);
    assert_eq!(id_of(&mut table, &builtin), builtin_id);
    assert_eq!(id_of(&mut table, &monitor), monitor_id);
}

#[test]
fn a_reconnected_screen_never_takes_an_old_id() {
    let mut table = IdTable::new();
    let builtin = Rc::new(());
    let builtin_id = id_of(&mut table, &builtin);
    let first_monitor = Rc::new(());
    let first_id = id_of(&mut table, &first_monitor);

    drop(first_monitor);
    let second_monitor = Rc::new(());
    let second_id = id_of(&mut table, &second_monitor);

    assert_ne!(second_id, first_id);
    assert_ne!(second_id, builtin_id);
    assert_eq!(id_of(&mut table, &builtin), builtin_id);
}

#[test]
fn disconnected_screens_leave_the_table() {
    let mut table = IdTable::new();
    let builtin = Rc::new(());
    id_of(&mut table, &builtin);
    for _ in 0..3 {
        let monitor = Rc::new(());
        id_of(&mut table, &monitor);
    }

    id_of(&mut table, &builtin);
    assert_eq!(table.entries.len(), 1);
}

const ACTIVE: UISceneActivationState = UISceneActivationState::ForegroundActive;
const INACTIVE: UISceneActivationState = UISceneActivationState::ForegroundInactive;

/// A scene in `state` on `screen`, the display with session id `id`.
fn scene(
    state: UISceneActivationState,
    screen: &'static str,
    id: u64,
) -> (ScreenOrder, &'static str) {
    let order = ScreenOrder {
        rank: activation_rank(state),
        external: screen != "builtin",
        id,
    };
    (order, screen)
}

fn ordered(scenes: &[(ScreenOrder, &'static str)]) -> Vec<&'static str> {
    order_screens(scenes.iter().copied(), |a, b| a == b)
}

#[test]
fn equally_active_screens_put_the_device_screen_first() {
    let builtin = scene(ACTIVE, "builtin", 2);
    let monitor = scene(ACTIVE, "monitor", 1);
    assert_eq!(ordered(&[monitor, builtin]), ["builtin", "monitor"]);
    assert_eq!(ordered(&[builtin, monitor]), ["builtin", "monitor"]);
}

#[test]
fn equally_active_monitors_keep_the_order_the_app_saw_them() {
    let first = scene(ACTIVE, "first", 2);
    let second = scene(ACTIVE, "second", 3);
    assert_eq!(ordered(&[second, first]), ["first", "second"]);
    assert_eq!(ordered(&[first, second]), ["first", "second"]);
}

#[test]
fn the_most_active_scene_outranks_the_device_screen() {
    let builtin = scene(INACTIVE, "builtin", 1);
    let monitor = scene(ACTIVE, "monitor", 2);
    assert_eq!(ordered(&[builtin, monitor]), ["monitor", "builtin"]);
}

#[test]
fn a_screen_with_several_scenes_is_listed_once_by_its_most_active() {
    let builtin = scene(INACTIVE, "builtin", 1);
    let idle_on_monitor = scene(INACTIVE, "monitor", 2);
    let active_on_monitor = scene(ACTIVE, "monitor", 2);
    assert_eq!(
        ordered(&[idle_on_monitor, builtin, active_on_monitor]),
        ["monitor", "builtin"]
    );
}

#[test]
fn scenes_order_from_active_to_detached() {
    let ranks = [
        UISceneActivationState::ForegroundActive,
        UISceneActivationState::ForegroundInactive,
        UISceneActivationState::Background,
        UISceneActivationState::Unattached,
    ]
    .map(activation_rank);
    assert!(ranks.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn screen_bounds_keep_uikit_points_and_top_left_origin() {
    let rect = CGRect {
        origin: CGPoint { x: 0., y: 0. },
        size: CGSize {
            width: 1366.,
            height: 1024.,
        },
    };
    let bounds = bounds_from_rect(rect);
    assert_eq!(bounds.origin, point(px(0.), px(0.)));
    assert_eq!(bounds.size, size(px(1366.), px(1024.)));
}
