//! The smallest UIKit app, without GPUI or this crate's platform: it proves
//! that bundling, installing, launching, logging, and screenshots work in the
//! iPad simulator before anything of herdr-uikit is involved (herdr-gpui's
//! docs/ios.md, "Running in the simulator").
//!
//! It starts the way herdr-uikit does. herdr-gpui's `scripts/ios/Info.plist`
//! declares no scene configurations, so the application delegate supplies one
//! whose delegate class is this example's scene delegate. That delegate fills
//! its window with magenta and a white label, which the screenshot check finds.
//!
//! Examples run from an app bundle, which herdr-gpui's tooling builds; from a
//! herdr-gpui checkout:
//!
//! ```sh
//! just ios-sim uikit_smoke
//! ```
//!
//! On every other target `main` returns at once.

#[cfg(target_os = "ios")]
#[allow(
    unsafe_code,
    reason = "Objective-C classes defined with objc2 and UIKit setters it marks unsafe"
)]
mod smoke {
    use std::cell::OnceCell;

    use objc2::rc::{Allocated, Retained};
    use objc2::{
        ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    };
    use objc2_foundation::{NSObject, NSObjectProtocol, NSString};
    use objc2_ui_kit::{
        NSTextAlignment, UIApplication, UIApplicationDelegate, UIColor, UIFont, UILabel, UIScene,
        UISceneConfiguration, UISceneConnectionOptions, UISceneDelegate, UISceneSession,
        UIViewAutoresizing, UIViewController, UIWindow, UIWindowScene, UIWindowSceneDelegate,
    };

    /// Printed to stderr, which herdr-gpui's `scripts/ios/sim.sh` keeps in `stderr.log`;
    /// the example installs no tracing subscriber.
    const LOG_PREFIX: &str = "uikit_smoke:";

    define_class!(
        // SAFETY: NSObject has no subclassing requirements, and the class does
        // not implement Drop.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "HerdrUikitSmokeAppDelegate"]
        struct AppDelegate;

        // SAFETY: NSObjectProtocol has no requirements beyond NSObject's.
        unsafe impl NSObjectProtocol for AppDelegate {}

        // SAFETY: the method below has the signature UIKit calls it with.
        unsafe impl UIApplicationDelegate for AppDelegate {
            #[unsafe(method_id(application:configurationForConnectingSceneSession:options:))]
            fn configuration_for_connecting(
                &self,
                _application: &UIApplication,
                session: &UISceneSession,
                _options: &UISceneConnectionOptions,
            ) -> Retained<UISceneConfiguration> {
                let configuration = UISceneConfiguration::configurationWithName_sessionRole(
                    Some(&NSString::from_str("Default")),
                    &session.role(),
                    self.mtm(),
                );
                // SAFETY: SceneDelegate subclasses NSObject, has an `init` that
                // takes no arguments, and adopts UIWindowSceneDelegate, which
                // is what UIKit instantiates a window scene's delegate as.
                unsafe { configuration.setDelegateClass(Some(SceneDelegate::class())) };
                eprintln!(
                    "{LOG_PREFIX} configured a scene session with role {}",
                    session.role()
                );
                configuration
            }
        }
    );

    /// The window a scene delegate shows; UIKit keeps only the delegate.
    #[derive(Default)]
    struct SceneIvars {
        window: OnceCell<Retained<UIWindow>>,
    }

    define_class!(
        // SAFETY: NSObject has no subclassing requirements, and the class does
        // not implement Drop.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "HerdrUikitSmokeSceneDelegate"]
        #[ivars = SceneIvars]
        struct SceneDelegate;

        impl SceneDelegate {
            #[unsafe(method_id(init))]
            fn init(this: Allocated<Self>) -> Retained<Self> {
                let this = this.set_ivars(SceneIvars::default());
                // SAFETY: NSObject's init takes no arguments and returns the
                // initialized receiver.
                unsafe { msg_send![super(this), init] }
            }
        }

        // SAFETY: NSObjectProtocol has no requirements beyond NSObject's.
        unsafe impl NSObjectProtocol for SceneDelegate {}

        // SAFETY: the method below has the signature UIKit calls it with.
        unsafe impl UISceneDelegate for SceneDelegate {
            #[unsafe(method(scene:willConnectToSession:options:))]
            fn will_connect(
                &self,
                scene: &UIScene,
                _session: &UISceneSession,
                _options: &UISceneConnectionOptions,
            ) {
                let Some(window_scene) = scene.downcast_ref::<UIWindowScene>() else {
                    eprintln!("{LOG_PREFIX} a scene that is not a window scene connected");
                    return;
                };
                self.show_window(window_scene);
            }
        }

        // SAFETY: the method below has the signature UIKit calls it with.
        unsafe impl UIWindowSceneDelegate for SceneDelegate {
            #[unsafe(method_id(window))]
            fn window(&self) -> Option<Retained<UIWindow>> {
                self.ivars().window.get().cloned()
            }
        }
    );

    impl SceneDelegate {
        fn show_window(&self, window_scene: &UIWindowScene) {
            let mtm = self.mtm();
            let controller = UIViewController::new(mtm);
            let Some(view) = controller.view() else {
                eprintln!("{LOG_PREFIX} the root view controller loaded no view");
                return;
            };
            view.setBackgroundColor(Some(&UIColor::magentaColor()));

            let label = UILabel::new(mtm);
            label.setText(Some(&NSString::from_str("herdr-uikit smoke")));
            // SAFETY: textColor is set to a color, never nil.
            unsafe { label.setTextColor(Some(&UIColor::whiteColor())) };
            // SAFETY: font is set to a font, never nil.
            unsafe { label.setFont(Some(&UIFont::boldSystemFontOfSize(48.0))) };
            label.setTextAlignment(NSTextAlignment::Center);
            label.setFrame(view.bounds());
            label.setAutoresizingMask(
                UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
            );
            view.addSubview(&label);

            let window = UIWindow::initWithWindowScene(UIWindow::alloc(mtm), window_scene);
            window.setRootViewController(Some(&controller));
            window.makeKeyAndVisible();
            let bounds = window.bounds();
            eprintln!(
                "{LOG_PREFIX} window visible, {}x{} points",
                bounds.size.width, bounds.size.height
            );
            if self.ivars().window.set(window).is_err() {
                eprintln!("{LOG_PREFIX} the scene connected twice; keeping the first window");
            }
        }
    }

    pub fn run() {
        let Some(mtm) = MainThreadMarker::new() else {
            eprintln!("{LOG_PREFIX} main is not on the main thread");
            return;
        };
        eprintln!("{LOG_PREFIX} starting UIApplicationMain");
        let delegate = NSString::from_class(AppDelegate::class());
        UIApplication::main(None, Some(&delegate), mtm)
    }
}

fn main() {
    #[cfg(target_os = "ios")]
    smoke::run();
}
