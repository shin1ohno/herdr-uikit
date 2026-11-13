# CI

`.github/workflows/ci.yml` runs on pull requests and pushes to `main` on
GitHub-hosted `macos-26`, with read-only repository permission, no persisted
checkout credentials, a 40-minute timeout, and cancellation of superseded runs.
Actions use full commit SHA pins. The runner and checkout behavior follow
yebisu's CI at `aa457e9f0c06dbc706c64447cc3c4f839c3979f3`; this workflow
additionally replaces the checkout tag with the immutable `v4.3.1` commit.

The job installs the toolchain, components and targets declared in
`rust-toolchain.toml` via `rustup show active-toolchain`, then records the
compiler, host triple, installed targets, Xcode and both iOS SDK versions.
It uses the runner's selected Xcode and sets `IPHONEOS_DEPLOYMENT_TARGET=17.0`.

Checks run sequentially, stopping at the first failure:

```sh
cargo fmt --check --all
cargo test --locked --tests
cargo clippy --locked --all-targets --features test-support --target aarch64-apple-ios -- -D warnings
cargo clippy --locked --all-targets --features test-support --target aarch64-apple-ios-sim -- -D warnings
```

The explicit `test-support` feature is the only feature in the current manifest
and covers the input-handler regressions included in the prior SDK gates.
Clippy checks all targets, including test code, for both iOS architectures;
it does not execute iOS test binaries. Cargo keeps the checked-in lockfile.

The host library is empty because `src/lib.rs` is gated to iOS. Host tests
still execute the integration targets `tests/scene_fallback.rs` and
`tests/scene_requests.rs`, including their imported pure modules. They need
no optional feature. They do not replace the separate Simulator test suite
in the README, native UI/device QA, or consumer acceptance gates.

This job does not boot a Simulator, sign an app, access the keychain, or
contact a physical Mac. A failing source check must be addressed in a
separate SDK change; CI should retain its command and warning policy.
