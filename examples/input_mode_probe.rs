//! UIKit fixture for the selection display's input mode indicator (issue
//! #139): the system indicator shows at the GPUI text view's caret when the
//! input source changes, under a transparent cursor view, only while the text
//! view is first responder, and it follows retirement and window close. Runs
//! under UIApplicationMain on a disposable Simulator through
//! `scripts/check-native-search.py` with the `InputMode/testInputMode`
//! XCUITest; `scripts/input_mode_mutations.py` lists the source mutations
//! each case must fail on. Not iPadOS hardware, hardware keyboard or IME proof.
#[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
#[path = "input_mode_probe/mod.rs"]
mod probe;

fn main() {
    #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
    probe::run();
}
