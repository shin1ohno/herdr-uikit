use super::*;

use objc2::{ClassType, ProtocolType};
use objc2_ui_kit::{UIKeyInput, UITextInput, UITextInputTraits, UITextPosition, UITextRange};

// Registering a class runs objc2's debug checks: every required protocol
// method is implemented, every protocol method exists on the protocol, and
// overrides keep the superclass signatures. A mistake panics here rather
// than on the first key press.

#[test]
fn text_input_view_adopts_the_text_input_protocols() {
    let class = GPUITextInputView::class();
    let protocols = [
        ("UITextInput", <dyn UITextInput>::protocol()),
        ("UIKeyInput", <dyn UIKeyInput>::protocol()),
        ("UITextInputTraits", <dyn UITextInputTraits>::protocol()),
    ];
    for (name, protocol) in protocols {
        let Some(protocol) = protocol else {
            panic!("{name} is not in the Objective-C runtime");
        };
        assert!(class.conforms_to(protocol), "{name}");
    }
}

#[test]
fn position_and_range_classes_register() {
    let position = positions::TextPosition::class();
    let range = positions::TextRange::class();
    assert_eq!(position.superclass(), Some(UITextPosition::class()));
    assert_eq!(range.superclass(), Some(UITextRange::class()));
}

// On a Mac, AppKit takes control-A and other control letters bound to text
// commands before UIKit unless the responder lists key commands for them
// (`control_keys`). The view must override `keyCommands` and answer the
// commands' action.
#[test]
fn text_input_view_lists_its_own_key_commands_and_answers_control_keys() {
    use objc2::sel;
    let view = GPUITextInputView::class();
    let ui_view = UIView::class();
    let imp = |class: &objc2::runtime::AnyClass| {
        class
            .instance_method(sel!(keyCommands))
            .map(|method| method.implementation() as usize)
    };
    assert!(imp(view).is_some(), "keyCommands");
    assert_ne!(imp(view), imp(ui_view), "keyCommands is UIView's");
    assert!(
        view.instance_method(sel!(herdrControlKey:)).is_some(),
        "herdrControlKey:"
    );
}
