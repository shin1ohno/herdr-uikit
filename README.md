# herdr-uikit

A [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) platform
for iPadOS, built on UIKit. GPUI ships no iOS platform, but
`gpui::Application::with_platform` accepts any `gpui::Platform`; this crate
supplies one, so a GPUI app runs on an iPad:

```rust
#[cfg(target_os = "ios")]
fn main() {
    herdr_uikit::application().run(|cx| {
        // open windows as on any other platform
    });
}
```

`herdr_uikit::application()` is
`gpui::Application::with_platform(Rc::new(IosPlatform::new()))`, the
counterpart of `gpui_platform::application()` on desktop targets. Build an
`IosPlatform` yourself to keep a handle for lifecycle phases.

What it provides:

- **Lifecycle.** `Platform::run` enters `UIApplicationMain` and never returns.
  GPUI's launch callback runs when the first `UIWindowScene` connects, and
  `BackgroundTask` holds on to time to finish work, such as saving state,
  after the app leaves the screen.
- **Windows.** Each GPUI window is a scene with a `UIView` hosting a
  `CAMetalLayer`, paced by `CADisplayLink`; displays are `UIScreen`s.
- **Rendering.** The Metal renderer and its shaders, ported from GPUI's macOS
  platform, recovering from failed Metal allocations.
- **Text.** The Core Text text system, also ported from GPUI's macOS platform.
- **Input.** Raw touches (GPUI's core turns them into clicks, scrolling, and
  drags), pointer and hover, hardware keys, and `UITextInput` for the software
  keyboard and IME composition.
- **Everything else.** A GCD dispatcher, the pasteboard, thermal state, and
  idle-timer control.

The crate is empty on every target other than iOS: every dependency sits
under `cfg(target_os = "ios")`, so a cross-platform workspace can depend on it
unconditionally.

## Status

Experimental. It runs Herdr UI's iPadOS build in the simulator and on an iPad,
and pins `gpui-pre` (GPUI as published from Zed) to exactly `0.3.6`: the
renderer's shader ABI is generated from that snapshot's sources (`build.rs`),
so another GPUI version needs the port re-checked. The crate is not published;
depend on it by path or by git.

Not supported yet: Core Video paint surfaces (GPUI exposes them only on
macOS; they are skipped with a one-time warning), and anything GPUI's
`Platform` trait asks of a desktop that iPadOS has no counterpart for.

## Building

Requires macOS with Xcode and the iOS simulator runtime. `rust-toolchain.toml`
pins the toolchain and its two iOS targets: `aarch64-apple-ios` for devices
and `aarch64-apple-ios-sim` for the simulator on an Apple silicon Mac.

Build and lint with the deployment target set, since rustc links the minimum
OS version into every artifact:

```sh
export IPHONEOS_DEPLOYMENT_TARGET=17.0
cargo clippy --locked --target aarch64-apple-ios --all-targets -- -D warnings
cargo clippy --locked --target aarch64-apple-ios-sim --all-targets -- -D warnings
cargo check --locked   # the host build, where the crate is empty
cargo fmt --check
```

`build.rs` reads `gpui-pre`'s sources from Cargo's registry, at the version
`Cargo.toml` pins exactly, to generate the shader header, so fetch
dependencies (`cargo fetch`) before an offline build. It refuses a range, a
path, or a git source for `gpui-pre`, since those could compile other sources
than the ones it reads.

## Testing

The tests run inside the iPad simulator. `scripts/sim-runner.sh` is a Cargo
runner that boots the simulator when it is not running, never shuts it down,
and runs each test binary there with `xcrun simctl spawn`:

```sh
export IPHONEOS_DEPLOYMENT_TARGET=17.0
CARGO_TARGET_AARCH64_APPLE_IOS_SIM_RUNNER=scripts/sim-runner.sh \
  cargo test --locked --target aarch64-apple-ios-sim
```

`HERDR_IOS_SIMULATOR` picks the simulator by UDID or device name; the default
is `iPad Pro 13-inch (M5)`. A name resolves to an available device, preferring
a booted one, then the newest iOS runtime. Test binaries run as plain
processes outside an app bundle, so nothing that needs `UIApplicationMain`
is tested this way.

## Examples

`examples/uikit_smoke.rs` is UIKit alone, without GPUI or this crate, to prove
bundling, launching, and screenshots work; `examples/hello.rs` is a GPUI app
on `herdr_uikit::application()`. Both build here
(`cargo build --example hello --target aarch64-apple-ios-sim`), but an iOS app
must run from an `.app` bundle with an `Info.plist`, which this repository
does not assemble. herdr-gpui's `scripts/ios/bundle.sh --example NAME` builds
the bundle, `scripts/ios/sim.sh` installs and launches it, and
`just ios-sim NAME` runs both from a herdr-gpui checkout; its `docs/ios.md`
describes them.

## Provenance

This crate was written for Herdr UI's iPadOS build, in the
[shin1ohno/herdr-gpui](https://github.com/shin1ohno/herdr-gpui) fork of
[herdr-gpui](https://github.com/penso/herdr-gpui), and extracted from that
fork's `crates/herdr-uikit` with its history (`git filter-repo
--subdirectory-filter`), so `git log` shows every change made there. Herdr UI
depends on it for iOS only.

Rewriting the history dropped the original commits' GPG signatures, so the
extracted commits are unsigned; the signed originals remain in the fork. The
merge commits keep the names of the branches they merged there, including
short-lived worktree branches.

`support/embed-resource` is a no-op stand-in for crates.io's `embed-resource`,
which only GPUI's Windows build script uses. Cargo resolves the lockfile for
every target, so patching it in keeps the real crate's Windows-only
dependencies out of this lockfile, as herdr-gpui does. `[patch]` applies only
in the root manifest, so consumers of this crate are unaffected.

## License

Apache License, Version 2.0; see `LICENSE`. The Metal renderer, the shader
source and `build.rs`, the Core Text text system, the GCD dispatcher, and
parts of the window and platform code are adapted from GPUI's macOS platform
crates (Copyright Zed Industries, Inc., Apache-2.0), and the font property
mappings from zed-font-kit (Apache-2.0); `NOTICE.md` lists what came from
where and what changed.
