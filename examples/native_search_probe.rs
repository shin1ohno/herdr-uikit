//! UIKit fixture for `present_native_search` and the native modal gate
//! (T-U1 to T-U7, review F2 to F6 and F8). Runs under UIApplicationMain on a
//! disposable Simulator through `scripts/check-native-search.py` with the
//! `NativeSearch/testProbe` XCUITest; `scripts/native-search-mutations.txt`
//! lists the source mutations each case must fail on. Not iPadOS hardware,
//! IME or Universal Clipboard proof.
#[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
#[path = "native_search_probe/mod.rs"]
mod probe;

fn main() {
    #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
    probe::run();
}
