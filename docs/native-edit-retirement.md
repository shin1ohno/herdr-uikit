# Limited native Edit retirement (B1 source stage)

This is an additive experimental API for yebisu SH1-156 composition retirement.
It does not establish UIKit cancellation, OS retargeting, physical-key recovery,
consumer adoption or the original S1-S6 / Z1-Z7 / B2 acceptance. The older
`retire_text_input_session` API and SDK8 defaults remain available unchanged.

## Published API

`native_input_origin(AnyWindowHandle) -> Option<NativeInputOrigin>` samples the
weak Window allocation identity and immutable native client ID, without a
handler loan or focus request. Equal numeric client IDs in different Windows
are different origins. `NativeInputOrigin::client()` exposes only the numeric
component; use equality on the whole origin when validating ownership.

`request_native_edit_retirement(window, expected, prepare)` returns
`Result<NativeRetirementRequest, NativeRetirementRequestError>`. `prepare` is a
main-thread `FnOnce(PreparedNativeRetirement) -> PrepareDecision + 'static`.
The main thread is checked before mutation. Origins and receipts use Rc/Weak;
they cannot be sent between threads. `RequestKey::origin()` and `serial()`
identify a single accepted request. Serials and successor IDs are checked and
never wrapped. Receipt clones share state; they do not duplicate callback work.

The only admitted contexts are the exact SDK continuation of text/drawing
`copy:`, `cut:`, `paste:` or `selectAll:`, and a tagged main-thread DispatcherTask
with no native frame. Only EditDispatch and PasteFallback phases admit requests.
ObserveSnapshot counts loans but does not admit requests. A restored callback
slot, GPUI depth or synchronous nesting alone never grants admission.

The operation spans availability, Down, Up, restoration of native-action origin
and modifiers, optional unhandled Paste fallback, snapshot sampling/store and
drawing forwarding return. An enclosing sendEvent must return from super and
restore event delivery state. Completion occurs at an explicit normal-return
epilogue after native/task frames and all callback/handler loans have ended.
No operation/loan Drop callback drains the new API.

Any distinct native entry after submission interrupts the queued request before
that entry's body. This includes same-object queries, raw later/scene closures,
new selectors and nested events. The original body runs normally against its
own endpoint. No input is buffered, relabeled, filtered by value or replayed.

General text, unmark, selection, query, responder, lifecycle, observer and raw
callbacks refuse requests. Native unmark still commits via replacement and
restores selection. Hardware presses, repeat and keyboard/unknown shortcut
routes set sticky Window hardware history before dispatch. New requests for
that Window then return UnsupportedHardwareContext, including after release or
retirement. This restriction leaves hardware/general-text recovery unsupported.

## Ownership and outcomes

There is one shared per-Window legacy/new reservation and one global new-API
reservation. No new queue, timer, replay buffer or refill loop is introduced.
Each eligible epilogue processes at most one new reservation. Consumer callback
work and the existing legacy AFTER refill loop have no new global time bound.

| Status | Meaning |
| --- | --- |
| Queued { old_fenced_at_submission } | Serial/slot reservation only; preserves old's prior fence. No successor exists. |
| Preparing | Old is permanently fenced; prepare is running. |
| Replaced { successor, responder_requested, first_responder } | Actual installed successor and synchronous acquisition observation. False readiness stays false. |
| Declined { old_fenced: true } | No successor; old remains fenced and current. |
| InterruptedBeforeFence { reason, old_fenced_at_submission } | Before THIS request's fence step; preserves any fence from an earlier request. |
| WindowClosed { old_fenced } | Before Preparing this is the captured prior value; after Preparing it is true. |
| InvalidatedAfterFence | Revalidation failed after fencing; old is never reopened. |

Errors are NotMainThread, WindowUnavailable, StaleOrigin, InProgress,
NewCapacityBusy, IdentityExhausted, UnsupportedNativeEntry,
UnsupportedHardwareContext, UnsupportedReentrancy, LegacyDispatchActive and
UnprotectedEntry. Refusals do not reserve a serial, fence, call prepare or change
native focus. Same-Window InProgress takes precedence over cross-Window capacity.

At the safe tail, revalidate the weak Window, current captured object, origin,
reservation and successor arithmetic. Fence old, publish Preparing, cancel its
queued native work and call prepare without any pre-existing SDK/AppCell loan.
`PreparedNativeRetirement` contains `key`, `old_origin` and `owner_cancelled`;
it carries no reserved successor or readiness acknowledgement.

`PrepareDecision::Decline` creates no successor. `Proceed(completed)` authorizes
replacement after revalidation and supplies one terminal callback. Publish the
terminal outcome before that callback; retain Window/global reservations through
its return, release them, then release the synchronous legacy AFTER hold. Close
or invalidation after Proceed still invokes the terminal callback once. A
cancelled owner returning Proceed is coerced to Decline without installing or
invoking that completion. Native acquisition may run synchronous queries; new
requests there remain refused. Unexpected OS input during acquisition is B2.

## Consumer handoff requirements

1. Register a distinct consumer owner ticket before requesting. Keep the discard
   gate closed while Queued and Preparing. Match the full origin and RequestKey.
2. In prepare, enter the weak GPUI update and recheck host, generation, selection,
   revision, destination, open/suspended/core state, Window and focus. Decline on
   weak failure, cancellation or any mismatch. Proceed alone never acknowledges.
3. In the terminal callback, recheck the same ticket, actual current origin,
   successor and responder readiness. Only then acknowledge the matching discard
   revision once. False readiness, close or owner mismatch keeps the gate closed.
4. To cancel, invalidate only the matching owner ticket first, then call
   `mark_owner_cancelled()`. Queued returns Marked idempotently; Preparing returns
   TooLate; terminal returns AlreadyTerminal. Dropping receipts does not cancel.
5. Wait for terminal callback return and slot release before a fresh ticket.
   After Decline, an explicitly tagged DispatcherTask may retry the same fenced
   origin with a new RequestKey. It never revives old; only its own Proceed can
   construct a successor. Do not schedule timers to convert unsupported input
   into apparently supported recovery, or paste legacy retired+1 acknowledgement
   arithmetic into this API.

Same-Window legacy requests remain InProgress during new work. A successful
other-Window legacy request interrupts NewQueued and releases it before legacy
AFTER starts. Refused legacy requests do not interrupt. During new prepare,
acquisition and terminal callback, other-Window legacy requests retain ordinary
acceptance but their AFTER callback waits until the new transaction releases.
New requests never join or refill an active legacy drain.

## Evidence and execution limits

`scripts/check-b1-source.py` compiles selected production Rust modules with
explicit native/GPUI doubles. Its three raw-body fixtures extract actual closure
bodies; external effects are doubled. It covers scheduling/ownership source,
not the target library, Objective-C ABI, AppCell or UIKit receiving behavior.

`examples/b1_probe/` uses real SDK Windows/selectors and callback slots. It
authors text/drawing edits, fallback, query interruption, cancellation, mixed
live/fenced requests, raw bodies and an empty hardware refusal. It is a SUBSET
of A1-A16; focus/detach/reconnect, every text/press variation, consumer peer
receipt and physical OS delivery are not supplied by its PASS marker.

`scripts/check-public-b1.py` authors eight single source-removal negatives and
exact restores. None of its native runs is authorized by this document. Under a
fresh controller-granted exclusive mini lease and the existing absolute 6 GiB
guard, use the frozen HEAD with device/simulator clippy all-targets/all-features,
SDK tests, this native subset and the separate P13 Q05 query mutations. Set
HERDR_B1_EXPECTED_HEAD, HERDR_RETIREMENT_LEASE, HERDR_PROBE_OUTPUT,
SIMULATOR_UDID and the task-owned CARGO_TARGET_DIR from that lease. Preserve all
baseline/failure/negative/restore logs. Never infer OS no-loss from these fixtures.
