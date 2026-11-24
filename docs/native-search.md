# Native search panel and the native modal gate

`present_native_search(window, request)` presents a UIKit view controller
over the window's root controller: a title, a Cancel button, a
`UISearchTextField`, a status and a message line, and a results
`UITableView` (or an empty state with a clear button). It is a popover without
arrow (kept a popover in compact width) or, when the consumer asks for
`NativeSearchPresentation::Sheet`, a page sheet with medium and large detents
dimmed behind at every detent: the dimmed background takes no touches, so a
tap there neither closes the sheet nor reaches the window. The consumer decides which (yebisu:
a sheet below 352 pt).

The field is first responder while the panel is up. Keys, composition, Paste
and dictation go to it through UIKit's own responder chain; neither of GPUI's
views is first responder or in that chain (P0 premise probe,
`examples/native_search_premise.rs`). The presenting root controller, a plain
`UIViewController`, is in the chain, as Apple documents.

## Session

Events arrive in order on `NativeSearchEvents` (a `futures::Stream`):

| Event | When |
|---|---|
| `Opened` | UIKit finished presenting and the field became first responder |
| `Failed(NativeSearchFailure)` | the panel never became usable: the root controller was already presenting (`PresenterBusy`), the window lost its UIWindow (`WindowDetached`), or the field never took first responder (`FieldRefused`). No `Dismissed` follows |
| `QueryChanged(text)` | committed text changed (marked text excluded); the newest replaces one still unread. The final committed text is sent again before `Dismissed` when it differs |
| `ClearRequested` | the empty state's button |
| `Selected { id, revision }` | a row was tapped; `revision` is the `update` that showed it. The panel stays up: open the target, then `dismiss_selected(id)`, or show a message with `update` |
| `Dismissed(reason)` | `Return`, `Escape` (not composing), `Cancel`, `OutsideTap` (outside the popover), `SwipedDown` (a completed sheet swipe; an abandoned swipe changes nothing), `System` (UIKit removed the panel, seen on reactivation), `Selected(id)`, `Requested`, `WindowClosed` |
| `InputRestored(responder)` | after the handle was dropped: `Text` or `Drawing` became first responder, `WindowClosed`, or `None`: no GPUI view took first responder after 3 attempts 500 ms apart, and the gate opened anyway (a tap or focus change takes it later) |

`set_query(text)` replaces the field's text (for example after
`ClearRequested`). It becomes the session's last query: no `QueryChanged`
echoes it, and before UIKit presents the panel it becomes the field's initial
text.

Dropping `NativeSearchHandle` ends the session; while the panel is up it is
dismissed first. Hold the handle until a selection's target has opened, so the
terminal never takes input in between. The stream ends after `InputRestored`.
The handle is neither `Send` nor `Sync`: only the main thread can release a
session, and a handle dropped elsewhere would leave the gate shut.

## The gate

`State::native_modal` (`src/window/native_modal/gate.rs`, std only) is shut
from `present_native_search` accepting a request until a GPUI view is actually
first responder again after the handle was dropped. Readers:

| Reader | While shut |
|---|---|
| `Window::update_first_responder` (`lifecycle.rs`) | returns without acquiring (until `Restoring`) |
| `InputTarget::text_input_can_acquire` (flush, `didMoveToWindow`) | false (until `Restoring`) |
| `show_keyboard` / `hide_keyboard` | recorded in `modal_intent`, replayed at the restore, last wins |
| keyboard show/hide notifications | dropped and counted (the field's hide key never becomes GPUI's dismissal) |
| `InputTarget` dispatch, native edit dispatch, input handler | dropped and counted (`late_drops`): late IME results, Paste, dictation, Edit |
| `edit_window_active` (Edit `canPerformAction`, `validateCommand`, execution) | false |
| `present_native_edit_menu` | refused; a shown Edit menu is revoked (`Unavailable`) |
| GPUI view presses (`text_input/keys.rs`) | every phase of a fenced key is swallowed |
| `HerdrApplication.sendEvent:` | keys beginning in the key window while shut are fenced there; a release ends its key's fence in every window; keys UIKit stops reporting lost their release |
| `herdrMenuAction:` (drawing view, app delegate) | the command does nothing when its origin window (the key window when UIKit performed it) is shut or its key is fenced; checked again when the deferred delivery runs |
| `validateCommand:` for the app's commands | disabled while the key window is shut |
| window close, scene disconnect | the panel is dismissed without animation; one `Dismissed(WindowClosed)` (if none yet) and `InputRestored(WindowClosed)` |
| scene reactivation | the field takes first responder back if it lost it; a panel UIKit removed is `Dismissed(System)` |
| an abandoned sheet swipe (UIKit cancels the dismissal it began) | the field takes first responder back; UIKit resigned it when the swipe began |

Keys GPUI holds when the gate shuts get their `KeyUp` before it shuts, and
the SDK repeat stops; those keys stay fenced until UIKit delivers their
release. Modifiers are not fenced: GPUI adopts the flags every key reports.

## Tests

- `tests/native_modal.rs`: the gate's transitions (presentation failure, OS
  dismissal, late callbacks, window close in every phase, restore only after
  release, a restore retried and given up with `None`), the key fence, the
  keyboard intent, the menu decision, the late-input drop (F5), the field's
  Escape decision and the event queue. `scripts/native_search_mutations.py host` removes each guarded
  transition and requires the named test to fail.
- `examples/native_search_probe.rs` with the `NativeSearch/testProbe` XCUITest
  (`scripts/native-search-qa`), run by `scripts/check-native-search.py` on a
  disposable Simulator under `UIApplicationMain`. `scripts/native_search_mutations.py
  uikit OUT` runs it unchanged and with each reader removed; every case first
  checks the panel opened (or, for the presenter case, failed), so a mutation
  fails at its own assertion. The fixture's app command is bound to
  Shift-Command-B: on Command-B UIKit's Bold keeps the shortcut, the command
  gets no key command, and the press never reaches the menu's validate and
  perform paths.
- The fixture's `set_query` case (`c4`) sets a query before UIKit presents
  the panel (`set-query-before-present`), replaces and clears the field's text
  (`set-query-replaces-field-text`, `set-query-clears-field`), sets "final" and
  requires no `QueryChanged` through the dismissal (`set-query-no-echo`). Its
  UIKit mutations drop the field write (`set-query-not-applied`), the last
  query once the panel is up (`set-query-echoes`; the early text is kept, so
  the run reaches the echo check) and the early query
  (`set-query-lost-before-present`).

Hosted CI compiles the fixtures and runs the host tests; it cannot run the
`UIApplicationMain` fixtures (its Simulator runner spawns a bare process).

## Not shown here

Simulator input is XCUITest's synthesized typing: it does not show what an
iPad hardware keyboard, a real IME, Universal Clipboard or dictation deliver.
A real narrow window (below 352 pt) was not reachable in the Simulator; the
sheet was presented by request. Composition in the panel's field was not
reproduced with XCUITest typing (the ja-JP input mode was active); it was
checked with `setMarkedText:`. A command performed before a search opened and
delivered after it is covered by the host test of the decision, not by a
Simulator run. Escape is not in the fixture: XCUITest's Escape key never
reached the field (its `pressesBegan:` saw no such press, and a key command
for Escape did not fire either), so the field's Escape handling, read from the
press, is untested on the Simulator; the decision itself
(`escape_dismisses`) has a host test. An abandoned sheet swipe is not in the
fixture: when a drag begins the software keyboard hides and the sheet falls to
its medium detent by itself, so the same XCUITest drag was abandoned on some runs and completed on
others. On the runs where it was abandoned, the field had lost first responder
before the refocus on cancellation was added and kept it after.

Some readers have no Simulator mutation, because on every path the fixture
reaches another check already decides (each removal was run on the head tree,
`probe-30`, and the fixture still passed):

- The text view's own check (`text_input_can_acquire`): GPUI's text view was
  first responder in key-only mode (`mode=KeyOnly`, `probe-31`), so moving it
  out of its window and back, with the popover up or after it closed, made a
  request that settles without acquiring. The check matters for a text view
  in text mode, which the fixture did not reach. The steps stay as checks
  (`t-u2-text-view-rejoined-stays-off`, `...-while-awaiting-stays-off`).
- Dropping input at `dispatch` and at the native edit dispatch: the fixture's
  late Paste is refused earlier, since `edit_window_active` also needs a GPUI
  key responder, which the gate keeps from them. The input handler's drop is
  mutated (`late-input-delivered`) and the decision has a host test.
- The final query flush before a dismissal (F9): `unmarkText` itself sent the
  committed text, so `f9-committed-query-before-dismissed` held without it.
- The menu gate in a second window (a command performed in one window and
  delivered to GPUI's active window in another), and two keys held across
  `sendEvent:` with one released: neither was set up in the fixture.
