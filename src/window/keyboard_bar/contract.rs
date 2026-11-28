//! Simulator contract for the shortcuts bar's UIKit objects (run by
//! `tests/keyboard_bar_uikit.rs` on the main thread, outside
//! UIApplicationMain): what `apply` puts on a responder's
//! `inputAssistantItem`. Whether UIKit draws it in each keyboard state is
//! the consumer's Simulator E2E (yebisu#229) and a device check.
#![allow(unsafe_code, reason = "reads accessibility identifiers with msg_send")]

use std::{cell::RefCell, rc::Rc};

use objc2::{MainThreadMarker, msg_send, rc::Retained};
use objc2_foundation::NSString;
use objc2_ui_kit::{UIBarButtonItemGroup, UIView};

use crate::keyboard_bar::{KeyboardBar, KeyboardBarItem, KeyboardBarMenuEntry, PressQueue};

fn identifier<T: objc2::Message>(object: &T) -> String {
    // SAFETY: UIBarButtonItem and UIMenuElement adopt
    // UIAccessibilityIdentification; the getter returns a nullable NSString.
    let id: Option<Retained<NSString>> = unsafe { msg_send![object, accessibilityIdentifier] };
    id.map(|id| id.to_string()).unwrap_or_default()
}

fn item(id: u32, name: &str, selected: bool) -> KeyboardBarItem {
    KeyboardBarItem {
        id,
        title: name.into(),
        symbol: None,
        accessibility_label: name.into(),
        identifier: format!("bar-{name}"),
        selected,
        menu: Vec::new(),
    }
}

fn identifiers(groups: &objc2_foundation::NSArray<UIBarButtonItemGroup>) -> Vec<Vec<String>> {
    groups
        .iter()
        .map(|group| {
            group
                .barButtonItems()
                .iter()
                .map(|item| identifier(&*item))
                .collect()
        })
        .collect()
}

pub(crate) fn run() {
    let Some(mtm) = MainThreadMarker::new() else {
        panic!("the keyboard bar contract runs on the main thread");
    };
    let view = UIView::new(mtm);
    let queue = Rc::new(RefCell::new(PressQueue::default()));
    let mut more = item(99, "more", false);
    more.symbol = Some("ellipsis.circle".into());
    more.menu = vec![KeyboardBarMenuEntry {
        id: 7,
        title: "Paste".into(),
        symbol: Some("doc.on.clipboard".into()),
        identifier: "bar-paste".into(),
    }];
    let bar = KeyboardBar {
        leading: vec![
            vec![item(0, "ctrl", true), item(1, "esc", false)],
            Vec::new(),
        ],
        trailing: vec![vec![more]],
    };
    let Some(sentinel) = super::apply(&view, &bar, &queue, true, mtm) else {
        panic!("a consumer's bar carries the sentinel");
    };
    assert!(
        !sentinel.isUserInteractionEnabled(),
        "the sentinel takes no touches"
    );
    // SAFETY: UIView's UIAccessibility category getters return BOOL.
    let (element, hidden): (bool, bool) = unsafe {
        (
            msg_send![&*sentinel, isAccessibilityElement],
            msg_send![&*sentinel, accessibilityElementsHidden],
        )
    };
    assert!(
        !element && hidden,
        "the sentinel is hidden from accessibility"
    );

    let assistant = view.inputAssistantItem();
    let leading = assistant.leadingBarButtonGroups();
    // An empty group is left out.
    assert_eq!(identifiers(&leading), [vec!["bar-ctrl", "bar-esc"]]);
    let ctrl = leading
        .iter()
        .next()
        .and_then(|group| group.barButtonItems().iter().next());
    let Some(ctrl) = ctrl else {
        panic!("no ctrl item");
    };
    assert!(ctrl.isSelected(), "a latched key is drawn selected");
    assert!(ctrl.primaryAction().is_some(), "a plain key has an action");
    assert!(ctrl.menu().is_none());

    let trailing = assistant.trailingBarButtonGroups();
    // The sentinel leads the trailing groups, so it moves none of them.
    assert_eq!(
        identifiers(&trailing),
        [vec![super::SENTINEL_IDENTIFIER], vec!["bar-more"]]
    );
    let custom = trailing
        .iter()
        .next()
        .and_then(|group| group.barButtonItems().iter().next())
        .and_then(|item| item.customView());
    assert!(
        custom.is_some_and(|custom| std::ptr::eq(&*custom, &*sentinel)),
        "the sentinel item shows the returned view"
    );
    let more = trailing
        .iter()
        .nth(1)
        .and_then(|group| group.barButtonItems().iter().next());
    let Some(more) = more else {
        panic!("no more item");
    };
    let Some(menu) = more.menu() else {
        panic!("the more item opens a menu");
    };
    let entries: Vec<String> = menu
        .children()
        .iter()
        .map(|entry| identifier(&*entry))
        .collect();
    assert_eq!(entries, ["bar-paste"]);

    // An empty bar removes every consumer group and keeps the sentinel, so a
    // split keyboard is still seen while the consumer shows no keys.
    let kept = super::apply(&view, &KeyboardBar::default(), &queue, true, mtm);
    assert!(kept.is_some(), "an emptied bar keeps the sentinel");
    assert_eq!(
        view.inputAssistantItem().leadingBarButtonGroups().count(),
        0
    );
    assert_eq!(
        identifiers(&view.inputAssistantItem().trailingBarButtonGroups()),
        [vec![super::SENTINEL_IDENTIFIER]]
    );

    // Before a consumer set a bar (a replacement view), no group at all.
    let none = super::apply(&view, &KeyboardBar::default(), &queue, false, mtm);
    assert!(none.is_none(), "no sentinel before a consumer set a bar");
    assert_eq!(
        view.inputAssistantItem().leadingBarButtonGroups().count(),
        0
    );
    assert_eq!(
        view.inputAssistantItem().trailingBarButtonGroups().count(),
        0
    );
    println!("keyboard bar contract: ok");
}
