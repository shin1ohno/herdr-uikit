# Held-press injection (test-support, iOS Simulator)

`herdr_uikit::retirement_probe::held_press` sends one keyed `UIPress` through
the text view's actual press selectors and keeps it held across real time, so
a Simulator test can drive the press ledger, the text view's key state and
the SDK's own key-repeat timer the way a held hardware key does. Consumers use
it to check what reaches their views while a key is held across a text-view
retirement (yebisu's P13 closing-Escape tests).

## Gate

Every item is compiled only under

```rust
#[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
```

- `src/window/retirement_probe.rs` declares `pub mod held_press` under it.
- `src/window/text_input/probe.rs` gives `NativeClient::view` and
  `NativeClient::live` (`pub(crate)`) under it.
- `examples/held_press_probe.rs` puts its whole body under it; there is no
  `[[example]]` entry in `Cargo.toml`.

A device build, or any build without `test-support`, contains none of it.
`tests/held_press_script.rs` is a host test target of this package only and
includes the std-only phase script by path.

## API

| Item | Purpose |
|---|---|
| `HeldProbeKeySpec { usage, characters }`, `HeldProbeKeySpec::ESCAPE` | The key the press reports (Escape: HID usage 41, `"\u{1b}"`, no modifiers). |
| `held_press(spec) -> Option<HeldProbeHandle>` | A new press object (`HerdrHeldProbePress`, a `UIPress` subclass carrying a `HerdrHeldProbeKey`, a `UIKey` subclass). `None` off the main thread. |
| `HeldProbePhase { Began, Changed, Ended, Cancelled }` | The selector (`pressesBegan:withEvent:` and so on) and the raw `UIPressPhase` the press reports. |
| `HeldProbeTarget::{Current, Retired(&NativeClient)}` | The window's text view now, or a view captured with `retirement_probe::capture` that has since retired. |
| `inject_held_press(window, &handle, phase, target) -> Result<(), HeldProbeError>` | Checks the phase script, sets the press's phase and sends the selector with a nil event. Returns after the view's selector body ran. |
| `HeldProbeHandle::script()` | The phases sent so far. |

`HerdrHeldProbePress` reports `type` −1 (no TV-remote or arrow handling), like
the existing `HerdrProbePress`.

## Phase script

Checked before anything is sent; a refused step sends nothing.

1. `Began` and `Changed` go only to the current text view.
2. A view sees a press begin at most once. A second `Began` to a new current
   view models UIKit delivering the held press again after a retirement.
3. `Changed`, `Ended` and `Cancelled` go only to a view that saw this press
   begin. A retired view qualifies when it saw `Began` before it retired.
4. `Ended` or `Cancelled` finishes the press.

A `Retired` target that is still the window's text view, or still live, is
refused with `TargetNotRetired`.

## What a send does in the SDK

- Every selector records the press in the ledger first (`view_class.rs`).
- `Began` with Escape: GPUI receives `KeyDown { is_held: false }`; if the GPUI
  view handles it, the SDK repeats it as `KeyDown { is_held: true }` after
  0.5 s and then every 0.033 s (`keys.rs`).
- `Changed` is only recorded and goes on to UIKit (`super`).
- `Ended` and `Cancelled` on the current view send `KeyUp` and stop the repeat.
- On a retired view they clear the ledger entry but send no `KeyUp`, and the
  new view's repeat keeps running (`keys.rs`, `presses_ended`).

## What this does not show

The press never passes through UIKit's press tracking, so UIKit neither
cancels nor re-delivers it. The order of phases and views is the caller's
script. A passing test means "if UIKit delivers presses in this order, the
consumer behaves so"; it is not evidence that iPadOS delivers that order.
Key commands that UIKit matches before `pressesBegan:` are not exercised.

## Fixture and negative controls

`examples/held_press_probe.rs` runs under `UIApplicationMain` on a Simulator
and asserts, in order: Began (one `KeyDown`, one ledger entry), the SDK repeat
(at least half the nominal count), Ended (one `KeyUp`, ledger and key state
clear, repeat stopped), Changed (ledger entry and its age kept, no `KeyUp`),
Cancelled (one `KeyUp`, repeat stopped), script refusals, and the retired
release (re-delivered `Began` on the new view, `Ended` to the retired view:
no `KeyUp`, ledger clear). It records, without asserting, the repeats that
continue on the new view after the retired release.

`scripts/check-public-held-press.py` runs it on a leased Mac with an exclusive
Simulator (environment in the script header), then rebuilds with each
mutation and requires the named assertion to fail:

| Mutation | Required assertion |
|---|---|
| `Began` sent through `pressesChanged:` | `began-keydown` |
| `REPEAT_DELAY` 0.5 → 5.0 s | `began-repeats` |
| `Changed` sent through `pressesEnded:` | `changed-keeps-ledger` |
| `Cancelled` sent through `pressesChanged:` | `cancelled-keyup` |
| `Ended` sent through `pressesChanged:` | `ended-keyup` |
| `Retired` target resolved to the current view | `retired-release-no-keyup` |

Each mutation is restored byte for byte, and the restored tree must hash the
same as the baseline before the final run. Hosted CI compiles and lints the
module and the example for both iOS targets and runs the host tests; it does
not run the fixture, because its Simulator runner has no `UIApplicationMain`
(`scripts/sim-runner.sh`).
