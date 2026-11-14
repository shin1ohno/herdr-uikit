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
- **Menus.** GPUI's application menus in UIKit's main menu (the iPadOS menu
  bar and the keyboard shortcut overlay), with their shortcuts as key
  commands; see [Menus](#menus).
- **Layout direction.** `herdr_uikit::set_layout_direction` tells UIKit which
  way the app's text runs; see [Layout direction](#layout-direction).
- **Window controls.** Per-window insets for iPadOS 26's window controls;
  see [Window controls](#window-controls).
- **Everything else.** A GCD dispatcher, the pasteboard, thermal state, and
  idle-timer control.

The crate is empty on every target other than iOS: every dependency sits
under `cfg(target_os = "ios")`, so a cross-platform workspace can depend on it
unconditionally.

## Status

Experimental. It runs Herdr UI's iPadOS build in the simulator and on an iPad,
and pins `gpui-pre` (GPUI as published from Zed) to exactly `0.3.10`: the
renderer's shader ABI is generated from that snapshot's sources (`build.rs`),
so another GPUI version needs the port re-checked. The crate is not published;
depend on it by path or by git.

Not supported yet: Core Video paint surfaces (GPUI exposes them only on
macOS; they are skipped with a one-time warning), and anything GPUI's
`Platform` trait asks of a desktop that iPadOS has no counterpart for. The
menus' keyboard precedence rests on UIKit behavior still to be confirmed on a
device ([Menus](#menus)).

## Menus

`Platform::set_menus` keeps GPUI's menus and asks UIKit to rebuild its main
menu. The application delegate, a `UIResponder`, adds them in
`buildMenuWithBuilder:`: GPUI's first menu goes into the application menu,
menus titled File, Edit, View, Window, or Help (in English, or as UIKit
titles its own menu) join UIKit's menu of that name, and the rest stand
beside them. Separators become inline groups. Cut, Copy, Paste, and Select
All items with an `OsAction` are UIKit's own Edit commands, which reach the
text input view's `cut:`, `copy:`, `paste:`, and `selectAll:`; a second item
with the same `OsAction` becomes an ordinary command, since UIKit refuses two
identical commands. Every other item is one command whose selector,
`herdrMenuAction:`, the focused window's drawing view performs, or the
application delegate when no window has focus. `validateCommand:` asks GPUI
whether the action is available and shows `checked` items with a check mark;
with no GPUI callback registered a command is disabled, as on macOS, and
while GPUI itself is running (its app is borrowed) a command keeps what it
showed last instead of asking.

UIKit's own application menu items (About, Settings, Services, Hide, Quit, as
iPadOS provides them) stay, since the system performs them. GPUI's Undo and
Redo replace UIKit's when GPUI's menus have them. An app should leave out of
its iOS menus any item whose platform call does nothing on iPadOS (`quit`,
`hide`, `hide_other_apps`): it would look usable and do nothing. Herdr UI
already does so for Hide; its Quit item still needs the same `cfg`.

Shortcuts come from the keymap as macOS GPUI chooses them: the first
single-keystroke binding whose context matches an editor in a pane in a
workspace, else the first. Only shortcuts with command or control become key
commands, and none that UIKit's own menus already use; the others show no
shortcut and their keys behave exactly as without menus.

A key press keeps going through GPUI's keymap, so context-specific bindings
and keys the daemon defines are never bypassed by a menu item's action. When
a key command's press performs the command, the window hands the shortcut's
keystroke to GPUI as the press would have reported it (enter, tab, and space
with their character, `cmd-shift-/` as `cmd-?`), and runs the item's action
only if GPUI declines it, as AppKit's key equivalents do on macOS. A command
chosen with the pointer, by touch, or with the keys of an open menu runs the
action.

Which of the two happened is read from the event UIKit is delivering: the
application class (`HerdrApplication`, passed to `UIApplicationMain`)
records it around `sendEvent:`. A key event whose modifiers match the
shortcut's is the press; any other event is a pick from the menu. With no
event being delivered the keystroke goes to GPUI, which is harmless for a
pick and keeps the keymap in charge otherwise. No modifier state a view kept
is consulted, so a modifier held down before the app had focus, or a release
lost while it was away, cannot turn a key press into a menu pick.

This relies on UIKit behavior that is not documented and has to be checked on
an iPad with a hardware keyboard:

1. UIKit matches a main-menu key command before it calls the first
   responder's `pressesBegan:withEvent:`. If it delivers the press first
   instead, nothing is offered twice: a key command for a press GPUI already
   handled in the same key event does nothing more, and one for a press GPUI
   declined runs the action without offering the keystroke again.
2. UIKit performs a key command synchronously inside `sendEvent:` for its
   key event, and a pick from the menu during a touch or pointer event or on
   its own schedule. Check it with the modifiers held down before switching
   to the app (Cmd-Tab into it with Cmd still held, then press Cmd-W): the
   press must still reach GPUI's keymap. Holding the modifiers while tapping
   a menu item runs the action.
3. A key command that `validateCommand:` marked disabled lets its press go on
   to the responder chain instead of swallowing it, so a binding for the same
   keystroke in a context where the menu's action is unavailable still reaches
   GPUI. If UIKit swallows it, commands have to stay enabled for key matching
   and be disabled only for display.
4. Whether UIKit repeats a held key command, and whether it sends
   `pressesEnded:` for a press its key command took. The view sends a repeat
   to GPUI as a held key and runs no repeat timer of its own, so a lost
   release cannot repeat forever; GPUI hears a key's release only if UIKit
   sends it.
5. UIKit validates commands from its event loop or from inside a call GPUI
   makes; the latter is detected and skipped. Check it by changing focus with
   a shortcut while the menu bar is visible.
6. Which application menu items iPadOS shows by default (Hide, Quit), and
   that they work.

iPadOS reports nothing when a menu opens, so GPUI's `on_will_open_app_menu`
callback is kept but never called (`validateCommand:` also runs while UIKit
matches key commands, where clearing GPUI's pending keystrokes would break
multi-keystroke bindings). iPadOS has no Dock menu.

## Native edit actions

UIKit's `copy:`, `cut:`, `paste:`, and `selectAll:` still dispatch the same
Command-key down/up events. Inside your synchronous GPUI input callback,
`herdr_uikit::native_edit_action()` returns `Some(NativeEditAction::Copy)`,
`Cut`, `Paste`, or `SelectAll` for those commands, and `None` for ordinary
physical key events, including an identical hardware Command-V. A terminal
can use `Some(NativeEditAction::Paste)` to choose its clipboard path while
keeping its hardware Command-to-Control mapping.

The thread-local origin covers only the synthesized key-down and key-up;
it restores the previous value on return or panic unwinding, including
nested commands. Read it in the callback, not in a deferred task. Modifier
restoration and the unhandled-paste fallback run with the previous origin.
Consumers that do not read it keep their existing behavior.

## Layout direction

GPUI lays out every pixel itself, so UIKit cannot tell which way the app's
text runs. On the main thread, call
`herdr_uikit::set_layout_direction(LayoutDirection::RightToLeft)` (or
`LeftToRight`, the default) when the app's language changes; it fails with
`Error::NotMainThread` elsewhere. UIKit then reports that base writing
direction to the keyboard and IME, lays out its own UI around the windows
(`semanticContentAttribute`), and the text input maps visual left and right
onto the text's logical order. The main menu is rebuilt, which refreshes its
commands; the menu bar's own direction follows the app's localization.
Terminal rendering and text shaping are unaffected.

The left/right mapping assumes the focused text is drawn in the app's
direction. Latin text or a terminal in a right-to-left app is still drawn
left to right, so there a caret UIKit moves left goes towards the end, not
visually left, and every position reports a right-to-left base direction:
GPUI's input handler reports no per-range direction to do better with. To
check on a device in a right-to-left app: move the caret with the space-bar
trackpad and through an IME over Latin text and over right-to-left text, and
note which way it goes, including whether UIKit's string tokenizer swaps left
and right a second time.

## Window controls

On the main thread, call
`herdr_uikit::window_control_insets(window.window_handle())` to read how far
iPadOS 26's window controls extend into that window past its plain safe area.
It returns `gpui::Edges<gpui::Pixels>` in points, using UIKit's physical edges:
the drawing view's safe area with horizontal corner adaptation, less
`safeAreaInsets`, clamped to zero per edge.

With the controls showing, the value covers them: on the iPadOS 26.5
simulator a narrow windowed window reports 66 pt on the left and 9.5 pt on
the right. Without them UIKit still reports the window's corner allowance,
5.5-9.5 pt, so the value is not zero on iOS 26 even when no controls show. It
is zero on iOS 17-25, for a window in no live scene, and off the main thread.
UIKit layout callbacks sample and cache it; GPUI redraws the window when it
changes. Herdr UI should use it to pad only top-bar content so its leading
button stays clear of the controls.

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

The tests run inside the iPad simulator. `--all-features` enables the
`test-support` feature, including GPUI input-handler composition regressions. `scripts/sim-runner.sh` is a Cargo
runner that boots the simulator when it is not running, never shuts it down,
and runs each test binary there with `xcrun simctl spawn`:

```sh
export IPHONEOS_DEPLOYMENT_TARGET=17.0
CARGO_TARGET_AARCH64_APPLE_IOS_SIM_RUNNER=scripts/sim-runner.sh \
  cargo test --locked --all-features --target aarch64-apple-ios-sim
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

### Explicit native edit availability

`set_native_edit_availability(window, Some(NativeEditAvailability { ... }))`
publishes a per-window policy for consumers whose visible selection is not their
UITextInput document. Publish it after focus/selection changes, including all-false
while a terminal is blocked, and clear with `None` when leaving that consumer.
`None` preserves the previous text-mode/text-focus policy for ordinary clients.
The SDK does not retain an application callback or borrow GPUI during validation.

Both text and drawing responders use the same decision for
`canPerformAction:withSender:`, `validateCommand:` and standard edit execution.
Explicit availability requires an active window owning a key responder. Commands
retain the existing `native_edit_action()` scoped dispatch and paste fallback.
The consumer must recheck its live destination/selection when executing a command:
a menu can remain visible while application state changes. This also supports a
future long-press menu using the standard UIKit selectors without a second policy.

SH1-153 validation: unit tests cover both modes, inactive/stale policy, defaults,
and both responder method tables. The yebisu Simulator harness invokes actual
UIKit selectors and checks clipboard/wire receipts. Physical hardware validation
and menu screenshots are separate gates, not established by the unit tests.

## Explicit native edit menus

Consumers can call `present_native_edit_menu(window, point)` after publishing
`NativeEditAvailability` and focusing their content. It returns an opaque,
process-unique `NativeEditMenuToken`, or `None` off the main thread, for an
unknown/closed window, absent availability, inactive scene, or absent key
responder. UIKit presentation is deferred out of the GPUI update and checks
that the token is still current before displaying.

Read `native_edit_menu_token()` alongside `native_edit_action()` during native
key-down/key-up dispatch. `dismiss_native_edit_menu(window, token)` only closes
the matching current menu. `native_edit_menu_status(window, token)` reports
`Current`, `Revoked(reason)`, or `Unknown`; consumers should discard tickets
that cease to be current and independently check their target and selection.
No menu origin survives outside synchronous key dispatch, including fallback,
physical-key delivery, and nested dispatch restoration.

Revocation is irreversible: replacement, explicit dismissal, availability
becoming absent, close, scene inactivity, external responder resignation, and
an internal responder switch that fails acquisition or ends without either
responder. A same-flush successful keyboard restart retains ownership. All
internal native responder calls go through `ResponderSwitch`; only close's
already-revoked cleanup calls UIKit directly.

Owned configuration identifiers have prefix `EDIT_MENU_IDENTIFIER_PREFIX`.
The delegate returns token-tagged Copy/Paste only for the current token, and
an explicit empty `UIMenu` for every invalid owned configuration. It forwards
`suggestedActions` only for non-owned identifiers. Standard Edit commands
without `EDIT_MENU_PROPERTY_KEY`, nil senders and other senders retain their
previous untagged behavior. A present but malformed/stale/foreign token is
rejected before dispatch and before clipboard fallback.

The P1/P4 preflight on iOS 26.5 (Xcode 26.6) observed UICommand senders retaining
the token dictionary, exactly Copy/Paste for populated menus, and zero items
for empty menus. Therefore the UIAction and disabled-item alternatives are
not selected. System Edit was not exposed to that XCTest run; its physical
interaction remains device QA, separately from untagged-command contracts.

Additional validation:

```sh
bash scripts/check-responder-calls.sh
python3 scripts/test-simulator.py
```

The responder checker verifies the known five-call baseline before requiring
only the close cleanup outside the guard. The Simulator runner creates and
deletes its own iPad. `tests/edit_menu_uikit.rs` uses a harness-free main thread
for actual UIKit delegate/command objects; ordinary Rust test threads exercise
the same production state and guard with controlled ownership/acquisition.
These checks do not replace the consumer's live scene/responder E2E or device QA.
