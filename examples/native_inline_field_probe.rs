//! UIKit fixture for `present_native_inline_field` over a GPUI label, on the
//! shared native modal gate. Runs under UIApplicationMain on a disposable
//! Simulator through `scripts/check-native-search.py` with the
//! `InlineField/testProbe` XCUITest; `scripts/native_inline_field_mutations.py`
//! lists the source mutations each case must fail on. Not iPadOS hardware or
//! IME proof.
#[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
#[path = "native_inline_field_probe/mod.rs"]
mod probe;

fn main() {
    #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
    probe::run();
}
