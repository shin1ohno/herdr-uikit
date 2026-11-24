# Input mode indicator at the text input caret

A standard iPadOS text field shows the system's input mode indicator next to
its caret: "A" or "あ" when it gains focus with a hardware keyboard, and the
"A かな あ" switcher when the input source changes. GPUI's text input view
(`GPUITextInputView`) is an invisible `UITextInput` with no `UITextInteraction`,
so UIKit had no component to draw it. Since issue #139 each text input view
owns a `UITextSelectionDisplayInteraction` (iOS 17), which supplies the cursor
accessories, and the system draws the indicator at the caret the text view
already reports (`caretRectForPosition:`, the IME candidate window's rectangle).
The SDK draws nothing of its own.

## Who owns what

| Part | Where | Does |
|---|---|---|
| `SelectionDisplay` | `src/window/text_input/selection_display.rs` | One interaction per text input view, created with that view as `textInput` (fixed at creation) and a required, otherwise empty delegate. It is attached to the window's drawing view, not to the text view: the text view takes no touches (`pointInside:withEvent:` is false), and the indicator is tapped to switch the input source. The drawing view does not override hit testing |
| `TransparentCursorView` | `src/window/text_input/selection_display/cursor.rs` | The interaction's cursor view (`HerdrTransparentCursorView`): a `UITextCursorView` that draws nothing, with a clear background. The standard cursor view would draw a second, blue caret over GPUI's own cursor. `hidden` and `alpha` are left alone |
| `DisplayState` | `src/window/text_input/selection_display/state.rs` | When to switch the interaction on or off and when UIKit must re-read the caret. Std-only; `tests/selection_display_state.rs` runs it on the host |
| The consumer | for example yebisu | Calls `Window::invalidate_character_coordinates` when its caret moved without a selection change (a terminal's cursor moved by output), so GPUI reports it through `update_ime_position` |

## On and off

The interaction only reads the text view's first-responder state. Key routing,
`inputView`, the first-responder acquisition rules and the input delegate
notifications are unchanged.

| When | Where | What |
|---|---|---|
| The text view became first responder | `sync.rs`, `apply_mode` after `switch.acquire` | On, and re-read the selection |
| The window asked for text while the text view already was first responder | `sync.rs`, `apply_mode`, the `requested` branch | On (it stays on when it already was; it never goes through off) |
| The text view resigned (key-only mode, the keyboard's Hide, a sheet, the native search panel, an inactive scene) | `sync.rs`, `did_resign_first_responder` | Off |
| The caret moved | `sync.rs`, `set_caret` | `setNeedsSelectionUpdate`, only when the rectangle changed. It re-reads `selectedTextRange` and `caretRectForPosition:` and tells the input delegate nothing, so a composition in progress is neither committed nor dropped |
| Retirement replaced the text view | `retirement.rs`, `finish_retirement` | The old interaction is switched off and removed before the successor can acquire; the successor gets its own, switched on when it acquires |
| The window closed | `mod.rs`, `Drop for TextInputView` | Off and removed |

There is no API to show the indicator again. UIKit shows it when the input
source changes and, on a device with a hardware keyboard, when a field gains
focus. Switching the interaction off and on in one turn or across turns,
replacing it, or moving the caret did not show it again (issue #139, Design 1
c); resigning and re-acquiring first responder would bring the software
keyboard back and end a composition, so the SDK does not do that. A focused
view tapped again, or a pane switch that only moves the caret, shows nothing.

A first-responder change that UIKit makes on its own, with no window request
and no callback (for example UIKit restoring a responder after a system
alert), leaves the interaction off until the window next asks for text; the
fixture's `im5_requested_while_responder` case shows both states.

## Simulator QA

`herdr_uikit::input_mode_indicator_state(window)` (`cfg(target_abi = "sim")`,
not `test-support`, so a consumer's plain Simulator build can report it)
returns `InputModeIndicatorState`: whether the text view is first responder,
the interaction's `isActivated`, the number of selection display interactions
on the drawing view, whether the interaction's `textInput` is the current text
view, the cursor view's class, the first visible indicator view's frame
(UIKit's private `_UICursorAccessoryView`, read for QA only), the caret, and
how often UIKit was asked to re-read the selection. It never queries GPUI's
input handler, so it is safe inside a GPUI update: `caret_rect` is the caret
GPUI last reported, as `caretRectForPosition:` returns it when the handler
cannot answer (the fixture checks that UIKit's own query agrees).

Checks:

- Host: `cargo test --locked --test selection_display_state`;
  `python3 scripts/input_mode_mutations.py host`.
- Simulator unit contracts (outside `UIApplicationMain`):
  `src/window/text_input/tests/selection_display.rs` through
  `python3 scripts/test-simulator.py`; `python3 scripts/input_mode_mutations.py unit`.
- Fixture under `UIApplicationMain`, with real keys from XCUITest:

  ```sh
  IPHONEOS_DEPLOYMENT_TARGET=17.0 python3 scripts/check-native-search.py \
    --example input_mode_probe --test InputMode/testInputMode --prefix INPUTMODE \
    --output /abs/new/dir
  python3 scripts/input_mode_mutations.py uikit /abs/new/dir
  ```

  The script writes the Simulator's `AppleKeyboards` (`en_US` and
  `ja_JP-Romaji`) after boot. XCUITest's Shift puts the Simulator in the
  hardware keyboard state and Control-Space switches the input source. A
  standard text field is checked first under the same conditions; the cases
  that need the indicator to appear are judged only when it shows there.

## Unverified

- Whether the indicator shows when the terminal gains focus. The Simulator
  brings its software keyboard back when a field gains focus, which hides the
  indicator for standard fields too; this needs a device with a hardware
  keyboard (yebisu owner check D1).
- The occasional switch that shows nothing (the probe saw 2 of 15); the
  fixture dumps the view hierarchy for each miss.
- The software keyboard's globe key, range highlighting in the hardware
  keyboard state, and whether older iPadOS releases behave the same.
