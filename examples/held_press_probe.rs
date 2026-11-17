//! Held-press receiving fixture: injected presses through the actual press
//! selectors, the press ledger and the SDK repeat timer. Not iPadOS delivery
//! proof. Run by `scripts/check-public-held-press.py` on a leased Simulator.
#[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
#[path = "held_press_probe/mod.rs"]
mod probe;

fn main() {
    #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
    probe::run();
}
