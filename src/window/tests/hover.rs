use super::*;

#[test]
fn a_release_over_the_view_or_on_its_edge_is_inside() {
    let view = rect(0.0, 0.0, 1000.0, 800.0);
    assert!(convert::contains(view, CGPoint::new(500.0, 400.0)));
    assert!(convert::contains(view, CGPoint::new(0.0, 0.0)));
    assert!(convert::contains(view, CGPoint::new(1000.0, 800.0)));
}

#[test]
fn a_release_past_any_edge_is_outside() {
    let view = rect(0.0, 0.0, 1000.0, 800.0);
    assert!(!convert::contains(view, CGPoint::new(-1.0, 400.0)));
    assert!(!convert::contains(view, CGPoint::new(1000.5, 400.0)));
    assert!(!convert::contains(view, CGPoint::new(500.0, -0.5)));
    assert!(!convert::contains(view, CGPoint::new(500.0, 801.0)));
}
