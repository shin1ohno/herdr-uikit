# Simulator input receipts

A consumer's Simulator QA synthesizes touches, trackpad clicks and menu picks
through XCTest. When the expected result does not appear, a timeout only says
that it has not appeared yet. The SDK therefore records where each input
arrives, so the QA can tell an input that never reached the app from one that
reached it and stopped further in.

The receipts exist only in builds for the iOS Simulator
(`cfg(all(target_os = "ios", target_abi = "sim"))`). The module
`src/platform/qa_input.rs` and every statement that calls it carry that
`cfg`, so a device build compiles none of it and behaves as before; the test
`every_receipt_is_compiled_for_the_simulator_only` fails if a call loses it.
No public API is added and no input is changed, delayed or retried.

## How a consumer reads them

Each receipt is one `tracing` event at level INFO with the target
`herdr_uikit::qa_input`. A consumer's QA build adds a subscriber layer for
that target and writes the events next to its own observations (yebisu:
its diagnostics `Capture` and `app.jsonl`). Nothing is written when no
subscriber listens.

## Receipts

Every event has a `receipt` field.

| `receipt` | Emitted where | Fields |
|---|---|---|
| `touch` | `Window::handle_touches` (`src/window/touch.rs`), for each touch the drawing view's `touchesBegan`/`Ended`/`Cancelled` callback passes, before it becomes a GPUI event. Moves are not recorded. | `stage` (`began`, `ended`, `cancelled`), `kind` (`direct` for a finger or Pencil, `pointer` for a trackpad or mouse click), `x` and `y` in the window's points, `touches` (touches in that callback) |
| `menu-perform` | `Performed::new` (`src/platform/menus.rs`), which both `herdrMenuAction:` implementations (drawing view and application delegate) call first | `outcome`: `no_sender`, `no_tag` (the sender is not one of the SDK's commands) or `performed`; for `performed`: `generation` and `index` of the command's tag and `keyed` (a key command with a key input performed it). Always `current`, the generation of the menus as last set, `-1` when the platform is not running |
| `menu-deliver` | `deliver` (`src/platform/menus.rs`), on the main queue after UIKit's call returned | `outcome`: `stale_generation` (the tag names no command of the current menus), `no_callback` (GPUI registered no menu callback), `callback_running` (GPUI's callback was already running further up the stack) or `delivered`; `generation`, `index`, `current` |
| `frame` | `Renderer::draw` (`src/renderer/mod.rs`), when a window's draw comes to a different outcome than its draw before (each draw would flood the record; a steady run of one outcome stays quiet) | `outcome`: `presented`, `zero_size` (the layout gave the layer no size yet), `paths_not_ready` (the scene has paths and the path textures are not allocated), `no_drawable` (the layer gave no drawable before its timeout) or `render_failed` (encoding the frame failed); `previous`, the outcome of the run it ends (`none` for a window's first draw); `previous_count`, how many draws that run had |
| `edit-menu-present` | the deferred present in `edit_menu::present` (`src/window/edit_menu.rs`), on the main queue after GPUI's update | `outcome`: `presented` (UIKit's `presentEditMenuWithConfiguration` was called) or `skipped` (the token was no longer current); `token`; when presented, `active` (the scene was active) and `responder` (`drawing`, `text` or `none`, the window's first responder); when skipped, `reason` (`replaced`, `dismissed`, `unavailable`, `closed`, `inactive`, `resigned_outside`, `switch_left_no_responder`, `unknown`) |
| `edit-menu-menu` | `EditMenuDelegate::menu_for` (`src/window/edit_menu/delegate.rs`), when UIKit asks for the menu of a configuration | `decision` (`items`, `empty` or `passthrough`) and `owned` (the configuration carried the SDK's identifier) |

## Reading a lost menu command

| Receipts after the pick | Where the command stopped |
|---|---|
| no `menu-perform` | before the app: the system did not perform the command (or the touch that should choose it never reached the menu) |
| `menu-perform` `performed`, `keyed=true`, no `menu-deliver` | handled as a key shortcut by the text input view, as designed |
| `menu-perform` `performed`, `keyed=false`, no `menu-deliver` | dropped in the drawing view before dispatch (for example, a branch that refuses the command) or the main-queue block never ran |
| `menu-deliver` other than `delivered` | in the SDK: the outcome names the reason |
| `menu-deliver` `delivered`, but the consumer saw no action | in GPUI or the consumer: GPUI's `App::dispatch_action` reached no handler (for example, with no active window it dispatches to global listeners only) |

While a native search panel's gate is shut in the window the command was
chosen in, or the key that chose it is fenced ([native-search.md](native-search.md)),
the command stops after `menu-perform` (keyed or not) or on the main queue
before `deliver`, and leaves no `menu-deliver`.

## Reading an edit menu that did not appear

| Receipts after the consumer presented | Where the menu stopped |
|---|---|
| no `edit-menu-present` | the deferred present never ran (the window closed, or the main queue did not run it) |
| `edit-menu-present` `skipped` | the SDK revoked the token before UIKit was asked; `reason` names why |
| `edit-menu-present` `presented`, no `edit-menu-menu` | UIKit was asked to present and never asked the delegate for a menu |
| `edit-menu-menu` `empty` | the delegate answered with no items (the token was no longer current when UIKit asked) |
| `edit-menu-menu` `items` and no menu on screen | UIKit had the items and showed nothing, or closed it at once |

## Reading a window that shows nothing

A window with no `frame` receipt at all has not drawn since it opened: GPUI
asked for no frame. A last receipt other than `presented` names why its draws
stop short of the screen. A last receipt `presented` with a black window means
the frame left the renderer, so the cause is after it (the layer or the system
compositor).

## Tests

- `src/platform/qa_input/tests.rs`: each receipt's fields, that other targets
  are ignored, that `handle_touches` records each touch before dispatching it,
  that `FrameRuns` reports only a change of outcome with the run it ended,
  that `Renderer::draw` notes the outcome of every draw, that the edit menu
  receipts sit before a skip returns, after UIKit is asked to present, and
  before the delegate builds its menu, and that every call
  into the module is Simulator-only.
- `src/platform/menus/tests/commands.rs`: real `UICommand`s and
  `UIKeyCommand`s through `Performed::new`, and `deliver` with a current, a
  stale, a missing and a running callback, each leaving its receipt.

These run in the Simulator through `scripts/test-simulator.py`, which the CI
job "macOS SDK checks" runs.
