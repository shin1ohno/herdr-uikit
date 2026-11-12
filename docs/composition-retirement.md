# Experimental native composition retirement

This opt-in API replaces the `GPUITextInputView` owned by one window. Existing
consumers never request it and keep their existing responder behavior. It is
not a claim that UIKit cancels or preserves every OS-queued input across the
transfer. Do not use a local receipt as approval of that separate guarantee.

## Why this boundary

`unmarkText` commits the marked text in this SDK. `notify_text_input_changed`
updates UIKit's observed snapshot; `reloadInputViews` reloads input views;
resigning/reacquiring the same object preserves its callback destination. None
of those source paths carries a native cancellation acknowledgement. GPUI
0.3.12 sends IME-first printable keys to UIKit before a consumer KeyDown, so a
consumer key flag alone cannot reopen all fresh compositions.

Apple documents that `unmarkText` clears the marked range and that responder
acquisition can fail. Those are smaller contracts than queued-text retirement:

- https://developer.apple.com/documentation/uikit/uitextinput/unmarktext().md
- https://developer.apple.com/documentation/uikit/uiresponder/becomefirstresponder().md
- https://developer.apple.com/documentation/uikit/uiresponder/resignfirstresponder().md

This prototype chooses a distinct native object, not a timer, text-value
filter, or an epoch assigned when a callback arrives. No alternative internal
architectures or user-visible variants are included.

## API and ordering

`retire_text_input_session(window, retired)` returns a `TextInputRetirement`
receipt or a refusal that leaves the old client unchanged. The supplied callback
runs after GPUI releases the current update, before acquiring the replacement.
It receives `{ retired, replacement }` window-local identities. The consumer
must clear its old marked state and enable its new input session in that
callback, using weak entity/window handles and the usual GPUI update API.
Do not call SDK `unmarkText` to clear the composition. This SDK intentionally
does not impose terminal-specific cancellation semantics on a document editor.

1. Resolve the live window and reject duplicate requests or identity overflow.
2. Reject a request made from an active native text/key callback. Moving the
   remainder of an already-running key dispatch could lose or replay that key.
   `NativeCallbackInFlight` is an integration limitation, not successful cancel.
3. Permanently close the old object's lifetime token, before any native call.
   Every clone of its `InputTarget` retains that same token. No callback reads
   the new client's identity to decide its own origin.
4. Drain one pending operation per window at the outer GPUI callback/task
   unwind, after callback slots are restored. No run-loop timer is added.
5. Run the consumer callback once for a still-live window. Closed windows do
   not acquire a responder. Closing from the callback also prevents replacement.
6. Cancel old deferred selectors and key repeat. Build a new object, copying
   mode, traits, caret and geometry; do not copy delegate/tokenizer, marked
   state, held presses, or native session objects. Install the new owner before
   UIKit can query it. Preserve acquisition intent and application/user dismissal.
7. Attach, consume the attachment's queued flush synchronously, attempt acquisition,
   remove the old view, and record the actual responder result. Native queries
   never hold the view-storage RefCell across callbacks. Weak window references
   and cancelled old selectors prevent a retained old view from reviving input.

`TextInputRetirementStatus::Pending` is only a local fence.
`Replaced { first_responder, responder_requested }` reports what UIKit returned
at that moment, including failed acquisition. `WindowClosed` means no replacement
was installed. `text_input_session_state(window)` separately samples current
client identity, retired/pending flags, attachment and actual first responder.
Consumers must not equate `Replaced` with readiness when acquisition failed.

The old target also ignores observer/keyboard/AX updates and event dispatch;
retired views cannot reacquire or forward hardware keys up the responder chain.
The new object uses the existing AX acquisition notification. Planned old hide
notifications use the existing Restart reason. Keyboard flicker, late global
keyboard notifications and VoiceOver focus during a real swap need native QA.

## Unproven boundaries and adoption limits

The SDK fence isolates callbacks **addressed to the old native object**. UIKit
may have undelivered input whose eventual native recipient is chosen later.
Whether an old IME result can be retargeted to the new object, and whether new
English/software/IME-first hardware input is preserved during transfer, remain
unverified. Identical old/new strings are deliberately not filtered.

A nested UIKit event loop before the GPUI update unwinds can expose a fenced
old client before its replacement exists. There is no dropped-input buffer and
no replay. Such delivery would fail the no-loss requirement. Native callbacks
which synchronously start new composition during responder transfer also need
receiving evidence. A failed native acquisition, refused key-callback request,
or window close is never reported as a completed SH1-156 recovery.

This prototype can be integrated experimentally at non-native-callback discard
boundaries. It cannot yet justify removing yebisu's safety gates for all discard,
reconnect, Detach, software IME and hardware routes. Consumer selection, P7/P9
ordering, status, destination checks and the original Z1-Z7 remain consumer work.

## Tests and native proof

The initializer-based regression invokes real Objective-C UIKeyInput/UITextInput
selectors on retained old/new SDK views, through GPUI's actual input adapter.
It uses a test-only handler slot because a renderer/window is not constructed.
It checks late old insert/mark/replace/unmark/delete before and after replacement,
identical new text, direct English, emoji deletion, repeated replacement,
old selector cancellation and single-view ownership. It is **injected callback
order**, not software keyboard or UIPress delivery, scene focus or OS proof.

The existing key-routing tests cover the IME-first decision independently;
they do not prove a Japanese hardware key reaches the replacement after swap.
`client_lifetime` and `gpui_call` tests cover identity and unwind scheduling only.
Use `scripts/check-retirement-mutations.py` on the isolated Mac clone to verify
that removing retirement or reusing the old client fails the native adapter test
and that restoring the exact source passes. An environment/compile failure is
not a successful negative control.

For native observation, enable `HERDR_IOS_COMPOSITION_DIAGNOSTICS=1` and debug
tracing. Logs contain native client identity, callback operation, retirement and
responder flags, never text. Record both that trace and consumer peer receipts:

| Sequence | Required receiving evidence |
| --- | --- |
| Fresh connection, English hardware `a` | One `a` without tap; repeat off/on separately |
| Compose then discard; old result before/after fresh input | No old peer bytes; new text exactly once, even when identical |
| New software Japanese composition, partial emoji edit, unmark | Correct marked range and one final result without tap |
| Japanese IME-first hardware printable; Control key comparison | Native routing plus new composition; no KeyDown-only substitution |
| Focus loss/return, reconnect, Detach/reattach, host switch | New client/destination only; no late old bytes or stale focus |
| Key-only, full software keyboard, minimized, hardware attached | Actual responder and AX ownership; no unwanted reopening/flicker |
| Pending P7/P9 command barriers and native callback refusal | No replay/reorder, explicit refusal, successful subsequent fresh input |

Run actual app/Simulator receiving checks in addition to the SDK test binary:
`scripts/sim-runner.sh` explicitly runs outside UIApplicationMain. Physical
keyboard/IME behavior not reproduced by Simulator stays an owner device gate.

## Independent SDK menu change

This branch starts at SDK main `b2fa7811fa9fad9592f47b06d851fc0c33facb16`.
SDK PR #8 is independent and is not included. Both edit `InputTarget`, native
view code and exports; integrate with review of those overlaps. Any new Edit
availability accessor added there must honor the captured client's live gate.
Its per-window availability state must remain shared by the replacement, not
copied from an old native view. Re-run both suites after combining the branches.
