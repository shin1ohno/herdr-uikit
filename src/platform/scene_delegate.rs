//! The scene delegate. UIKit creates one per application scene, from the
//! class the application delegate names, and reports that scene's life to
//! it. Each event goes to `crate::window`, which owns windows, and the scene
//! states fold into GPUI's app lifecycle (`super::lifecycle`).

#![allow(
    unsafe_code,
    reason = "defines an Objective-C class with objc2's define_class!"
)]

use std::rc::Rc;

use objc2::{ClassType, MainThreadOnly, define_class, runtime::NSObject};
use objc2_foundation::{NSObjectProtocol, NSSet};
use objc2_ui_kit::{
    UIOpenURLContext, UIResponder, UIScene, UISceneConnectionOptions, UISceneDelegate,
    UISceneSession, UIWindowScene, UIWindowSceneDelegate,
};

use super::{
    lifecycle::SceneState,
    shared::{self, Shared},
};
use crate::window;

define_class!(
    // SAFETY: UIResponder adds no subclassing requirements to NSObject's,
    // and SceneDelegate does not implement Drop.
    #[unsafe(super(UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    // UIKit saves each scene session with its delegate class's name and
    // restores the session on a later launch by that name, so the name must
    // not change between builds. The default one ends in the crate version.
    #[name = "HerdrSceneDelegate"]
    pub(super) struct SceneDelegate;

    // SAFETY: NSObjectProtocol asks nothing more of an NSObject subclass.
    unsafe impl NSObjectProtocol for SceneDelegate {}

    // SAFETY: Every method of the protocol is optional, and each one below
    // has the signature objc2-ui-kit declares for its selector.
    unsafe impl UISceneDelegate for SceneDelegate {
        #[unsafe(method(scene:willConnectToSession:options:))]
        fn will_connect(
            &self,
            scene: &UIScene,
            _session: &UISceneSession,
            options: &UISceneConnectionOptions,
        ) {
            let Some((window_scene, shared)) = hosted(scene) else {
                return;
            };
            let mtm = self.mtm();
            window::scene_connected(window_scene, mtm);
            shared.scene_connected(shared::scene_key(scene), mtm);
            // A cold launch from a link carries the URLs here instead of in
            // `scene:openURLContexts:`; GPUI has launched by now.
            shared.deliver_urls(urls(&options.URLContexts()));
        }

        #[unsafe(method(sceneDidDisconnect:))]
        fn did_disconnect(&self, scene: &UIScene) {
            let Some((window_scene, shared)) = hosted(scene) else {
                return;
            };
            window::scene_disconnected(window_scene, self.mtm());
            shared.scene_changed(shared::scene_key(scene), None);
        }

        #[unsafe(method(sceneWillEnterForeground:))]
        fn will_enter_foreground(&self, scene: &UIScene) {
            if let Some((_, shared)) = hosted(scene) {
                shared.scene_changed(shared::scene_key(scene), Some(SceneState::Foreground));
            }
        }

        #[unsafe(method(sceneDidBecomeActive:))]
        fn did_become_active(&self, scene: &UIScene) {
            let Some((window_scene, shared)) = hosted(scene) else {
                return;
            };
            window::scene_activation_changed(window_scene, true, self.mtm());
            shared.scene_changed(shared::scene_key(scene), Some(SceneState::Active));
        }

        #[unsafe(method(sceneWillResignActive:))]
        fn will_resign_active(&self, scene: &UIScene) {
            let Some((window_scene, shared)) = hosted(scene) else {
                return;
            };
            window::scene_activation_changed(window_scene, false, self.mtm());
            shared.scene_changed(shared::scene_key(scene), Some(SceneState::Foreground));
        }

        #[unsafe(method(sceneDidEnterBackground:))]
        fn did_enter_background(&self, scene: &UIScene) {
            if let Some((_, shared)) = hosted(scene) {
                shared.scene_changed(shared::scene_key(scene), Some(SceneState::Background));
            }
        }

        #[unsafe(method(scene:openURLContexts:))]
        fn open_url_contexts(&self, scene: &UIScene, contexts: &NSSet<UIOpenURLContext>) {
            if let Some((_, shared)) = hosted(scene) {
                shared.deliver_urls(urls(contexts));
            }
        }
    }

    // SAFETY: Every method of the protocol is optional, and SceneDelegate
    // implements none: `crate::window` creates each scene's `UIWindow`
    // itself, so the delegate's `window` property is not needed.
    unsafe impl UIWindowSceneDelegate for SceneDelegate {}
);

/// Registers the class with the Objective-C runtime. A restored scene session
/// names its delegate class without UIKit asking the application delegate
/// for a configuration, so the class must exist before `UIApplicationMain`
/// restores any session.
pub(super) fn register() {
    SceneDelegate::class();
}

/// The window scene and the platform state for an event, or `None`, logged,
/// when either is missing. The application delegate only gives this class to
/// application sessions, whose scenes are window scenes.
fn hosted(scene: &UIScene) -> Option<(&UIWindowScene, Rc<Shared>)> {
    let Some(window_scene) = scene.downcast_ref::<UIWindowScene>() else {
        tracing::warn!("ignoring a scene that is not a window scene");
        return None;
    };
    let Some(shared) = shared::current() else {
        tracing::error!("a scene event arrived before the platform ran");
        return None;
    };
    Some((window_scene, shared))
}

fn urls(contexts: &NSSet<UIOpenURLContext>) -> Vec<String> {
    contexts
        .iter()
        .filter_map(|context| context.URL().absoluteString())
        .map(|url| url.to_string())
        .collect()
}

#[cfg(test)]
mod tests;
