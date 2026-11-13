//! Real delegate/command objects and production dispatch on the process main thread.
#![allow(
    unsafe_code,
    reason = "main-thread Simulator contract uses typed UIKit selectors"
)]
use super::{
    delegate::{EditMenuDelegate, build_menu, classify},
    state::*,
};
use objc2::{
    ClassType, MainThreadMarker, MainThreadOnly, ProtocolType,
    rc::Retained,
    runtime::{AnyObject, ProtocolObject},
    sel,
};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};
use objc2_ui_kit::{
    UICommand, UIEditMenuConfiguration, UIEditMenuInteraction, UIEditMenuInteractionDelegate,
    UIMenu, UIMenuElementAttributes,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn token(state: &RefCell<EditMenuState>) -> NativeEditMenuToken {
    state
        .borrow_mut()
        .present()
        .unwrap_or_else(|| panic!("available menu"))
}
fn command(properties: Option<&AnyObject>, mtm: MainThreadMarker) -> Retained<UICommand> {
    // SAFETY: standard selector, main thread, optional property list.
    unsafe {
        UICommand::commandWithTitle_image_action_propertyList(
            &NSString::from_str("Paste"),
            None,
            sel!(paste:),
            properties,
            mtm,
        )
    }
}
fn configuration(id: &str, mtm: MainThreadMarker) -> Retained<UIEditMenuConfiguration> {
    // SAFETY: NSString is NSCopying; source point is in the view's coordinates.
    unsafe {
        UIEditMenuConfiguration::configurationWithIdentifier_sourcePoint(
            Some(ProtocolObject::from_ref(&*NSString::from_str(id))),
            objc2_core_foundation::CGPoint::ZERO,
            mtm,
        )
    }
}

fn execute(
    state: &EditMenuState,
    sender: Option<&AnyObject>,
    selector: objc2::runtime::Sel,
    handled: bool,
) -> (usize, usize, Vec<Option<NativeEditMenuToken>>) {
    let count = Cell::new(0);
    let fallback = Cell::new(0);
    let mut origins = Vec::new();
    super::super::text_input::actions::dispatch_checked_selector(
        selector,
        state.delivery(classify(sender)),
        true,
        Default::default(),
        |event| {
            if matches!(event, gpui::PlatformInput::KeyDown(_)) {
                count.set(count.get() + 1);
            }
            if matches!(
                event,
                gpui::PlatformInput::KeyDown(_) | gpui::PlatformInput::KeyUp(_)
            ) {
                origins.push(crate::native_edit_menu_token());
            }
            handled
        },
        || {
            assert!(crate::native_edit_menu_token().is_none());
            fallback.set(fallback.get() + 1);
        },
    );
    assert!(crate::native_edit_menu_token().is_none());
    (count.get(), fallback.get(), origins)
}
fn execute_menu(state: &EditMenuState, menu: &UIMenu) -> (usize, usize, usize) {
    let (mut deliveries, mut fallbacks, mut tagged) = (0, 0, 0);
    for item in menu.children() {
        let Some(command) = item.downcast_ref::<UICommand>() else {
            panic!("only UICommand is selected by P1");
        };
        // SAFETY: standard command created by the delegate.
        let (sent, fallback, origins) =
            execute(state, Some(command), unsafe { command.action() }, true);
        deliveries += sent;
        fallbacks += fallback;
        tagged += usize::from(origins.first().is_some_and(Option::is_some));
    }
    (deliveries, fallbacks, tagged)
}

pub(crate) fn run() {
    let mtm = MainThreadMarker::new()
        .unwrap_or_else(|| panic!("UIKit contracts must run on main thread"));
    let protocol = <dyn UIEditMenuInteractionDelegate>::protocol()
        .unwrap_or_else(|| panic!("UIKit edit menu protocol"));
    assert!(EditMenuDelegate::class().conforms_to(protocol));
    let state = Rc::new(RefCell::new(EditMenuState::default()));
    state.borrow_mut().availability(true);
    let t1 = token(&state);
    let delegate = EditMenuDelegate::fixture(state.clone(), mtm);
    let interaction = UIEditMenuInteraction::initWithDelegate(
        UIEditMenuInteraction::alloc(mtm),
        Some(ProtocolObject::from_ref(&*delegate)),
    );
    let ordinary = command(None, mtm);
    // SAFETY: Copy is a standard editing selector.
    let copy = unsafe {
        UICommand::commandWithTitle_image_action_propertyList(
            &NSString::from_str("Copy"),
            None,
            sel!(copy:),
            None,
            mtm,
        )
    };
    let suggestions = NSArray::from_retained_slice(&[
        Retained::into_super(ordinary.clone()),
        Retained::into_super(copy),
    ]);
    let get_menu = |id: String| {
        ProtocolObject::<dyn UIEditMenuInteractionDelegate>::from_ref(&*delegate)
            .editMenuInteraction_menuForConfiguration_suggestedActions(
                &interaction,
                &configuration(&id, mtm),
                &suggestions,
            )
            .unwrap_or_else(|| panic!("owned menu must not be nil"))
    };
    let old = get_menu(format!("{EDIT_MENU_IDENTIFIER_PREFIX}{}", t1.value()));
    assert_eq!(old.children().count(), 2);
    assert_eq!(execute_menu(&state.borrow(), &old), (2, 0, 2));
    let t2 = token(&state);
    let stale = get_menu(format!("{EDIT_MENU_IDENTIFIER_PREFIX}{}", t1.value()));
    assert_eq!(stale.children().count(), 0, "P4 empty branch");
    assert_eq!(execute_menu(&state.borrow(), &stale), (0, 0, 0));
    assert_eq!(
        execute_menu(&state.borrow(), &old),
        (0, 0, 0),
        "retained stale items must not fall back"
    );
    let current = get_menu(format!("{EDIT_MENU_IDENTIFIER_PREFIX}{}", t2.value()));
    assert_eq!(current.children().count(), 2);
    assert_eq!(execute_menu(&state.borrow(), &current), (2, 0, 2));
    let foreign = get_menu("other-owner".into());
    assert_eq!(foreign.children().count(), 2);
    assert_eq!(execute_menu(&state.borrow(), &foreign), (2, 0, 0));
    assert_eq!(
        execute(&state.borrow(), Some(&ordinary), sel!(paste:), false),
        (1, 1, vec![None, None])
    );
    assert_eq!(
        execute(&state.borrow(), None, sel!(paste:), false),
        (1, 1, vec![None, None])
    );
    assert_eq!(
        execute(
            &state.borrow(),
            Some(&*NSString::from_str("other sender")),
            sel!(copy:),
            true
        )
        .0,
        1
    );
    {
        let malformed = NSString::from_str("42");
        let properties = NSDictionary::from_slices(
            &[&*NSString::from_str(EDIT_MENU_PROPERTY_KEY)],
            &[&*malformed as &AnyObject],
        );
        let cmd = command(Some(&properties), mtm);
        assert_eq!(classify(Some(&cmd)), Sender::Tagged(None));
        assert_eq!(
            execute(&state.borrow(), Some(&cmd), sel!(paste:), false),
            (0, 0, vec![])
        );
    }
    let other_property = NSDictionary::from_slices(
        &[&*NSString::from_str("other")],
        &[&*NSNumber::new_u64(t2.value()) as &AnyObject],
    );
    assert_eq!(
        classify(Some(&command(Some(&other_property), mtm))),
        Sender::Command
    );
    for reason in [
        RevokeReason::Inactive,
        RevokeReason::ResignedOutside,
        RevokeReason::SwitchLeftNoResponder,
        RevokeReason::Closed,
    ] {
        let t = token(&state);
        let retained = get_menu(format!("{EDIT_MENU_IDENTIFIER_PREFIX}{}", t.value()));
        state.borrow_mut().revoke(reason);
        assert_eq!(execute_menu(&state.borrow(), &retained), (0, 0, 0));
    }
    let t = token(&state);
    let retained = get_menu(format!("{EDIT_MENU_IDENTIFIER_PREFIX}{}", t.value()));
    state.borrow_mut().availability(false);
    assert_eq!(execute_menu(&state.borrow(), &retained), (0, 0, 0));
    let disabled = build_menu(MenuDecision::Items(t), [false; 2], &suggestions, mtm);
    assert!(
        disabled
            .children()
            .iter()
            .all(
                |c| c.downcast_ref::<UICommand>().is_some_and(|command| command
                    .attributes()
                    .contains(UIMenuElementAttributes::Disabled))
            )
    );
    println!(
        "SH1-152 S1/S2/S2c/S4/S5/S6: real delegate, token commands, retained-item rejection, no fallback, ordinary Edit PASS"
    );
}
