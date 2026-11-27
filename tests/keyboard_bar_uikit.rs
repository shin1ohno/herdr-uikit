//! The shortcuts bar's UIKit objects (`src/window/keyboard_bar/contract.rs`),
//! on the process main thread without UIApplicationMain.
#[cfg(target_os = "ios")]
fn main() {
    herdr_uikit::test_keyboard_bar_contract();
}
#[cfg(not(target_os = "ios"))]
fn main() {
    println!("UIKit contract requires the iOS Simulator target");
}
