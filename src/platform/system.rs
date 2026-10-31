//! App-wide UIKit requests and queries: the interface style, opening URLs in
//! other apps, and where the app is installed.

#![allow(
    unsafe_code,
    reason = "UIKit's trait-collection reads and URL opening are unsafe accessors in objc2"
)]

use std::path::PathBuf;

use block2::RcBlock;
use gpui::WindowAppearance;
use objc2::{MainThreadMarker, runtime::Bool};
use objc2_foundation::{NSBundle, NSDictionary, NSString, NSURL};
use objc2_ui_kit::{UIApplication, UIUserInterfaceStyle, UIWindowScene};

/// Light or dark, from the first connected window scene's traits. The style
/// is system-wide, so any window scene answers; before one connects there is
/// nothing to read, and the app is light.
pub(super) fn window_appearance(mtm: MainThreadMarker) -> WindowAppearance {
    let style = UIApplication::sharedApplication(mtm)
        .connectedScenes()
        .iter()
        .find_map(|scene| scene.downcast::<UIWindowScene>().ok())
        // SAFETY: The trait collection is read on the main thread, which the
        // marker proves, and UIKit only changes it there.
        .map(|scene| unsafe { scene.traitCollection().userInterfaceStyle() });
    if style == Some(UIUserInterfaceStyle::Dark) {
        WindowAppearance::Dark
    } else {
        WindowAppearance::Light
    }
}

/// Asks the system to open `url` in the app that handles it. The answer
/// arrives later; a refusal is logged with the URL's scheme only, since the
/// rest may carry tokens.
pub(super) fn open_url(url: &str, mtm: MainThreadMarker) {
    let scheme = scheme(url).to_owned();
    let Some(ns_url) = NSURL::URLWithString(&NSString::from_str(url)) else {
        tracing::warn!(%scheme, "not opening a URL Foundation cannot parse");
        return;
    };
    let completion = RcBlock::new(move |opened: Bool| {
        if !opened.as_bool() {
            tracing::warn!(%scheme, "the system did not open a URL");
        }
    });
    let options = NSDictionary::new();
    // SAFETY: The options dictionary is empty, so none of its values can be
    // of the wrong type, and the marker proves the main thread.
    unsafe {
        UIApplication::sharedApplication(mtm).openURL_options_completionHandler(
            &ns_url,
            &options,
            Some(&*completion),
        );
    }
}

/// The installed `.app` bundle.
pub(super) fn bundle_path() -> PathBuf {
    PathBuf::from(NSBundle::mainBundle().bundlePath().to_string())
}

/// The part of a URL before its first colon, or nothing when it has none.
fn scheme(url: &str) -> &str {
    url.split_once(':').map_or("", |(scheme, _)| scheme)
}
