//! The device's thermal state from `NSProcessInfo`, and its change
//! notification. Foundation may post the notification on any thread, so its
//! observer is not a main-thread object; it forwards to the main queue.

#![allow(
    unsafe_code,
    reason = "defines an Objective-C observer class and registers it by selector"
)]

use gpui::ThermalState;
use objc2::{AllocAnyThread, define_class, msg_send, rc::Retained, runtime::NSObject, sel};
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSObjectProtocol, NSProcessInfo,
    NSProcessInfoThermalState, NSProcessInfoThermalStateDidChangeNotification,
};

use super::shared;

pub(super) fn thermal_state() -> ThermalState {
    match NSProcessInfo::processInfo().thermalState() {
        NSProcessInfoThermalState::Fair => ThermalState::Fair,
        NSProcessInfoThermalState::Serious => ThermalState::Serious,
        NSProcessInfoThermalState::Critical => ThermalState::Critical,
        _ => ThermalState::Nominal,
    }
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and ThermalObserver
    // does not implement Drop.
    #[unsafe(super(NSObject))]
    pub(super) struct ThermalObserver;

    // SAFETY: NSObjectProtocol asks nothing more of an NSObject subclass.
    unsafe impl NSObjectProtocol for ThermalObserver {}

    impl ThermalObserver {
        // SAFETY: The notification center calls an observer's selector with
        // one `NSNotification` argument and expects no result.
        #[unsafe(method(thermalStateDidChange:))]
        fn thermal_state_did_change(&self, _notification: &NSNotification) {
            let _native_entry = crate::native_operation::refused();
            shared::later(|shared| {
                shared.thermal_state_change.call(|callback| callback());
            });
        }
    }
);

impl ThermalObserver {
    /// An observer of thermal state changes, registered with the default
    /// notification center. The center does not retain it, so the caller
    /// keeps it alive for as long as it should observe.
    pub(super) fn register() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        // SAFETY: `init` is NSObject's designated initializer, and the
        // (empty) ivars were set just above.
        let observer: Retained<Self> = unsafe { msg_send![super(this), init] };
        // SAFETY: `thermalStateDidChange:` is defined on this class with the
        // single `NSNotification` argument the center calls observers with,
        // and the notification name is a constant Foundation exports.
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                &observer,
                sel!(thermalStateDidChange:),
                Some(NSProcessInfoThermalStateDidChangeNotification),
                None,
            );
        }
        observer
    }
}
