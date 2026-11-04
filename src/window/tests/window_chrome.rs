use crate::window_controls::geometry;
use gpui::px;
use objc2_ui_kit::UIEdgeInsets;

#[test]
fn corner_controls_reserve_horizontal_room_without_lowering_content() {
    let safe = UIEdgeInsets {
        top: 0.,
        left: 4.,
        bottom: 20.,
        right: 8.,
    };
    let corner = UIEdgeInsets {
        top: 42.,
        left: 100.,
        bottom: 0.,
        right: 0.,
    };
    let (content, leading) = geometry(safe, Some(corner));
    assert_eq!(content, safe);
    assert_eq!(leading, px(96.));
}

#[test]
fn ordinary_safe_area_is_not_counted_twice_and_old_systems_add_no_clearance() {
    let safe = UIEdgeInsets {
        top: 24.,
        left: 20.,
        bottom: 20.,
        right: 0.,
    };
    assert_eq!(geometry(safe, None), (safe, px(0.)));
    assert_eq!(
        geometry(
            safe,
            Some(UIEdgeInsets {
                top: 0.,
                left: 0.,
                bottom: 0.,
                right: 0.
            })
        ),
        (safe, px(0.))
    );
    assert_eq!(geometry(safe, Some(safe)), (safe, px(0.)));
}
