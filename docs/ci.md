# CI

`.github/workflows/ci.yml` runs on pull requests and pushes to `main` and
the `feat/native-edit-menu-sh1-152` development branch on
GitHub-hosted `macos-26`, with read-only repository permission, no persisted
checkout credentials, a 40-minute timeout, and cancellation of superseded runs.
Actions use full commit SHA pins. The runner and checkout behavior follow
yebisu's CI at `aa457e9f0c06dbc706c64447cc3c4f839c3979f3`; this workflow
additionally replaces the checkout tag with the immutable `v4.3.1` commit.
Checkout includes history so the responder-routing check can compare the
known five-call baseline commit before checking the current source.

The job installs the toolchain, components and targets declared in
`rust-toolchain.toml` via `rustup show active-toolchain`, then records the
compiler, host triple, installed targets, Xcode and both iOS SDK versions.
It uses the runner's selected Xcode and sets `IPHONEOS_DEPLOYMENT_TARGET=17.0`.

Checks run sequentially, stopping at the first failure:

```sh
cargo fmt --check --all
cargo test --locked --tests
cargo clippy --locked --target aarch64-apple-ios -- -D warnings
cargo clippy --locked --target aarch64-apple-ios-sim -- -D warnings
cargo clippy --locked --all-targets --features test-support --target aarch64-apple-ios -- -D warnings
cargo clippy --locked --all-targets --features test-support --target aarch64-apple-ios-sim -- -D warnings
bash scripts/check-responder-calls.sh
python3 scripts/test-simulator.py
```

The two clippy runs without features build the product configuration, so a
probe hook that escapes its `#[cfg(feature = "test-support")]` gate fails CI.
The explicit `test-support` feature is the only feature in the current manifest
and covers the input-handler regressions and native edit-menu object contracts.
Clippy checks all targets, including test code, for both iOS architectures;
it does not execute iOS test binaries. Cargo keeps the checked-in lockfile.

The host library is empty because `src/lib.rs` is gated to iOS. Host tests
still execute the scene integration targets and `tests/edit_menu_state.rs`,
including their imported pure modules and the production responder-switch guard.
They need no optional feature.

The Simulator step creates an available iPad Pro 13-inch device, runs
`cargo test --locked --all-features --target aarch64-apple-ios-sim` through
`scripts/sim-runner.sh`, and deletes only that device in a `finally` block.
`edit_menu_uikit` runs without the Rust test harness so actual UIKit delegate
and command objects are exercised on the main thread. Like the other SDK
tests, it runs outside `UIApplicationMain`: actual menu presentation and
responder transitions remain consumer Simulator E2E gates. Device-only input
checks remain separate owner QA.

That job does not sign an app, access the keychain, or contact a physical Mac.
CI retains its command and warning policy when a source check fails.

## Native cut-query stages

A second job, `native-cut-query`, runs the native stages B1-B5 for the
test-support helper `NativeClient::cut_and_observe_selection` (SH1-234) with
`scripts/check-cut-query-native.py`. It has the same triggers, runner,
permissions, checkout pin and environment as the SDK checks, a 60-minute
timeout, and it checks out the exact pull-request head (not the merge ref) and
refuses to run unless `git rev-parse HEAD` equals that head and the checkout
is clean. The script refuses to run outside GitHub Actions.

It creates one disposable iPad Pro 13-inch Simulator, then builds
`examples/cut_query_probe.rs` five times, bundles each build as an ad-hoc
signed Simulator app, installs it and launches it under `UIApplicationMain`:

| Stage | Source | Launch | Expected | Cuts |
|---|---|---|---|---|
| B1 | head | L1 default | PASS: main, nil, loan, disabled, retired (last) | 2 |
| B2 | fixture `NEXT_TURN = true` (M1) | L2 | ASSERT `p13-cut-first-before-next-turn` | 1 |
| B3 | exact restore | L3 `--main-only` | PASS | 1 |
| B4 | core early `selected_range_utf16` (M2) | L4 `--main-only` | ASSERT `p13-cut-first-no-earlier-native-query` | 1 |
| B5 | exact restore | L5 `--main-only`, L6 `--explicit-policy` | PASS, PASS | 2 |

Each launch must print every required `CUT_QUERY_PROBE_CHECK` line; a negative
must fail only with its named assertion. Each build records source, build and
installed binary hashes, the UUID and CDHash; the job log ends with one
`NATIVE_LEDGER` JSON line and `NATIVE_PASS` or `NATIVE_STOP`. Mutated sources
are restored byte for byte and `git status` must be clean afterwards. The
Simulator is deleted in a `finally` block. Simulator injection is not OS IME
routing evidence and does not certify consumer (P13) or physical-device gates.
