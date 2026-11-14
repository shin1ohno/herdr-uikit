#![allow(
    unsafe_code,
    reason = "typed main-thread UIKit delegate and standard editing selectors"
)]
use super::{Window, state::*};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::AnyObject, sel,
};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSString};
use objc2_ui_kit::{
    UICommand, UIEditMenuConfiguration, UIEditMenuInteraction, UIEditMenuInteractionDelegate,
    UIMenu, UIMenuElement, UIMenuElementAttributes,
};
use std::rc::Weak;

pub(super) struct DelegateOwner {
    owner: Weak<Window>,
    #[cfg(any(test, feature = "test-support"))]
    fixture: Option<std::rc::Rc<std::cell::RefCell<EditMenuState>>>,
}

define_class!(
    // SAFETY: NSObject subclass; weak owner drops with ivars, no custom Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrUIKitEditMenuDelegate"]
    #[ivars = DelegateOwner]
    pub(super) struct EditMenuDelegate;
    unsafe impl NSObjectProtocol for EditMenuDelegate {}
    unsafe impl UIEditMenuInteractionDelegate for EditMenuDelegate {
        #[unsafe(method_id(editMenuInteraction:menuForConfiguration:suggestedActions:))]
        fn menu(
            &self,
            _: &UIEditMenuInteraction,
            configuration: &UIEditMenuConfiguration,
            suggested: &NSArray<UIMenuElement>,
        ) -> Option<Retained<UIMenu>> {
            let _native_entry = crate::native_operation::refused();
            Some(self.menu_for(configuration, suggested))
        }
    }
);

impl EditMenuDelegate {
    pub(super) fn new(owner: Weak<Window>, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DelegateOwner {
            owner,
            #[cfg(any(test, feature = "test-support"))]
            fixture: None,
        });
        // SAFETY: designated NSObject initializer after initialized ivars.
        unsafe { msg_send![super(this), init] }
    }
    fn menu_for(
        &self,
        configuration: &UIEditMenuConfiguration,
        suggested: &NSArray<UIMenuElement>,
    ) -> Retained<UIMenu> {
        let identifier = configuration.identifier();
        let object: &AnyObject = (*identifier).as_ref();
        let identifier = object.downcast_ref::<NSString>().map(ToString::to_string);
        #[cfg(any(test, feature = "test-support"))]
        if let Some(fixture) = &self.ivars().fixture {
            return build_menu(
                fixture.borrow().menu_for(identifier.as_deref()),
                [true; 2],
                suggested,
                self.mtm(),
            );
        }
        let owner = self.ivars().owner.upgrade();
        let decision = owner.as_ref().map_or_else(
            || EditMenuState::default().menu_for(identifier.as_deref()),
            |w| w.edit_menu.state.borrow().menu_for(identifier.as_deref()),
        );
        let enabled = owner.as_ref().map_or([false; 2], |w| {
            [
                w.text_input.editing_enabled(sel!(copy:)),
                w.text_input.editing_enabled(sel!(paste:)),
            ]
        });
        build_menu(decision, enabled, suggested, self.mtm())
    }
}

pub(super) fn build_menu(
    decision: MenuDecision,
    enabled: [bool; 2],
    suggested: &NSArray<UIMenuElement>,
    mtm: MainThreadMarker,
) -> Retained<UIMenu> {
    let elements = match decision {
        MenuDecision::Passthrough => return UIMenu::menuWithChildren(suggested, mtm),
        // P4 on iOS 26.5: empty stays empty. Never return nil or suggestions.
        MenuDecision::Empty => Vec::new(),
        MenuDecision::Items(token) => {
            let key = NSString::from_str(EDIT_MENU_PROPERTY_KEY);
            let value = NSNumber::new_u64(token.value());
            let objects: [&AnyObject; 1] = [&value];
            let properties = NSDictionary::from_slices(&[&*key], &objects);
            [("Copy", sel!(copy:)), ("Paste", sel!(paste:))]
                .into_iter()
                .zip(enabled)
                .map(|((title, selector), enabled)| {
                    // SAFETY: standard selector and property-list dictionary.
                    let command = unsafe {
                        UICommand::commandWithTitle_image_action_propertyList(
                            &NSString::from_str(title),
                            None,
                            selector,
                            Some(&properties),
                            mtm,
                        )
                    };
                    command.setAttributes(if enabled {
                        UIMenuElementAttributes::empty()
                    } else {
                        UIMenuElementAttributes::Disabled
                    });
                    Retained::into_super(command)
                })
                .collect()
        }
    };
    UIMenu::menuWithChildren(&NSArray::from_retained_slice(&elements), mtm)
}

pub(in crate::window) fn classify(sender: Option<&AnyObject>) -> Sender {
    let Some(command) = sender.and_then(|s| s.downcast_ref::<UICommand>()) else {
        return Sender::Other;
    };
    let Some(properties) = command.propertyList() else {
        return Sender::Command;
    };
    // NSDictionary generic types are checked again at the value boundary.
    let Some(dictionary) = properties.downcast_ref::<NSDictionary>() else {
        return Sender::Command;
    };
    let Some(value) = dictionary.objectForKey(&NSString::from_str(EDIT_MENU_PROPERTY_KEY)) else {
        return Sender::Command;
    };
    let token = value.downcast_ref::<NSNumber>().and_then(|n| {
        let value = n.as_u64();
        // Reject fractional/negative numbers instead of silently truncating them.
        n.isEqualToNumber(&NSNumber::new_u64(value))
            .then_some(value)
    });
    Sender::Tagged(token)
}

#[cfg(any(test, feature = "test-support"))]
impl EditMenuDelegate {
    pub(super) fn fixture(
        state: std::rc::Rc<std::cell::RefCell<EditMenuState>>,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DelegateOwner {
            owner: Weak::new(),
            fixture: Some(state),
        });
        // SAFETY: main-thread test-owned delegate with initialized ivars.
        unsafe { msg_send![super(this), init] }
    }
}
