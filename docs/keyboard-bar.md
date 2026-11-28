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
| Split | Shown, but every group squeezed over the left half (yebisu#276, below) | Not measured |
| Hardware keyboard only | Not measured in the Simulator | Not measured |

A responder that adopts only `UIKeyInput` gets no shortcuts bar at all;
`GPUITextInputView` adopts `UITextInput`, so it gets one.

So a consumer that needs its keys in every state draws them itself while
`keyboard_placement` is `Floating`, or while `keyboard_bar_split` is true.
herdr-uikit does not install an `inputAccessoryView` for it: switching one
in and out needs `reloadInputViews` on the first responder, which can
disturb marked text.

## Split keyboards (yebisu#276)

XCUITest cannot split the keyboard (no hide-key menu, pinches and
two-finger spreads do not split). A Simulator-only call to the private
`UIKeyboardImpl.activeInstance` `setSplit:animated:` does, on the iPad mini
(A20 Pro) Simulator; on the iPad Pro 11-inch (M5) and iPad Air 11-inch (M4)
Simulators the same call leaves the keyboard docked. yebisu forces the split
that way in Simulator builds only; nothing here calls private API.

UIKit then puts the leading and the trailing groups over the left half
(about 239 points on the 756-point iPad mini), shrinking items to 10-18
points so their symbols overlap. The keyboard's frame does not tell the
split apart: unanimated, the frame notification stays docked (full width
on the bottom edge, 336 points tall against 340); animated, as on a device,
it is `Undocked` (0, 811, 756, 336), and merging again stays `Undocked` at
(0, 807, 756, 340). The keyboard layout guide and its near-edge
constraints (`setConstraints:activeWhenNearEdge:`) report the same for
split and unsplit. A split posts `UIKeyboardWillHide`/`DidHide` with an
on-screen frame, but so does a float.

What does tell: where UIKit draws the trailing groups. `apply` starts them
with a sentinel item, a 1-point transparent custom view that takes no
touches and is hidden from accessibility. Its frame (keyboard window
coordinates plus that window's origin; converting to the screen's
coordinate space from the keyboard window gave a null rectangle) is on
the right half of the screen for a docked or undocked keyboard and on the
left half for a split one, also with no consumer items:

| iPad mini (A20 Pro) portrait | Sentinel x (points) |
| --- | --- |
| Docked or undocked, with yebisu's keys | 449-451 (the arrows start at 476) |
| Docked or undocked, no keys | 727 |
| Split, with yebisu's keys | 142-146 |
| Split, no keys | 238 |
| Shrinking into a floating keyboard | 377.1 for 0.7 s (on the middle) |
| Floating or hidden | In no window |

`keyboard_placement::bar_split` takes a sentinel ending left of the
screen's middle as split; `split_after` keeps the last answer while the
sentinel is briefly out of its window (the bar's groups were replaced) and
never reports a floating keyboard split. The sentinel is read at each
layout pass, and 0.1, 0.3, 0.7 and 1.5 s after each keyboard frame
notification and each change of the bar, because UIKit lays the bar out
with no callback this view sees (it moved within 0.5 s of the split). A
change draws a frame.

At the end of the trailing groups the sentinel would push every trailing
button 24 points left (UIKit spaces trailing items 46 points apart, packed
against the right end); in front of them it moves none while the trailing
half has room, 98 points on the iPad mini in portrait.

Not measured: a real device's split (its frames, and whether UIKit draws the
bar there as on the Simulator), landscape, and Stage Manager windows.

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
  the classification, `guide_placement`, `combine`, `bar_split` and
  `split_after` (the Simulator's frame, guide and sentinel sequences) and
  the queue.
- `tests/keyboard_bar_uikit.rs`: the UIKit objects `apply` builds (groups,
  identifiers, the selected state, the menu, the sentinel and its view, the
  empty bar with and without the sentinel), on the Simulator's main thread
  outside UIApplicationMain.
- Whether UIKit draws the bar in each state, that a tap reaches the
  consumer, and that a forced split moves yebisu's keys to its band, is
  yebisu's Simulator E2E (`scripts/ios/keyboard-bar-check.py`, which also
  reads `keyboard_placement_sample`) and device checks.
