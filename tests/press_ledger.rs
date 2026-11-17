//! The held-press ledger's host tests: the module is std only, so its own
//! `#[cfg(test)]` tests run here as well as in the Simulator and in the
//! extracted-source fixture (`scripts/check-b1-source.py`).
#[path = "../src/window/state/press_ledger.rs"]
mod press_ledger;
