//! P0 premise probe for the native search panel: a UIKit view controller
//! presented over the window's root controller, as a popover and as a sheet,
//! whose `UISearchTextField` becomes first responder. Records whether either
//! GPUI view is first responder or in the field's responder chain, and which
//! side receives hardware keys, composition, Paste and dictation selectors.
//! Driven by `scripts/check-native-search.py` and its XCUITest on a leased
//! Simulator; not iPadOS hardware or IME proof.
#[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
#[path = "native_search_premise/mod.rs"]
mod probe;

fn main() {
    #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
    probe::run();
}
