//! UIKit objects for the shortcuts bar (`crate::keyboard_bar`): one
//! `UIBarButtonItemGroup` per consumer group on the text input view's
//! `inputAssistantItem`. Each button's `UIAction` queues its id, and the
//! arrow key (`keyboard_accessory::arrow_key`, the bar item's custom view)
//! queues its arrows; nothing here calls GPUI, so a press is safe inside any
//! UIKit callback. The accessory row (`keyboard_accessory`) reuses the
//! actions and menus built here.
#![allow(
    unsafe_code,
    reason = "UIAction takes its handler as a raw block pointer, and the \
              accessibility setters are sent with msg_send"
)]

use std::{cell::RefCell, ptr::NonNull, rc::Rc};

use block2::RcBlock;
use objc2::{MainThreadMarker, MainThreadOnly as _, msg_send, rc::Retained};
use objc2_foundation::{NSArray, NSString};
use objc2_ui_kit::{
    UIAction, UIBarButtonItem, UIBarButtonItemGroup, UIImage, UIMenu, UIMenuElement, UIResponder,
};

use super::keyboard_accessory::arrow_key::HerdrArrowKey;
use crate::keyboard_bar::{KeyboardBar, KeyboardBarItem, KeyboardBarMenuEntry, PressQueue};

#[cfg(feature = "test-support")]
pub(crate) mod contract;

/// Replaces `responder`'s shortcuts bar groups with `bar`'s. UIKit applies
/// the change to a visible bar at once (`UIResponder.h`).
pub(super) fn apply(
    responder: &UIResponder,
    bar: &KeyboardBar,
    queue: &Rc<RefCell<PressQueue>>,
    mtm: MainThreadMarker,
) {
    let assistant = responder.inputAssistantItem();
    assistant.setLeadingBarButtonGroups(&groups(&bar.leading, queue, mtm));
    assistant.setTrailingBarButtonGroups(&groups(&bar.trailing, queue, mtm));
}

/// Whether `old` and `new` differ at most in their items' `selected`, so
/// `update_selected` can apply `new` to the buttons `old` built.
pub(super) fn same_shape(old: &KeyboardBar, new: &KeyboardBar) -> bool {
    let shape = |bar: &KeyboardBar| {
        let mut bar = bar.clone();
        for item in bar.leading.iter_mut().chain(&mut bar.trailing).flatten() {
            item.selected = false;
        }
        bar
    };
    shape(old) == shape(new)
}

/// Draws `bar`'s `selected` on the buttons `apply` put on `responder` for a
/// bar of the same shape (`same_shape`), leaving every item in place.
pub(super) fn update_selected(responder: &UIResponder, bar: &KeyboardBar) {
    let assistant = responder.inputAssistantItem();
    let update = |applied: &NSArray<UIBarButtonItemGroup>, groups: &[Vec<KeyboardBarItem>]| {
        let groups = groups.iter().filter(|group| !group.is_empty());
        for (applied, group) in applied.iter().zip(groups) {
            for (button, item) in applied.barButtonItems().iter().zip(group) {
                if item.arrows.is_none() && button.isSelected() != item.selected {
                    button.setSelected(item.selected);
                }
            }
        }
    };
    update(&assistant.leadingBarButtonGroups(), &bar.leading);
    update(&assistant.trailingBarButtonGroups(), &bar.trailing);
}

fn groups(
    groups: &[Vec<KeyboardBarItem>],
    queue: &Rc<RefCell<PressQueue>>,
    mtm: MainThreadMarker,
) -> Retained<NSArray<UIBarButtonItemGroup>> {
    let groups: Vec<_> = groups
        .iter()
        .filter(|group| !group.is_empty())
        .map(|group| {
            let buttons: Vec<_> = group.iter().map(|item| button(item, queue, mtm)).collect();
            UIBarButtonItemGroup::initWithBarButtonItems_representativeItem(
                UIBarButtonItemGroup::alloc(mtm),
                &NSArray::from_retained_slice(&buttons),
                None,
            )
        })
        .collect();
    NSArray::from_retained_slice(&groups)
}

fn button(
    item: &KeyboardBarItem,
    queue: &Rc<RefCell<PressQueue>>,
    mtm: MainThreadMarker,
) -> Retained<UIBarButtonItem> {
    if let Some(arrows) = item.arrows {
        let key = HerdrArrowKey::new(arrows, queue, mtm);
        set_accessibility(&*key, &item.identifier, Some(&item.accessibility_label));
        key.hold_bar_width();
        let button = UIBarButtonItem::initWithCustomView(UIBarButtonItem::alloc(mtm), &key);
        set_accessibility(&*button, &item.identifier, Some(&item.accessibility_label));
        return button;
    }
    let image = symbol(item.symbol.as_deref());
    let button = if item.menu.is_empty() {
        let action = action(&item.title, image.as_deref(), item.id, queue, mtm);
        UIBarButtonItem::initWithPrimaryAction(UIBarButtonItem::alloc(mtm), Some(&action))
    } else {
        let menu = menu(&item.menu, queue, mtm);
        match &image {
            Some(image) => UIBarButtonItem::initWithImage_menu(
                UIBarButtonItem::alloc(mtm),
                Some(image),
                Some(&menu),
            ),
            None => UIBarButtonItem::initWithTitle_menu(
                UIBarButtonItem::alloc(mtm),
                Some(&NSString::from_str(&item.title)),
                Some(&menu),
            ),
        }
    };
    button.setSelected(item.selected);
    set_accessibility(&*button, &item.identifier, Some(&item.accessibility_label));
    button
}

pub(super) fn menu(
    entries: &[KeyboardBarMenuEntry],
    queue: &Rc<RefCell<PressQueue>>,
    mtm: MainThreadMarker,
) -> Retained<UIMenu> {
    let children: Vec<Retained<UIMenuElement>> = entries
        .iter()
        .map(|entry| {
            let image = symbol(entry.symbol.as_deref());
            let action = action(&entry.title, image.as_deref(), entry.id, queue, mtm);
            set_accessibility(&*action, &entry.identifier, None);
            Retained::into_super(action)
        })
        .collect();
    UIMenu::menuWithChildren(&NSArray::from_retained_slice(&children), mtm)
}

pub(super) fn symbol(name: Option<&str>) -> Option<Retained<UIImage>> {
    UIImage::systemImageNamed(&NSString::from_str(name?))
}

/// A UIAction that queues `id`. The block holds the queue, not the window,
/// so a press after the window closed lands in a closed queue and is dropped.
pub(super) fn action(
    title: &str,
    image: Option<&UIImage>,
    id: u32,
    queue: &Rc<RefCell<PressQueue>>,
    mtm: MainThreadMarker,
) -> Retained<UIAction> {
    let queue = Rc::clone(queue);
    let handler = RcBlock::new(move |_: NonNull<UIAction>| {
        let _native_entry = crate::native_operation::raw("keyboard-bar-press");
        let Ok(mut queue) = queue.try_borrow_mut() else {
            tracing::warn!(
                id,
                "keyboard bar press while its queue is borrowed; dropped"
            );
            return;
        };
        let queued = queue.push(id);
        tracing::debug!(id, queued, dropped = queue.dropped(), "keyboard bar press");
    });
    // SAFETY: `handler` is a valid block for UIActionHandler's signature;
    // UIKit copies it, and `RcBlock` keeps our reference alive for the call.
    unsafe {
        UIAction::actionWithTitle_image_identifier_handler(
            &NSString::from_str(title),
            image,
            None,
            RcBlock::as_ptr(&handler),
            mtm,
        )
    }
}

pub(super) fn set_accessibility<T: objc2::Message>(
    object: &T,
    identifier: &str,
    label: Option<&str>,
) {
    let identifier = NSString::from_str(identifier);
    // SAFETY: UIBarButtonItem, UIAction and UIView adopt
    // UIAccessibilityIdentification (setAccessibilityIdentifier: takes an
    // NSString, copied).
    let _: () = unsafe { msg_send![object, setAccessibilityIdentifier: &*identifier] };
    if let Some(label) = label {
        let label = NSString::from_str(label);
        // SAFETY: NSObject's UIAccessibility category; the label is copied.
        let _: () = unsafe { msg_send![object, setAccessibilityLabel: &*label] };
    }
}
