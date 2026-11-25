//! The inline field's UIKit object contracts, on the process main thread
//! inside the Simulator (`scripts/test-simulator.py`).
#[cfg(target_os = "ios")]
fn main() {
    herdr_uikit::test_native_inline_field_contract();
}
#[cfg(not(target_os = "ios"))]
fn main() {
    println!("UIKit contract requires the iOS Simulator target");
}
