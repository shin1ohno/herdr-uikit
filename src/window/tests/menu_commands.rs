use objc2::{ClassType, runtime::AnyClass, sel};

use crate::platform::menus;
use crate::window::view::GpuiView;

#[test]
fn the_drawing_view_performs_and_validates_menu_commands() {
    let class: &AnyClass = GpuiView::class();

    for selector in [menus::action_selector(), sel!(validateCommand:)] {
        assert!(class.instance_method(selector).is_some(), "{selector:?}");
    }
}
