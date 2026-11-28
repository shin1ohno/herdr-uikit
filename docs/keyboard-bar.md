# Keyboard bar and accessory row

Two places for a consumer's keys over the software keyboard:

- `set_keyboard_bar` puts buttons on the keyboard's shortcuts bar: the row
  above the keys, made of `UITextInputAssistantItem`'s leading and trailing
  `UIBarButtonItemGroup`s on the window's text input view
  (`GPUITextInputView`). Added for yebisu#229.
- `set_keyboard_accessory` puts a full-width row of equal-width keys in the
  text input view's `inputAccessoryView`: a keyboard-style `UIInputView`
  (`window/keyboard_accessory.rs`). Added for yebisu#276, whose owner chose
  layout D: the row normally, the shortcuts bar narrowed to seven items on a
  docked keyboard in a short window.

`keyboard_bar_presses` streams the tapped item's `id` from both;
`keyboard_placement` says where the keyboard is. An item with `arrows` is one
flick arrow key on either (below).

## What UIKit shows, by keyboard state

The SDK headers say nothing about keyboard states (`UIResponder.h` on
`inputAssistantItem` and `inputAccessoryView`, `UITextInput.h` on
`UITextInputAssistantItem`, Xcode 27.2). Measured on the iPadOS 27.2
Simulator, iPad Pro 11-inch (M5), with a spike app (yebisu#229 task record):

| Keyboard | Shortcuts bar items | `inputAccessoryView` |
| --- | --- | --- |
| Docked | Shown and tappable | Shown above the bar |
| Floating | Not shown; only the predictive row | Shown full-width at the screen's bottom edge |
| Split | Not producible from XCUITest (no hide-key menu, pinch does not split); a device check | Not measured |
| Hardware keyboard only | Not measured in the Simulator | Not measured |

A responder that adopts only `UIKeyInput` gets no shortcuts bar at all;
`GPUITextInputView` adopts `UITextInput`, so it gets one.

So the shortcuts bar alone cannot hold a consumer's keys in every state: a
floating keyboard has none, and a split one, or a Japanese keyboard whose
candidates take the bar's middle, leaves the bar 124-239 pt (yebisu#276), so
UIKit narrows 17 items to 8.5-15 pt each instead of dropping any. The
accessory row shows with every software keyboard, in its own row.

Adding or removing the row needs `reloadInputViews` on the first responder,
which can disturb marked text. So `set_keyboard_accessory` reloads only when
the row is added or removed (a row whose keys change updates them in place),
from the run-loop flush rather than inside GPUI's frame, and never while text
is being composed: the reload waits for a flush after the composition ends
(`text_input/sync.rs` `apply_accessory`). A view that is not first responder
reads the row when it becomes first responder.

## Behaviour

- The bar and the row belong to the responder. `TextInputView` keeps the
  consumer's bar and row and puts them on every replacement view
  (`finish_retirement`) before that view can become first responder.
- `set_keyboard_bar` and `set_keyboard_accessory` with what is already shown
  do nothing, so a consumer can call them every frame. An empty bar removes
  every group, the system's undo, redo and paste included (the first call is
  always applied, an empty bar too): with English the keyboard then drops
  its top row, 36-41 pt (yebisu#276; Japanese keeps its candidates' row). An
  empty row removes the accessory view. A bar or a
  row that differs only in `selected` (a latch) updates the buttons in place,
  so a touch on another key, a held arrow key's repeat, is not cut off.
- The row's keys are gray `UIButton`s (filled while `selected`), a `menu` key
  opens its menu on tap, laid out equally wide with 6 pt between them, 48 pt
  tall. Its accessibility identifier is `herdr-keyboard-accessory`.
- The arrow key (`KeyboardBarItem::arrows`, `keyboard_bar/arrow.rs`, after
  Blink Shell's behaviour; no code taken) is a `UIControl` that tracks one
  touch: the arrow is the side of the key's centre the finger is on (the
  key's diagonals divide the sides) when it lands and while it moves; a
  dead zone at the centre (20 % of the width and 15 % of the height either
  way) means none. The arrow is sent when the finger lifts; held 0.5 s it is
  sent and repeats every 0.1 s, 0.07 s once the finger is beyond the key's
  edge and 0.04 s three half-sizes from the centre, and lifting then sends
  nothing more. On the shortcuts bar it is a `UIBarButtonItem`'s custom view,
  52 x 36 pt.
- A button's `UIAction` only queues its `id` (`keyboard_bar/queue.rs`, at
  most 64 unread presses); nothing calls GPUI from UIKit's callback. The
  stream ends after the window closes.
- An item with a `menu` opens it on tap; each entry sends its own `id`.
- `keyboard_placement` classifies the end frame of each relevant
  `UIKeyboardWillChangeFrameNotification` in screen points
  (`window/keyboard_placement.rs`): full width on the bottom edge is
  `Docked`, full width elsewhere `Undocked`, narrower `Floating`, empty or
  off screen `Hidden`. A change draws a frame, because a floating keyboard
  that shows or hides leaves the bottom inset at 0.
- The frame alone is not enough. On the iPad Pro 13-inch (M5) Simulator,
  after the floating frame UIKit posts one more Will/Did pair whose end frame
  is `{{0, 1376}, {1032, 0}}` while the floating keyboard stays up, and the
  floating frame's own notification can carry another `UIScreen` object, so
  the window does not take it as its own (yebisu's run: 5 notifications, 3
  taken). So the drawing view's own `keyboardLayoutGuide` follows undocked
  keyboards (`followsUndockedKeyboard`); a probe subview's guide did not
  follow them. `guide_placement` classifies the guide at each layout pass;
  a guide no taller than the bottom safe area is `Hidden`. `combine` reports
  the notification's placement, except that `Hidden` defers to the guide.
  The next foreground-executor turn after a frame notification lays the
  view out again, so a lagging guide is read again.
- The bottom inset keeps its policy: a floating or undocked guide makes no
  inset beyond the bottom safe area, as the guide gave before it followed
  undocked keyboards, with one exception below.
- A full-width `Undocked` keyboard whose bottom edge is at most
  `NEAR_BOTTOM_GAP` = 32 pt above the screen's (`window/keyboard_inset.rs`
  `near_bottom`) takes a bottom inset from its top edge to the view's bottom
  edge, from both the notification and the guide; its placement stays
  `Undocked`. On the iPad mini (A20 Pro) Simulator, iPadOS 27.2, 756 x 1158
  pt (yebisu#277), a split keyboard ends at (0, 811, 756, 336) and one just
  unsplit at (0, 807, 756, 340): 11 pt above the bottom edge, so they take
  347 and 351 pt, and the window's last lines and prompt stay above them.
  One moved up ends 260 pt above it at (0, 558, 756, 340) and takes none,
  as before, so dragging an undocked keyboard does not resize the window.
  32 pt is about three times the measured gap, for a device whose gap is a
  little larger; such a keyboard leaves at most 32 pt unused below it.
  Docked (0, 818, 756, 340: 340 pt) and floating keyboards are unchanged.
- While split, the Simulator's notification moved the keyboard below the
  screen ((0, 1158, 756, 336)) about once a second, and the keyboard's own
  frame came back 0.25-0.27 s later. After a near-bottom keyboard, such a
  frame starts its inset animation `OFF_SCREEN_HOLD_SECS` = 0.5 s late
  (`holds_inset`); the next notification replaces the waiting animation, so
  the inset does not drop and come back. A real hide reaches the window
  0.5 s later than a docked keyboard's. The guide's sample waits as well,
  as it does for any animation.
- The gap and the off-screen frames are Simulator measurements, with the
  split forced through a private API (`setSplit:YES animated:YES`); a
  device's split keyboard frame is unmeasured.
- The accessory row takes a bottom inset of its own where it is the window's
  last thing on the screen (`keyboard_inset::accessory_inset`, from the row's
  frame at each layout pass): full width on the screen's bottom edge (below a
  floating keyboard, where UIKit puts it), or on top of a keyboard that takes
  an inset. The inset runs from the row's top edge, so the window's last lines
  and status bar end above the row. A row above a keyboard moved up takes
  none, as that keyboard does.

## Tests

- `tests/keyboard_arrow.rs`: host tests of the arrow key's touch logic: the
  arrow of each side, the dead zone, sending on release, the hold that starts
  repeating and the faster repeats farther out.
- `tests/keyboard_inset.rs`: host tests of `near_bottom`,
  `near_bottom_inset`, `holds_inset` and `accessory_inset` with the iPad mini
  frames above;
  `layout/keyboard_geometry/tests.rs` checks the notification's inset for
  the same frames in the Simulator test run.

- `tests/keyboard_placement.rs`, `tests/keyboard_bar_queue.rs`: host tests of
  the classification, `guide_placement` and `combine` (the Simulator's
  frame and guide sequences) and the queue.
- `tests/keyboard_bar_uikit.rs`: the UIKit objects `apply` builds (groups,
  identifiers, the selected state, the menu, the empty bar, the arrow key as
  a custom view, the in-place selected update) and the accessory row's keys
  (kinds, identifiers, equal widths inside the row, the in-place update), on
  the Simulator's main thread outside UIApplicationMain.
- Whether UIKit draws the bar and the row in each state, that a tap or an
  arrow key's flick reaches the consumer, and that marked text survives the
  row's keys, are yebisu's Simulator E2E (`scripts/ios/keyboard-bar-check.py`,
  which also reads `keyboard_placement_sample`) and device checks.
