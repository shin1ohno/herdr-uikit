//! The accessory row (`crate::keyboard_bar::KeyboardAccessory`): a
//! keyboard-style `UIInputView` that is the text input view's
//! `inputAccessoryView`, with one equal-width key per item. A plain item is a
//! gray `UIButton` (filled while `selected`) whose `UIAction` queues its id,
//! a `menu` item opens its menu on tap, and an `arrows` item is the flick
//! arrow key (`arrow_key`). Nothing here calls GPUI, so a press is safe
//! inside any UIKit callback.
//!
//! UIKit places the row: above a docked or split keyboard, and full width at
//! the screen's bottom edge below a floating one. The row stays one object
//! for the window's life; a new row of the same shape updates the keys in
//! place (`update`), so a touch on one is not cut off.
#![allow(
    unsafe_code,
    reason = "an Objective-C UIInputView subclass, and its keys' accessibility setters"
)]

pub(in crate::window) mod arrow_key;

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_foundation::NSString;
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UIButton, UIButtonConfiguration, UIButtonConfigurationCornerStyle,
    UIButtonConfigurationSize, UICoordinateSpace, UIInputView, UIInputViewStyle, UIResponder,
    UIView, UIViewAutoresizing,
};

use self::arrow_key::HerdrArrowKey;
use super::keyboard_bar::{action, menu, set_accessibility, symbol};
use crate::keyboard_bar::{KeyboardAccessory, KeyboardBarItem, PressQueue};

/// The row's height in points: 40 pt keys with 4 pt above and below.
pub(crate) const ROW_HEIGHT: CGFloat = 48.0;
/// The space between keys, and from the row's ends to the first and last.
const GAP: CGFloat = 6.0;
const KEY_MARGIN: CGFloat = 4.0;
/// The row's accessibility identifier (for UI tests).
pub(crate) const ROW_IDENTIFIER: &str = "herdr-keyboard-accessory";

/// One key of the row.
enum Key {
    Button(Retained<UIButton>),
    Arrow(Retained<HerdrArrowKey>),
}

impl Key {
    fn view(&self) -> &UIView {
        match self {
            Self::Button(button) => button,
            Self::Arrow(arrow) => arrow,
        }
    }
}

pub(in crate::window) struct Ivars {
    keys: RefCell<Vec<Key>>,
    /// The row the keys were last built or updated from.
    row: RefCell<KeyboardAccessory>,
    /// The keys' width at the last layout, for QA.
    key_width: Cell<CGFloat>,
}

define_class!(
    // SAFETY: UIInputView's subclassing rules are kept: instances are created
    // through `initWithFrame:inputViewStyle:` (`HerdrKeyboardRow::new`), the
    // class does not implement `Drop`, and every override keeps the
    // superclass signature.
    #[unsafe(super(UIInputView, UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    pub(in crate::window) struct HerdrKeyboardRow;

    impl HerdrKeyboardRow {
        #[unsafe(method(layoutSubviews))]
        fn __layout_subviews(&self) {
            // SAFETY: `layoutSubviews` takes and returns nothing.
            let () = unsafe { msg_send![super(self), layoutSubviews] };
            self.lay_out_keys();
        }
    }
);

impl HerdrKeyboardRow {
    pub(in crate::window) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            keys: RefCell::default(),
            row: RefCell::default(),
            key_width: Cell::new(0.0),
        });
        let frame = CGRect {
            origin: CGPoint::ZERO,
            size: CGSize {
                width: 0.0,
                height: ROW_HEIGHT,
            },
        };
        // SAFETY: `initWithFrame:inputViewStyle:` is UIInputView's designated
        // initializer, and the ivars were set before it runs.
        let this: Retained<Self> = unsafe {
            msg_send![super(this), initWithFrame: frame, inputViewStyle: UIInputViewStyle::Keyboard]
        };
        // UIKit sets the width; the height stays the frame's.
        this.setAutoresizingMask(UIViewAutoresizing::FlexibleWidth);
        this.setAllowsSelfSizing(false);
        set_accessibility(&*this, ROW_IDENTIFIER, None);
        this
    }

    /// Shows `row`'s keys. A row of the same shape (`same_shape`) updates
    /// the keys in place; any other is built anew.
    pub(in crate::window) fn update(
        &self,
        row: &KeyboardAccessory,
        queue: &Rc<RefCell<PressQueue>>,
    ) {
        let ivars = self.ivars();
        if same_shape(&ivars.row.borrow(), row) && ivars.keys.borrow().len() == row.items.len() {
            for (key, item) in ivars.keys.borrow().iter().zip(&row.items) {
                if let Key::Button(button) = key
                    && button.isSelected() != item.selected
                {
                    button.setConfiguration(Some(&configuration(item, self.mtm())));
                    button.setSelected(item.selected);
                }
            }
        } else {
            for key in ivars.keys.borrow_mut().drain(..) {
                key.view().removeFromSuperview();
            }
            let keys: Vec<Key> = row
                .items
                .iter()
                .map(|item| key(item, queue, self.mtm()))
                .collect();
            for key in &keys {
                self.addSubview(key.view());
            }
            *ivars.keys.borrow_mut() = keys;
            self.setNeedsLayout();
        }
        *ivars.row.borrow_mut() = row.clone();
    }

    /// Equal-width keys across the row, inside the safe area's sides.
    fn lay_out_keys(&self) {
        let keys = self.ivars().keys.borrow();
        let count = keys.len();
        if count == 0 {
            return;
        }
        let bounds = self.bounds();
        let safe = self.safeAreaInsets();
        let left = bounds.origin.x + safe.left + GAP;
        let usable = bounds.size.width - safe.left - safe.right - GAP * 2.0;
        let gaps = GAP * (count - 1) as CGFloat;
        let width = ((usable - gaps) / count as CGFloat).max(0.0);
        let height = (bounds.size.height - KEY_MARGIN * 2.0).max(0.0);
        self.ivars().key_width.set(width);
        for (index, key) in keys.iter().enumerate() {
            let x = left + (width + GAP) * index as CGFloat;
            key.view().setFrame(CGRect {
                origin: CGPoint {
                    x,
                    y: bounds.origin.y + KEY_MARGIN,
                },
                size: CGSize { width, height },
            });
        }
    }

    /// Simulator QA: the key count and their width at the last layout.
    #[cfg(target_abi = "sim")]
    pub(in crate::window) fn qa_keys(&self) -> (usize, CGFloat) {
        (
            self.ivars().keys.borrow().len(),
            self.ivars().key_width.get(),
        )
    }
}

/// Whether `old` and `new` differ at most in their items' `selected`.
fn same_shape(old: &KeyboardAccessory, new: &KeyboardAccessory) -> bool {
    old.items.len() == new.items.len()
        && old.items.iter().zip(&new.items).all(|(old, new)| {
            KeyboardBarItem {
                selected: false,
                ..old.clone()
            } == KeyboardBarItem {
                selected: false,
                ..new.clone()
            }
        })
}

fn key(item: &KeyboardBarItem, queue: &Rc<RefCell<PressQueue>>, mtm: MainThreadMarker) -> Key {
    if let Some(arrows) = item.arrows {
        let arrow = HerdrArrowKey::new(arrows, queue, mtm);
        set_accessibility(&*arrow, &item.identifier, Some(&item.accessibility_label));
        return Key::Arrow(arrow);
    }
    let config = configuration(item, mtm);
    let button = if item.menu.is_empty() {
        let image = symbol(item.symbol.as_deref());
        let action = action(&item.title, image.as_deref(), item.id, queue, mtm);
        let button = UIButton::buttonWithConfiguration_primaryAction(&config, Some(&action));
        // The action's title and image replace the configuration's; set
        // them again so a symbol key shows only its symbol.
        button.setConfiguration(Some(&config));
        button
    } else {
        let button = UIButton::buttonWithConfiguration_primaryAction(&config, None);
        button.setMenu(Some(&menu(&item.menu, queue, mtm)));
        button.setShowsMenuAsPrimaryAction(true);
        button
    };
    button.setSelected(item.selected);
    set_accessibility(&*button, &item.identifier, Some(&item.accessibility_label));
    Key::Button(button)
}

/// A key's look: gray, or filled while latched; its SF Symbol if UIKit has
/// it, else its title.
fn configuration(item: &KeyboardBarItem, mtm: MainThreadMarker) -> Retained<UIButtonConfiguration> {
    let config = if item.selected {
        UIButtonConfiguration::filledButtonConfiguration(mtm)
    } else {
        UIButtonConfiguration::grayButtonConfiguration(mtm)
    };
    match symbol(item.symbol.as_deref()) {
        Some(image) => config.setImage(Some(&image)),
        None => config.setTitle(Some(&NSString::from_str(&item.title))),
    }
    key_style(&config);
    config
}

/// What every key of the row shares: small text, little padding (a 44 pt
/// key fits "ctrl"), and the keyboard's rounded corners.
fn key_style(config: &UIButtonConfiguration) {
    config.setButtonSize(UIButtonConfigurationSize::Small);
    config.setCornerStyle(UIButtonConfigurationCornerStyle::Medium);
    config.setContentInsets(NSDirectionalEdgeInsets {
        top: 2.0,
        leading: 2.0,
        bottom: 2.0,
        trailing: 2.0,
    });
}

/// Simulator QA: the row's key count, key width and frame in screen points,
/// and whether it is in a window.
#[cfg(target_abi = "sim")]
pub(in crate::window) fn qa_sample(row: &HerdrKeyboardRow) -> String {
    let (keys, width) = row.qa_keys();
    let frame = row.window().map(|window| {
        let bounds = row.bounds();
        let screen = window.screen();
        let rect = row.convertRect_toCoordinateSpace(bounds, &screen.coordinateSpace());
        (
            rect.origin.x,
            rect.origin.y,
            rect.size.width,
            rect.size.height,
        )
    });
    format!("keys={keys} key_width={width:.1} frame={frame:?}")
}

/// The row's frame in its screen's points while it is in a window.
pub(in crate::window) fn screen_frame(row: &HerdrKeyboardRow) -> Option<(f64, f64, f64, f64)> {
    let window = row.window()?;
    if row.isHidden() {
        return None;
    }
    let screen = window.screen();
    let rect = row.convertRect_toCoordinateSpace(row.bounds(), &screen.coordinateSpace());
    Some((
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
    ))
}
