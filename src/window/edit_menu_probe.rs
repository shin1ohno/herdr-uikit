//! Temporary Simulator preflight for SH1-152 P1-P4, before choosing the adapter.
#![allow(
    unsafe_code,
    reason = "Simulator-only UIKit preflight on the main thread"
)]

use std::{cell::Cell, io::Write};

use objc2::{
    DefinedClass, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, ProtocolObject},
    sel,
};
use objc2_core_foundation::CGPoint;
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSString};
use objc2_ui_kit::{
    UICommand, UIEditMenuConfiguration, UIEditMenuInteraction, UIEditMenuInteractionDelegate,
    UIMenu, UIMenuElement, UIView,
};

pub(super) struct Probe {
    delegate: Retained<ProbeDelegate>,
    interaction: Retained<UIEditMenuInteraction>,
}

impl Probe {
    pub(super) fn new(view: &UIView) -> Self {
        let this = ProbeDelegate::alloc(view.mtm()).set_ivars(Cell::new(false));
        // SAFETY: designated NSObject initializer after setting ivars.
        let delegate: Retained<ProbeDelegate> = unsafe { msg_send![super(this), init] };
        let interaction = UIEditMenuInteraction::initWithDelegate(
            UIEditMenuInteraction::alloc(view.mtm()),
            Some(ProtocolObject::from_ref(&*delegate)),
        );
        view.addInteraction(ProtocolObject::from_ref(&*interaction));
        Self {
            delegate,
            interaction,
        }
    }

    pub(super) fn present(&self, empty: bool) {
        self.delegate.ivars().set(empty);
        log(&format!("present empty={empty}"));
        let identifier = NSString::from_str("dev.herdr.uikit.edit-menu.config:42");
        // SAFETY: NSString conforms to NSCopying; UIKit copies it.
        let configuration = unsafe {
            UIEditMenuConfiguration::configurationWithIdentifier_sourcePoint(
                Some(ProtocolObject::from_ref(&*identifier)),
                CGPoint::new(350.0, 350.0),
                self.delegate.mtm(),
            )
        };
        self.interaction
            .presentEditMenuWithConfiguration(&configuration);
    }
}

define_class!(
    // SAFETY: NSObject subclass with no custom Drop, main-thread-only delegate.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrEditMenuPreflightDelegate"]
    #[ivars = Cell<bool>]
    pub(super) struct ProbeDelegate;
    unsafe impl NSObjectProtocol for ProbeDelegate {}
    unsafe impl UIEditMenuInteractionDelegate for ProbeDelegate {
        #[unsafe(method_id(editMenuInteraction:menuForConfiguration:suggestedActions:))]
        fn menu(&self, _: &UIEditMenuInteraction, _: &UIEditMenuConfiguration, suggested: &NSArray<UIMenuElement>) -> Option<Retained<UIMenu>> {
            let elements = if self.ivars().get() { Vec::new() } else { self.commands() };
            log(&format!("delegate empty={} suggested={} returned={}", self.ivars().get(), suggested.count(), elements.len()));
            Some(UIMenu::menuWithChildren(&NSArray::from_retained_slice(&elements), self.mtm()))
        }
    }
);

impl ProbeDelegate {
    fn commands(&self) -> Vec<Retained<UIMenuElement>> {
        let key = NSString::from_str("dev.herdr.uikit.edit-menu.token");
        let value = NSNumber::new_u64(42);
        let objects: [&AnyObject; 1] = [&value];
        let properties = NSDictionary::from_slices(&[&*key], &objects);
        [("Copy", sel!(copy:)), ("Paste", sel!(paste:))]
            .into_iter()
            .map(|(title, selector)| {
                // SAFETY: standard editing selector and property-list dictionary.
                let command = unsafe {
                    UICommand::commandWithTitle_image_action_propertyList(
                        &NSString::from_str(title),
                        None,
                        selector,
                        Some(&properties),
                        self.mtm(),
                    )
                };
                Retained::into_super(command)
            })
            .collect()
    }
}

pub(super) fn sender(view: &str, action: &str, sender: Option<&AnyObject>) {
    log(&format!(
        "sender view={view} action={action} object={sender:?}"
    ));
    if let Some(sender) = sender {
        // SAFETY: query propertyList only when supported, on the main thread.
        unsafe {
            let responds: bool = msg_send![sender, respondsToSelector:sel!(propertyList)];
            if responds {
                let properties: Option<Retained<AnyObject>> = msg_send![sender, propertyList];
                if let Some(properties) = properties {
                    let description: Retained<NSString> = msg_send![&*properties, description];
                    log(&format!("propertyList={description}"));
                } else {
                    log("propertyList=nil");
                }
            }
        }
    }
}

pub(super) fn log(message: &str) {
    if std::env::var_os("HERDR_EDIT_MENU_PROBE").is_none() {
        return;
    }
    if let Some(home) = std::env::var_os("HOME") {
        let path = std::path::PathBuf::from(home).join("Documents/sh1-152-sdk-probe.log");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{message}");
        }
    }
}
