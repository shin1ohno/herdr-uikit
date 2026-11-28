//! Simulator contract for the shortcuts bar's and the accessory row's UIKit
//! objects (run by `tests/keyboard_bar_uikit.rs` on the main thread, outside
//! UIApplicationMain): what `apply` puts on a responder's
//! `inputAssistantItem`, the arrow key as a bar item's custom view, the
//! in-place selected update, and the row's keys and their layout. Whether
//! UIKit draws them in each keyboard state, and the arrow key's touches, are
//! the consumer's Simulator E2E (yebisu#229, #276) and device checks.
#![allow(unsafe_code, reason = "reads accessibility identifiers with msg_send")]

use std::{cell::RefCell, rc::Rc};

use objc2::{MainThreadMarker, msg_send, rc::Retained};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::NSString;
use objc2_ui_kit::{UIBarButtonItemGroup, UIButton, UIView};

use crate::keyboard_bar::{
    KeyboardAccessory, KeyboardArrows, KeyboardBar, KeyboardBarItem, KeyboardBarMenuEntry,
    PressQueue,
};
use crate::window::keyboard_accessory::{HerdrKeyboardRow, arrow_key::HerdrArrowKey};

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
        arrows: None,
    }
}

const ARROWS: KeyboardArrows = KeyboardArrows {
    up: 11,
    down: 12,
    left: 13,
    right: 14,
};

fn arrow_item() -> KeyboardBarItem {
    KeyboardBarItem {
        arrows: Some(ARROWS),
        ..item(0, "arrows", false)
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
    super::apply(&view, &bar, &queue, mtm);

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
    assert_eq!(identifiers(&trailing), [vec!["bar-more"]]);
    let more = trailing
        .iter()
        .next()
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

    // An empty bar removes every consumer group.
    super::apply(&view, &KeyboardBar::default(), &queue, mtm);
    assert_eq!(
        view.inputAssistantItem().leadingBarButtonGroups().count(),
        0
    );
    assert_eq!(
        view.inputAssistantItem().trailingBarButtonGroups().count(),
        0
    );
    arrow_key_on_the_bar(mtm, &queue);
    selected_updates_in_place(mtm, &queue);
    accessory_row(mtm, &queue);
    println!("keyboard bar contract: ok");
}

/// An arrows item is a custom view item: the arrow key, with the item's
/// identifier and ids.
fn arrow_key_on_the_bar(mtm: MainThreadMarker, queue: &Rc<RefCell<PressQueue>>) {
    let view = UIView::new(mtm);
    let bar = KeyboardBar {
        leading: Vec::new(),
        trailing: vec![vec![arrow_item(), item(5, "cmd", false)]],
    };
    super::apply(&view, &bar, queue, mtm);
    let trailing = view.inputAssistantItem().trailingBarButtonGroups();
    assert_eq!(identifiers(&trailing), [vec!["bar-arrows", "bar-cmd"]]);
    let custom = trailing
        .iter()
        .next()
        .and_then(|group| group.barButtonItems().iter().next())
        .and_then(|item| item.customView());
    let Some(custom) = custom else {
        panic!("the arrows item has no custom view");
    };
    let Ok(key) = custom.downcast::<HerdrArrowKey>() else {
        panic!("the arrows item's custom view is not the arrow key");
    };
    assert_eq!(key.arrows(), ARROWS);
    assert_eq!(identifier(&*key), "bar-arrows");
}

/// A bar that only changes `selected` keeps its item objects, so a held
/// arrow key is not replaced mid-touch.
fn selected_updates_in_place(mtm: MainThreadMarker, queue: &Rc<RefCell<PressQueue>>) {
    let view = UIView::new(mtm);
    let off = KeyboardBar {
        leading: vec![vec![item(0, "ctrl", false)]],
        trailing: vec![vec![arrow_item()]],
    };
    let mut on = off.clone();
    on.leading[0][0].selected = true;
    assert!(super::same_shape(&off, &on));
    let mut other = off.clone();
    other.leading[0][0].title = "control".into();
    assert!(!super::same_shape(&off, &other));
    super::apply(&view, &off, queue, mtm);
    let first = |view: &UIView| {
        view.inputAssistantItem()
            .leadingBarButtonGroups()
            .iter()
            .next()
            .and_then(|group| group.barButtonItems().iter().next())
    };
    let Some(before) = first(&view) else {
        panic!("no ctrl item");
    };
    assert!(!before.isSelected());
    super::update_selected(&view, &on);
    let Some(after) = first(&view) else {
        panic!("no ctrl item after the update");
    };
    assert!(std::ptr::eq(&*before, &*after), "the ctrl item is kept");
    assert!(after.isSelected(), "the kept item is drawn selected");
}

/// The row: one equal-width key per item inside the row, the selected key,
/// the menu key and the arrow key; a same-shape row updates in place.
fn accessory_row(mtm: MainThreadMarker, queue: &Rc<RefCell<PressQueue>>) {
    let mut more = item(99, "more", false);
    more.menu = vec![KeyboardBarMenuEntry {
        id: 7,
        title: "Paste".into(),
        symbol: None,
        identifier: "bar-paste".into(),
    }];
    let row = KeyboardAccessory {
        items: vec![
            item(0, "ctrl", true),
            item(1, "esc", false),
            arrow_item(),
            more,
        ],
    };
    let view = HerdrKeyboardRow::new(mtm);
    view.setFrame(CGRect {
        origin: CGPoint::ZERO,
        size: CGSize {
            width: 756.0,
            height: super::super::keyboard_accessory::ROW_HEIGHT,
        },
    });
    view.update(&row, queue);
    view.layoutIfNeeded();
    assert_eq!(
        identifier(&*view),
        super::super::keyboard_accessory::ROW_IDENTIFIER
    );
    let keys = view.subviews();
    let ids: Vec<String> = keys.iter().map(|key| identifier(&*key)).collect();
    assert_eq!(ids, ["bar-ctrl", "bar-esc", "bar-arrows", "bar-more"]);
    let frames: Vec<CGRect> = keys.iter().map(|key| key.frame()).collect();
    for (index, frame) in frames.iter().enumerate() {
        assert!(frame.origin.x >= 0.0, "key {index} starts inside the row");
        assert!(
            frame.origin.x + frame.size.width <= 756.0,
            "key {index} ends inside the row"
        );
        assert!(
            (frame.size.width - frames[0].size.width).abs() < 0.01,
            "key {index} is as wide as the first"
        );
        if let Some(next) = frames.get(index + 1) {
            assert!(
                frame.origin.x + frame.size.width < next.origin.x,
                "key {index} does not overlap the next"
            );
        }
    }
    let button = |index: usize| -> Retained<UIButton> {
        let Some(view) = keys.iter().nth(index) else {
            panic!("no key {index}");
        };
        let Ok(button) = view.downcast::<UIButton>() else {
            panic!("key {index} is not a button");
        };
        button
    };
    let ctrl = button(0);
    assert!(ctrl.isSelected(), "a latched key is drawn selected");
    assert!(!button(1).isSelected());
    assert!(button(3).menu().is_some(), "the more key has a menu");
    assert!(button(3).showsMenuAsPrimaryAction());
    let Some(arrow) = keys.iter().nth(2) else {
        panic!("no arrow key");
    };
    assert!(arrow.downcast::<HerdrArrowKey>().is_ok());

    let mut unlatched = row.clone();
    unlatched.items[0].selected = false;
    view.update(&unlatched, queue);
    let kept = view.subviews();
    assert_eq!(kept.count(), 4);
    let Some(first) = kept.iter().next() else {
        panic!("no first key after the update");
    };
    assert!(
        std::ptr::eq(
            Retained::as_ptr(&first).cast::<u8>(),
            Retained::as_ptr(&ctrl).cast::<u8>()
        ),
        "the ctrl key is kept"
    );
    assert!(!ctrl.isSelected(), "the kept key is no longer selected");

    let mut shorter = unlatched.clone();
    shorter.items.truncate(2);
    view.update(&shorter, queue);
    assert_eq!(
        view.subviews().count(),
        2,
        "another shape builds the keys anew"
    );
}
