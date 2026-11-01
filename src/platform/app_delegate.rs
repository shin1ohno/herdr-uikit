//! The application delegate `UIApplicationMain` creates. Under the scene life
//! cycle it only chooses each scene's configuration and hears process-wide
//! events, memory warnings and termination; everything about a window goes
//! through the scene delegate.
//!
//! It is a `UIResponder`, the last one in every responder chain, so it also
//! builds the main menu (`buildMenuWithBuilder:`) and performs GPUI's menu
//! commands when no window's drawing view took them first (`menus`).

#![allow(
    unsafe_code,
    reason = "defines an Objective-C class with objc2's define_class!"
)]

use std::ptr;

use objc2::{
    ClassType, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, NSObject, ProtocolObject, Sel},
};
use objc2_foundation::{NSDictionary, NSObjectProtocol, NSString};
use objc2_ui_kit::{
    UIApplication, UIApplicationDelegate, UIApplicationLaunchOptionsKey, UICommand, UIMenuBuilder,
    UIMenuSystem, UIResponder, UISceneConfiguration, UISceneConnectionOptions, UISceneSession,
    UIWindowScene, UIWindowSceneSessionRoleApplication,
};

use super::{
    menus::{self, Performed},
    scene_delegate::SceneDelegate,
    shared,
};

define_class!(
    // SAFETY: UIResponder asks subclasses only to keep its `init`, which
    // `UIApplicationMain` calls, and AppDelegate does not implement Drop.
    // Every override keeps the superclass signature.
    #[unsafe(super(UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    // A fixed name, like the scene delegate's, so an `Info.plist` can name
    // the class and the name does not change with the crate version.
    #[name = "HerdrAppDelegate"]
    pub(super) struct AppDelegate;

    impl AppDelegate {
        #[unsafe(method(buildMenuWithBuilder:))]
        fn build_menu(&self, builder: &ProtocolObject<dyn UIMenuBuilder>) {
            // SAFETY: the argument is the builder UIKit passed for this
            // selector; UIResponder's implementation returns nothing.
            let () = unsafe { msg_send![super(self), buildMenuWithBuilder: builder] };
            let mtm = self.mtm();
            // Context menus are built through this method too; GPUI's menus
            // belong in the main menu only.
            if ptr::eq(&*builder.system(), &*UIMenuSystem::mainSystem(mtm)) {
                menus::build(builder, mtm);
            }
        }

        // Reached when no window's drawing view is in the responder chain,
        // so the command can only dispatch its action.
        #[unsafe(method(herdrMenuAction:))]
        fn menu_action(&self, sender: Option<&AnyObject>) {
            if let Some(performed) = Performed::new(sender) {
                performed.dispatch_action();
            }
        }

        #[unsafe(method(validateCommand:))]
        fn validate_command(&self, command: &UICommand) {
            // SAFETY: as in `buildMenuWithBuilder:`, for the command.
            let () = unsafe { msg_send![super(self), validateCommand: command] };
            menus::validate(command);
        }

        #[unsafe(method(canPerformAction:withSender:))]
        fn can_perform_action(&self, action: Sel, sender: Option<&AnyObject>) -> bool {
            // define_class! converts the returned value, so this is one
            // expression rather than an early `return`.
            action == menus::action_selector()
                // SAFETY: the arguments are the ones UIKit passed;
                // UIResponder's implementation returns a BOOL.
                || unsafe { msg_send![super(self), canPerformAction: action, withSender: sender] }
        }
    }

    // SAFETY: NSObjectProtocol asks nothing more of an NSObject subclass.
    unsafe impl NSObjectProtocol for AppDelegate {}

    // SAFETY: Every method of the protocol is optional, and each one below
    // has the signature objc2-ui-kit declares for its selector.
    unsafe impl UIApplicationDelegate for AppDelegate {
        #[unsafe(method(application:didFinishLaunchingWithOptions:))]
        fn did_finish_launching(
            &self,
            _application: &UIApplication,
            _options: Option<&NSDictionary<UIApplicationLaunchOptionsKey, AnyObject>>,
        ) -> bool {
            // GPUI launches when the first scene connects, so it has a scene
            // to open its first window in.
            true
        }

        #[unsafe(method_id(application:configurationForConnectingSceneSession:options:))]
        fn configuration_for_connecting(
            &self,
            _application: &UIApplication,
            session: &UISceneSession,
            _options: &UISceneConnectionOptions,
        ) -> Retained<UISceneConfiguration> {
            scene_configuration(session, self.mtm())
        }

        #[unsafe(method(applicationDidReceiveMemoryWarning:))]
        fn did_receive_memory_warning(&self, _application: &UIApplication) {
            shared::later(|shared| {
                shared.memory_warning.call(|callback| callback());
            });
        }

        #[unsafe(method(applicationWillTerminate:))]
        fn will_terminate(&self, _application: &UIApplication) {
            // The process ends when this returns, so GPUI shuts down now
            // rather than from the main queue.
            if let Some(shared) = shared::current() {
                shared.quit.call(|quit| quit());
            }
        }
    }
);

/// The class name `UIApplicationMain` instantiates as the delegate.
pub(super) fn class_name() -> Retained<NSString> {
    NSString::from_class(AppDelegate::class())
}

/// Application scenes get the platform's scene delegate. Sessions of other
/// roles, such as a non-interactive external display, keep the configuration
/// UIKit derived from `Info.plist`, so the app does not claim them.
fn scene_configuration(
    session: &UISceneSession,
    mtm: MainThreadMarker,
) -> Retained<UISceneConfiguration> {
    let role = session.role();
    // SAFETY: The role is a constant string UIKit exports.
    let application_role = unsafe { UIWindowSceneSessionRoleApplication };
    if !role.isEqualToString(application_role) {
        tracing::debug!(%role, "leaving a scene session of another role to UIKit");
        return session.configuration();
    }
    let configuration = UISceneConfiguration::configurationWithName_sessionRole(None, &role, mtm);
    // SAFETY: UIWindowScene is the scene class of the application role, and
    // SceneDelegate is a UIResponder adopting UIWindowSceneDelegate, which is
    // what UIKit instantiates for it.
    unsafe {
        configuration.setSceneClass(Some(UIWindowScene::class()));
        configuration.setDelegateClass(Some(SceneDelegate::class()));
    }
    configuration
}

#[cfg(test)]
mod tests;
