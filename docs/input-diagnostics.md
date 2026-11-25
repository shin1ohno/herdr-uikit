# Input diagnostics and hardware typing recovery

On an iPad app running on a Mac ("Designed for iPad"), typed characters were
lost while the drawing view (`HerdrUIKitGPUIView`) held first responder
instead of the text input view (`GPUITextInputView`): UIKit's keyboard only
turns presses into `insertText:` for its first responder, so a printable press
the drawing view forwards goes nowhere. Only a click on the text recovered it.

## Static codes

`herdr_uikit::DIAGNOSTIC_TARGET` (`herdr_uikit::diagnostics`) carries one
`code` field per event, always one of `herdr_uikit::DIAGNOSTIC_CODES`. A code
is an enum value; no key, character, text, host or terminal content is ever
attached. A consumer records them through its own allowlist.

| Codes | When |
|---|---|
| `responder_text`, `responder_drawing_{inactive,unfocused,user_dismissed,app_dismissed}` | `update_first_responder` chose a view; the drawing cause is the first failing condition (scene active, GPUI text focus, dismissal). Emitted when the choice changes. |
| `dismissal_user_{will_hide,resign}`, `dismissal_application`, `dismissal_none_{show,focus,did_show,press}` | The keyboard dismissal changed, with its cause. |
| `keyboard_will_hide` | A `UIKeyboardWillHideNotification` this window owns. |
| `text_focus_gained`, `text_focus_lost` | GPUI's `TextInputStateChange::FocusGained` / `FocusLost`. |
| `press_recovered_text` | A printable press from the drawing view took the text input view back. |
| `press_dropped_no_text` | A printable press from the drawing view while no element takes text; once per drawing interval. |

`tests/input_recovery.rs` checks that the list is exactly what the decisions
produce and that every code is a lowercase static name.

## Fixes

1. A Mac has no software keyboard a user can dismiss, so an unrequested
   `WillHide` there does not record `Dismissal::User`
   (`keyboard::Context::user_dismissable`, false when `isiOSAppOnMac`). On an
   iPad nothing changes.
2. A printable press (a character that is not a control character, without
   Command or Control) that reaches the text input view while the drawing
   view is first responder and GPUI's focused element takes text clears the
   dismissal and asks for the text input view again, as `show_soft_keyboard`
   does. The press itself then follows its usual route through `super`.
   Nothing happens while no element takes text (a palette or a sheet), behind
   a native modal, or while the scene is inactive. This applies on an iPad
   too: there, a hardware press after the user hid the software keyboard takes
   the text responder back, so with a hardware keyboard attached typing
   continues; with none attached such a press cannot occur.

The first responder is taken on the next run-loop flush (`request_mode`), so
whether the very press that triggered the recovery reaches UIKit's keyboard is
not established; later presses do. Simulator checks cannot reproduce the Mac's
AppKit key delivery or keyboard notifications; the Mac acceptance is the
consumer's owner check.
