# Native edit observation: Simulator QA only

Source contract: admitted plan SHA256
`0461ca658933341948bc47aa09fe5b1015fe6e1bb3d1be2f85a6f8623c6bf4d9`.
Base: SDK8 `3e1d03c43965145f13860e8e344c27bf5b09521d`.
This implementation and its source tests do not establish native or physical
behavior. Independent review and exact-head native validation are required.

## Build and opt in

Every observer API, module, registration ivar, hook, TLS, counter and test seam
is compiled only for `all(target_os="ios", target_abi="sim",
feature="test-support")`. No new default feature or dependency is added.
Enable the existing `herdr-uikit/test-support` only in the reviewed Simulator
QA build. Keep device dependency configuration and all references to this API
feature-off. Do not enable a workspace-wide feature for device builds.

```rust,ignore
#[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
fn observe(window: gpui::AnyWindowHandle) -> Result<(), herdr_uikit::native_edit_observer::ObserverError> {
    use herdr_uikit::native_edit_observer as obs;
    let run = obs::begin_run()?; // once per process; record and reject reused run IDs
    // Freeze exact SDK and consumer heads, cfg, expected policy route, phase,
    // action order and expected receiving transcript BEFORE this arm.
    let armed = obs::arm(window, run, obs::Phase(1))?;
    // Record actual arm acknowledgment. Separately require the real selected
    // injected-provider preflight BEFORE any QA key operation.
    obs::invoke_native_selector(window, obs::Action::Copy, obs::ResponderOrigin::Drawing)?;
    let result = obs::drain(window, armed)?;
    // Require one actual Entry + Outcome with the same full key, predeclared
    // action/origin/decision, zero invalidity/drop/outstanding/reservation.
    // Match consumer callback and receiving completion independently.
    assert!(!result.status.sticky_invalid);
    // Join actual async worker/publication and receiving receipts, including
    // trailing transport drain/termination, before moving to the next phase.
    obs::disable(run)?;
    Ok(())
}
```

`current_native_observation_key()` augments `native_edit_action()` without
changing the older origin semantics. Read it only in the real synchronous
consumer KeyDown/KeyUp callback and record a fixed modal-refusal receipt there.
No key is exposed during modifiers, fallback, refresh, physical dispatch or
deferred work. Nested completion can be inner-before-outer; join by full
run/epoch/view_generation/phase/invocation, never record position.

`invoke_native_selector` sends a fixed selector through the real Objective-C
Text or Drawing entrypoint on the resolved live view. It is a stimulus, not
an entry/completion/receiving receipt. It also works with observation disabled:
the native oracle must fail then despite identical production handling.
The API does not use private UIKit selectors or send a synthetic key directly.
Unknown actions are rejected; validation callbacks never create invocations.

## Identity, phase and lifetime

`begin_run` explicitly creates a fresh nonsecret OS-random UUIDv4 (u128) once.
The runner must reject reuse across launches; the SDK cannot inspect prior
process artifacts. Arm resolves the existing live-window registry, releases
registry/window-state borrows, and registers fixed numeric data on the
canonical text view. Generation, epoch and invocation counters are checked
monotonic and never reset. Capacity is process-wide: 16 registrations, 32
outstanding/nested invocations, 256 ring records. Each admission reserves both
Entry and terminal Outcome. A later view never reuses a removed generation.

Each global arm invalidates previous arm epochs for observation admission and
drain. Rearm refuses outstanding invocations and undrained records. Disable,
close, counter overflow, loss, contention and unwind remain invalid until exit.
`run_status(run)` exposes invalidity after a view disappears. A callback can
drain an outstanding Entry; its terminal slot remains reserved. Out-of-band
atomic invalid/drop state is never cleared by drain/rearm/disable. Snapshots
are fixed copies; no serialization/callback/UI/I/O runs inside observer borrows.
`Status.queued` is the returned snapshot's count before consumption, and
`snapshot_sequence` increases on each successful status/drain snapshot.

**Capability gap: external UIKit scheduling cannot carry this observation key.**
The SDK does not know whether the consumer still has delayed selector work.
The runner must freeze phase before scheduling and wait for actual matching
completion of every operation, then terminate/drain transport before rearm.
A scheduling acknowledgment or timer cannot satisfy this obligation. On any
cancellation or timeout call `invalidate_run(run)` and terminate that run.
There is no API claiming to join or cancel a consumer's async paste/transport.
The current P11 timer/FIFO endpoint must stay failing until its owner replaces
it using actual receipts in a separately reviewed integration change.

## Schema and oracle

The Rust public schema in `src/native_edit_observer/types.rs` is fixed data.
Entry has `decision=None`; its terminal Outcome has the sampled branch/reason.
An aborted decision read has no fabricated reason. Refused has no handled or
fallback payload. Dispatched has the actual KeyDown boolean and fixed fallback
`Invoked` iff the existing unhandled Paste fallback closure ran. This does not
claim clipboard, handler, async publication, modal refusal or receiving success.

Consumers serialize explicit allowlisted numeric fields/enums/booleans only.
Use fixed numeric encodings for Action/Decision/Responder/Fallback and explicit
Event/Outcome tags; reject unknown fields/discriminants, duplicate keys or
pairs, missing terminals, wrong action/view/run/phase, invalid status, and
inconsistent handled/fallback. Encode u128 run losslessly (decimal digits in
an explicitly validated fixed-width representation, never a JS float).
Never serialize a Rust debug dump of a surrounding view/handler/window.
No payload or free-form map is part of this API. No selector string, pointer,
clipboard/text/key/token/credential/title/host/snapshot/handler is collected.

P11 adoption must retain the historical native 0/4 gate until the new real
entry/outcome oracle and its negative controls actually pass. Distinguish:

| Predeclared route | Required terminal evidence |
| --- | --- |
| Outside modal Some allow + active | Dispatched + real consumer action receipt + receiving result |
| Modal None + no Text mode / no focus | Exact RefusedNoTextMode / RefusedNoTextFocus; no consumer dispatch |
| Modal Some disallow + active | RefusedPolicyDisallowed; no consumer dispatch |
| Modal Some allow + active | Dispatched + correlated real consumer modal refusal + handled=true + NotEligible |
| Some inactive / no responder | RefusedInactiveOrNoResponder; dedicated negative, not modal substitute |
| Stale or foreign menu-tagged delivery (sender tag matches no current menu token) | RefusedMenuDelivery; terminal Refused before any edit policy, no consumer dispatch; appended last so earlier discriminants are unchanged |

Run Text and Drawing origins separately. Retain all four actions and four
settings combinations, the fake clipboard, and the complete ordered 24 FIFO
receiving segments with correct targets. Inspect the entire transcript after
the end barrier and actual transport termination. SDK refusal/handled alone
never proves zero wire traffic. Do not infer Mock-provider preflight from arm.
The final-writer race, F01-F26 and the entire P11 acceptance remain separate.

## Before an exclusive Mac lease

The source-only fixture compiles the actual queue/runtime files with stubbed
platform services. Its passing tests are **not** evidence of UIKit callbacks,
OS entropy, live-handle resolution, compiler target cfg or native privacy.
Existing action tests and `actions/observer_tests.rs` exercise real dispatch
helpers; their fixture invocations are not native-selector positive controls.
The native stimulus API above is ready for the consumer's actual QA app.

Required exact-head lease gates (do not run concurrently with another build):

1. Device and Simulator compile/clippy; Simulator SDK tests feature-off/on.
2. Actual app, selected injected-provider preflight, fixed manifest/arm,
   four real selectors, four settings, both origins and actual 24 FIFO receipts.
3. Some/None SDK refusals versus consumer modal refusal, all four distinct
   actual pairs, receiving zero-wire with final drain/termination.
4. Disabled observer, removed-entry hook, removed-outcome hook, stale/wrong
   identity/phase/action, forged handled/modal receipts, duplicate/double count,
   unknown schema/enum/fields, relaunch/generation replay, ring/outstanding/
   registration limits, counter wrap, teardown/disable/rearm, nested native/
   physical, callback drain, contention, and KeyDown/fallback/refresh unwind.
   Each mutant must fail for the intended reason with a selector positive
   control; never accept a compile/setup failure as a killed mutant.
5. Literal and encoded fake-public/private sentinels in actual consumer drain
   serialization, JSONL, stderr, diagnostics and runner sinks. Every sink needs
   a positive collection control; missing/empty/overflow is failure. Restore
   baseline after mutations and observe receiving zero-wire privacy again.

No native, physical, real-key, Keychain, device signing or permission-dialog
result is established by a source fixture. No SDK merge is authorized.

## Synthetic and unknown input provenance

Generic `InputTarget::dispatch` / `Window::dispatch` clear the new observation
key for every input, including KeyDown/KeyUp from `editing::tap_key` (control
insertion / deleteBackward) and any unknown direct ingress. Only the canonical
native-edit closure passes its own invocation key through `dispatch_native_edit`.
That route admits the key only for KeyDown/KeyUp, suppressing modifiers. Scoped
restoration preserves the outer native key after a different window's synthetic
callbacks and after callback unwind. Existing `native_edit_action` semantics,
callback handling/count, and SDK8 defaults are unchanged by this provenance rule.

The correction source fixture includes the actual tap_key, InputTarget and
Window ingress/dispatch/callback-extraction functions. Its window/view/platform
shells are host-shaped stubs, so it is not actual UIKit/native/receiving proof.
