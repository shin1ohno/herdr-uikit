use super::*;

use keymap::HeldModifiers;

#[test]
fn flags_map_to_gpui_modifiers() {
    let flags = UIKeyModifierFlags::Command
        .union(UIKeyModifierFlags::Control)
        .union(UIKeyModifierFlags::Alternate)
        .union(UIKeyModifierFlags::Shift)
        .union(UIKeyModifierFlags::AlphaShift)
        .union(UIKeyModifierFlags::NumericPad);
    assert_eq!(
        keymap::modifiers(flags),
        Modifiers {
            control: true,
            alt: true,
            shift: true,
            platform: true,
            function: false,
        }
    );
    assert_eq!(
        keymap::modifiers(UIKeyModifierFlags::empty()),
        Modifiers::default()
    );
    assert_eq!(
        keymap::modifiers(UIKeyModifierFlags::Command),
        Modifiers::command()
    );
}

#[test]
fn caps_lock_is_separate_from_the_modifiers() {
    assert!(keymap::capslock(UIKeyModifierFlags::AlphaShift));
    assert!(!keymap::capslock(UIKeyModifierFlags::Shift));
    assert_eq!(
        keymap::modifiers(UIKeyModifierFlags::AlphaShift),
        Modifiers::default()
    );
    assert!(keymap::is_capslock(UIKeyboardHIDUsage::KeyboardCapsLock));
    assert!(!keymap::is_capslock(UIKeyboardHIDUsage::KeyboardA));
}

#[test]
fn only_the_eight_modifier_usages_are_modifier_keys() {
    let usages = [
        UIKeyboardHIDUsage::KeyboardLeftControl,
        UIKeyboardHIDUsage::KeyboardLeftShift,
        UIKeyboardHIDUsage::KeyboardLeftAlt,
        UIKeyboardHIDUsage::KeyboardLeftGUI,
        UIKeyboardHIDUsage::KeyboardRightControl,
        UIKeyboardHIDUsage::KeyboardRightShift,
        UIKeyboardHIDUsage::KeyboardRightAlt,
        UIKeyboardHIDUsage::KeyboardRightGUI,
    ];
    let bits: Vec<u8> = usages
        .iter()
        .filter_map(|usage| keymap::modifier_bit(*usage))
        .collect();
    assert_eq!(bits.len(), usages.len());
    let mut unique = bits.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        bits.len(),
        "each modifier key has its own bit"
    );
    assert!(bits.iter().all(|bit| bit.count_ones() == 1));
    assert_eq!(keymap::modifier_bit(UIKeyboardHIDUsage::KeyboardA), None);
    assert_eq!(
        keymap::modifier_bit(UIKeyboardHIDUsage::KeyboardCapsLock),
        None
    );
}

fn bit(usage: UIKeyboardHIDUsage) -> u8 {
    keymap::modifier_bit(usage).unwrap_or_default()
}

#[test]
fn held_modifiers_track_each_side() {
    let mut held = HeldModifiers::default();
    held.set(bit(UIKeyboardHIDUsage::KeyboardLeftShift), true);
    held.set(bit(UIKeyboardHIDUsage::KeyboardRightShift), true);
    assert_eq!(held.modifiers(), Modifiers::shift());
    held.set(bit(UIKeyboardHIDUsage::KeyboardLeftShift), false);
    assert_eq!(
        held.modifiers(),
        Modifiers::shift(),
        "the right shift is still down"
    );
    held.set(bit(UIKeyboardHIDUsage::KeyboardRightShift), false);
    assert_eq!(held.modifiers(), Modifiers::default());
}

#[test]
fn held_modifiers_map_each_key_to_its_modifier() {
    let cases = [
        (
            UIKeyboardHIDUsage::KeyboardLeftControl,
            Modifiers::control(),
        ),
        (
            UIKeyboardHIDUsage::KeyboardRightControl,
            Modifiers::control(),
        ),
        (UIKeyboardHIDUsage::KeyboardLeftAlt, Modifiers::alt()),
        (UIKeyboardHIDUsage::KeyboardRightAlt, Modifiers::alt()),
        (UIKeyboardHIDUsage::KeyboardLeftGUI, Modifiers::command()),
        (UIKeyboardHIDUsage::KeyboardRightGUI, Modifiers::command()),
        (UIKeyboardHIDUsage::KeyboardLeftShift, Modifiers::shift()),
        (UIKeyboardHIDUsage::KeyboardRightShift, Modifiers::shift()),
    ];
    for (usage, modifiers) in cases {
        let mut held = HeldModifiers::default();
        held.set(bit(usage), true);
        assert_eq!(held.modifiers(), modifiers, "{usage:?}");
    }
}

#[test]
fn held_modifiers_adopt_reported_flags() {
    let mut held = HeldModifiers::default();
    held.sync(Modifiers::command_shift());
    assert_eq!(held.modifiers(), Modifiers::command_shift());

    held.set(bit(UIKeyboardHIDUsage::KeyboardRightControl), true);
    held.sync(Modifiers::command());
    assert_eq!(
        held.modifiers(),
        Modifiers::command(),
        "control and shift released while unseen are cleared on both sides"
    );
}
