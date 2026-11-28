//! UIKit objects for the shortcuts bar (`crate::keyboard_bar`): one
//! `UIBarButtonItemGroup` per consumer group on the text input view's
//! `inputAssistantItem`. Each button's `UIAction` queues its id; nothing here
//! calls GPUI, so a press is safe inside any UIKit callback.
#![allow(
    unsafe_code,
    reason = "UIAction takes its handler as a raw block pointer, and the \
              accessibility setters are sent with msg_send"
)]

use std::{cell::RefCell, ptr::NonNull, rc::Rc};

use block2::RcBlock;
use objc2::{MainThreadMarker, MainThreadOnly as _, msg_send, rc::Retained};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSArray, NSString};
use objc2_ui_kit::{
    UIAction, UIBarButtonItem, UIBarButtonItemGroup, UIImage, UIMenu, UIMenuElement, UIResponder,
    UIView,
};

use crate::keyboard_bar::{KeyboardBar, KeyboardBarItem, KeyboardBarMenuEntry, PressQueue};

#[cfg(feature = "test-support")]
pub(crate) mod contract;

/// The sentinel item's accessibility identifier (tests).
pub(crate) const SENTINEL_IDENTIFIER: &str = "herdr-keyboard-bar-sentinel";

/// Replaces `responder`'s shortcuts bar groups with `bar`'s. With
/// `with_sentinel` (a consumer has set a bar), the trailing groups start
/// with the sentinel: an item whose transparent custom view shows where UIKit
/// drew the trailing groups, which tells a split bar from a whole one
/// (`keyboard_placement::bar_split`). UIKit packs trailing groups against the
/// bar's right end, so a sentinel in front of them moves none of the
/// consumer's buttons while the trailing half has room (iPad mini (A20 Pro)
/// portrait: 98 points); at the end it would push every one of them 24
/// points left (yebisu#276). An empty bar keeps only the sentinel, so a
/// consumer that empties its bar while the keyboard is split still learns
/// when it joins again. UIKit applies the change to a visible bar at once
/// (`UIResponder.h`). Returns the sentinel's view.
pub(super) fn apply(
    responder: &UIResponder,
    bar: &KeyboardBar,
    queue: &Rc<RefCell<PressQueue>>,
    with_sentinel: bool,
    mtm: MainThreadMarker,
) -> Option<Retained<UIView>> {
    let assistant = responder.inputAssistantItem();
    let leading = groups(&bar.leading, queue, mtm);
    assistant.setLeadingBarButtonGroups(&NSArray::from_retained_slice(&leading));
    let mut trailing = groups(&bar.trailing, queue, mtm);
    let view = with_sentinel.then(|| {
        let (group, view) = sentinel(mtm);
        trailing.insert(0, group);
        view
    });
    assistant.setTrailingBarButtonGroups(&NSArray::from_retained_slice(&trailing));
    view
}

fn groups(
    groups: &[Vec<KeyboardBarItem>],
    queue: &Rc<RefCell<PressQueue>>,
    mtm: MainThreadMarker,
) -> Vec<Retained<UIBarButtonItemGroup>> {
    groups
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
        .collect()
}

/// The sentinel group and its view: one item with a 1-point transparent
/// custom view that takes no touches and is hidden from accessibility.
fn sentinel(mtm: MainThreadMarker) -> (Retained<UIBarButtonItemGroup>, Retained<UIView>) {
    let frame = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(1.0, 1.0));
    let view = UIView::initWithFrame(UIView::alloc(mtm), frame);
    let item = UIBarButtonItem::initWithCustomView(UIBarButtonItem::alloc(mtm), &view);
    // After the item takes the view: `initWithCustomView:` turns user
    // interaction back on (the Simulator contract caught it).
    view.setUserInteractionEnabled(false);
    // SAFETY: UIView's UIAccessibility category; both setters take a BOOL.
    unsafe {
        let _: () = msg_send![&*view, setIsAccessibilityElement: false];
        let _: () = msg_send![&*view, setAccessibilityElementsHidden: true];
    }
    set_accessibility(&*item, SENTINEL_IDENTIFIER, None);
    let group = UIBarButtonItemGroup::initWithBarButtonItems_representativeItem(
        UIBarButtonItemGroup::alloc(mtm),
        &NSArray::from_retained_slice(&[item]),
        None,
    );
    (group, view)
}

fn button(
    item: &KeyboardBarItem,
    queue: &Rc<RefCell<PressQueue>>,
    mtm: MainThreadMarker,
) -> Retained<UIBarButtonItem> {
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

fn menu(
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

fn symbol(name: Option<&str>) -> Option<Retained<UIImage>> {
    UIImage::systemImageNamed(&NSString::from_str(name?))
}

/// A UIAction that queues `id`. The block holds the queue, not the window,
/// so a press after the window closed lands in a closed queue and is dropped.
fn action(
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

fn set_accessibility<T: objc2::Message>(object: &T, identifier: &str, label: Option<&str>) {
    let identifier = NSString::from_str(identifier);
    // SAFETY: UIBarButtonItem and UIAction adopt UIAccessibilityIdentification
    // (setAccessibilityIdentifier: takes an NSString, copied).
    let _: () = unsafe { msg_send![object, setAccessibilityIdentifier: &*identifier] };
    if let Some(label) = label {
        let label = NSString::from_str(label);
        // SAFETY: NSObject's UIAccessibility category; the label is copied.
        let _: () = unsafe { msg_send![object, setAccessibilityLabel: &*label] };
    }
}
