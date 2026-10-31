//! The application delegate `UIApplicationMain` creates. Under the scene life
//! cycle it only chooses each scene's configuration and hears process-wide
//! events, memory warnings and termination; everything about a window goes
//! through the scene delegate.

#![allow(
    unsafe_code,
    reason = "defines an Objective-C class with objc2's define_class!"
)]

use objc2::{
    ClassType, MainThreadMarker, MainThreadOnly, define_class,
    rc::Retained,
    runtime::{AnyObject, NSObject},
};
use objc2_foundation::{NSDictionary, NSObjectProtocol, NSString};
use objc2_ui_kit::{
    UIApplication, UIApplicationDelegate, UIApplicationLaunchOptionsKey, UISceneConfiguration,
    UISceneConnectionOptions, UISceneSession, UIWindowScene, UIWindowSceneSessionRoleApplication,
};

use super::{scene_delegate::SceneDelegate, shared};

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and AppDelegate does
    // not implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    // A fixed name, like the scene delegate's, so an `Info.plist` can name
    // the class and the name does not change with the crate version.
    #[name = "HerdrAppDelegate"]
    pub(super) struct AppDelegate;

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
