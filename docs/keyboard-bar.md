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

## Tests

- `tests/keyboard_placement.rs`, `tests/keyboard_bar_queue.rs`: host tests of
  the classification (the Simulator's frames) and the queue.
- `tests/keyboard_bar_uikit.rs`: the UIKit objects `apply` builds (groups,
  identifiers, the selected state, the menu, the empty bar), on the
  Simulator's main thread outside UIApplicationMain.
- Whether UIKit draws the bar in each state, and that a tap reaches the
  consumer, is yebisu's Simulator E2E and device checks.
