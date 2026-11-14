//! NativeOperation receiving fixture. Injected selectors are not OS routing proof.
#[cfg(all(target_os = "ios", feature = "test-support"))]
#[path = "b1_probe/mod.rs"]
mod probe;

fn main() {
    #[cfg(all(target_os = "ios", feature = "test-support"))]
    probe::run();
}
