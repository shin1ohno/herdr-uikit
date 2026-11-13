#[cfg(target_os = "ios")]
fn main() {
    herdr_uikit::test_native_edit_menu_contract();
}
#[cfg(not(target_os = "ios"))]
fn main() {
    println!("UIKit contract requires the iOS Simulator target");
}
