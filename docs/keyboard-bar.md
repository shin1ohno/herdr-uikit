# Keyboard bar

`set_keyboard_bar` puts a consumer's buttons on the software keyboard's
shortcuts bar: the row above the keys, made of `UITextInputAssistantItem`'s
leading and trailing `UIBarButtonItemGroup`s on the window's text input view
(`GPUITextInputView`). `keyboard_bar_presses` streams the tapped item's `id`;
`keyboard_placement` says where the keyboard is. Added for yebisu#229, whose
terminal keys (ctrl, alt, esc, tab, symbols, arrows, ⌘) used to sit in an
in-app band.

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

So a consumer that needs its keys in every state draws them itself while
`keyboard_placement` is `Floating`. herdr-uikit does not install an
`inputAccessoryView` for it: switching one in and out needs
`reloadInputViews` on the first responder, which can disturb marked text.

## Behaviour

- The bar belongs to the responder. `TextInputView` keeps the consumer's bar
  and puts it on every replacement view (`finish_retirement`) before that
  view can become first responder.
- `set_keyboard_bar` with the bar already shown does nothing, so a consumer
  can call it every frame. An empty bar removes every consumer group.
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
  undocked keyboards.

## Tests

- `tests/keyboard_placement.rs`, `tests/keyboard_bar_queue.rs`: host tests of
  the classification, `guide_placement` and `combine` (the Simulator's
  frame and guide sequences) and the queue.
- `tests/keyboard_bar_uikit.rs`: the UIKit objects `apply` builds (groups,
  identifiers, the selected state, the menu, the empty bar), on the
  Simulator's main thread outside UIApplicationMain.
- Whether UIKit draws the bar in each state, and that a tap reaches the
  consumer, is yebisu's Simulator E2E (`scripts/ios/keyboard-bar-check.py`,
  which also reads `keyboard_placement_sample`) and device checks.
