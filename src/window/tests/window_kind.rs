use gpui::WindowKind;

use crate::window::{Error, check_kind};

fn refusal(kind: WindowKind) -> Option<&'static str> {
    let error = check_kind(&kind).err()?;
    match error.downcast_ref::<Error>() {
        Some(Error::UnsupportedKind(kind)) => Some(*kind),
        None => panic!("unexpected error: {error}"),
    }
}

#[test]
fn a_normal_window_opens() {
    assert!(check_kind(&WindowKind::Normal).is_ok());
}

#[test]
fn windows_that_float_or_are_modal_are_refused() {
    assert_eq!(refusal(WindowKind::PopUp), Some("pop-up"));
    assert_eq!(refusal(WindowKind::Floating), Some("floating"));
    assert_eq!(refusal(WindowKind::Dialog), Some("dialog"));
}
