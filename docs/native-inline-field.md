# Native inline field

`present_native_inline_field(window, request)` lays one native
`UITextField` over a rectangle of the window, for editing a short name in
place (a row's or a tab's label). It is generic: the request carries the
rectangle, the text, the limits, the hints and the look, and nothing in the
API names a consumer.

A clear full-screen view controller is presented over the window's root
controller (`UIModalPresentationOverFullScreen`, clear background, no
animation). Its view, the overlay, holds a clipping container with the
field's box (border color, then the background inset by the border width,
rounded corners) and the field, and one hint `UILabel` below the field.
Nothing is dimmed: GPUI keeps drawing everything around the field (probe
check `p1-gpui-keeps-drawing-under-the-field`).

The field is first responder while it is up. Keys, composition and Paste go
to it through UIKit's own responder chain; neither of GPUI's views is first
responder or on that chain (probe check `p1-gpui-views-not-in-field-chain`;
the chain is field, box views, overlay, the presented and the presenting
controller, the transition view, the window).

## Request

| Field | Meaning |
|---|---|
| `frame` | the label the field covers, in the drawing view's points |
| `clip` | the visible part of the label's scroll area; outside it the field hides and keeps editing |
| `text`, `select_all` | the initial text; selected on open so typing replaces it |
| `placeholder`, `accessibility_label` | as UIKit uses them |
| `allow_empty` | whether Return may commit an empty name |
| `max_chars` | the most Rust `char`s the name may have |
| `empty_hint`, `length_hint` | the hint lines |
| `style` | `NativeInlineFieldStyle`; the defaults are yebisu's look below |
| `identifiers` | accessibility identifiers of the field, the overlay and the hint |

The field's rectangle is the label's, `outset_h` (4 pt) wider and `outset_v`
(2 pt) taller on each side and at least `min_width` (120 pt) wide, growing
to the right so the text keeps the label's left edge (`policy::field_rect`).
The text is inset by `padding_h`/`padding_v` (4/2 pt) from the box's edges.
Default style: background `#101419`, border 1 pt `#88c0d0`, corner radius
4 pt, text `#d8dee9`, placeholder `#6b7080`, caret and selection
`#88c0d0`, the system UI font at `font_size` and `font_weight` (UIKit's
`UIFontWeight`), no clear button; the hint 12 pt `#bf616a`, 4 pt below the
field.

## Session

Events arrive in order on `NativeInlineFieldEvents` (a `futures::Stream`,
a FIFO of at most 8):

| Event | When |
|---|---|
| `Opened` | UIKit finished presenting and the field became first responder |
| `Committed(name)` | Return, not composing, with a changed name: trimmed, at most `max_chars`, empty only with `allow_empty`. At most one per session, just before `Dismissed(Committed)` |
| `Dismissed(reason)` | exactly one: `Committed`, `Unchanged` (Return with the name unchanged; nothing to send), `Escape` (not composing), `OutsideTap`, `Requested` (`dismiss`, a dropped handle, or UIKit removing the controller on its own), `WindowClosed`, `PresentationFailed(failure)` (the field never became usable: `PresenterBusy`, `WindowDetached`, `FieldRefused`; no `Opened` came) |
| `InputRestored(responder)` | after the handle was dropped: as the search panel's |

Return's decision is `policy::commit_decision(initial, text, allow_empty,
max_chars)`: the trimmed text equal to the initial text is `Unchanged`; an
empty text without `allow_empty` keeps the field and shows `empty_hint`;
otherwise `Committed`. Composition is never cut: typing and Paste outside
composition stop at `max_chars` in `shouldChangeCharactersInRange:` (the
change is cut to fit and `length_hint` shows), and text that grew past it
while composing is cut when the composition ends.

`set_frame(frame, clip)` moves the field on the next main-queue turn: at
most one frame late. The rectangles are converted from the drawing view to
the overlay with `convertRect:fromView:`; with nothing of the field inside
`clip`, the container hides and editing and the keyboard go on.

Dropping `NativeInlineFieldHandle` ends the session; drop it only after
acting on `Committed`, so the consumer's own input never takes a key in
between. Every call is main-thread only; the handle is neither `Send` nor
`Sync`.

## The gate

The field uses the window's native modal gate exactly as the search panel
does (`docs/native-search.md`, "The gate"): it shuts at
`present_native_inline_field`, GPUI's views resign before the field takes
first responder through the window's responder guard, and the gate opens
only once a GPUI view is first responder again. The window's one session
slot holds either kind (`native_modal::Session`), so the search panel and
the field are never up together: whichever comes second gets
`AlreadyPresented` (probe checks `gate-shared-*`). The gate's own dismissal
reasons are mapped (`policy::dismiss_reason`): `Return` becomes `Committed`
or `Unchanged` by Return's decision; `System` and the search-only reasons
become `Requested`.

Escape is read in the field's `pressesBegan:withEvent:`, before UITextField
handles the press, and closes only while nothing composes: the search panel
found that a key command for Escape does not fire while a field is first
responder. A tap anywhere outside the field reaches the overlay's
`touchesBegan:withEvent:`, which closes the field and passes the touch to
nothing else (probe check `outside-tap-dismisses-once-and-sink-sees-zero`).

## Menu item subtitles

`set_menu_subtitles(name → subtitle)` replaces a table of small lines UIKit
shows under disabled menu items (`UIMenuElement.subtitle`); GPUI's
`MenuItem::Action` has no field for it. An item shows its subtitle only
while disabled. The current commands take the new table and UIKit rebuilds
the menus.

## Tests

```sh
cargo test --locked --tests                         # tests/native_inline_field_policy.rs
python3 scripts/native_inline_field_mutations.py host
python3 scripts/test-simulator.py                   # tests/native_inline_field_uikit.rs, menu tests
python3 scripts/check-native-search.py --example native_inline_field_probe \
    --test InlineField/testProbe --prefix INLINE --output /abs/new/dir
python3 scripts/native_inline_field_mutations.py uikit /abs/new/dir
```

`tests/native_inline_field_policy.rs` (host) covers Return's decision, the
length cut, the rectangle and clip, the shared gate, the bounded queue and
the seven closers each sending one `Dismissed` and then one `InputRestored`.
`tests/native_inline_field_uikit.rs` runs the real controller, field,
delegate and hint outside `UIApplicationMain`: the style, a 200-character
Paste cut to 160, the cut after composition, placement within 1 pt, hiding
outside the clip, and the Escape press. The probe
(`examples/native_inline_field_probe`, UIApplicationMain, XCUITest
`InlineField`) checks the premises first, then Return, an unchanged and an
empty Return, Escape (sent to the field's own press selector), `set_frame`
and the clip, the shared gate, XCUITest's typing, an outside tap, and
window close. Each mutation in `scripts/native_inline_field_mutations.py`
must fail its named test or fixture assertion.

## Simulator and device

| Behaviour | Simulator | Device |
|---|---|---|
| First responder, GPUI views off the chain, GPUI keeps drawing | probe | owner check |
| Return/Escape while composing keep the field (marked text set by `setMarkedText:`) | probe | owner check with the Japanese IME |
| Japanese romaji typed by XCUITest after Control-Space | recorded only (`s3`) | owner check |
| `shouldChangeCharactersInRange:` called while composing | recorded only | not asserted |
| Escape from a hardware keyboard | not deliverable by XCUITest | owner check |
| Outside tap with a finger | probe (XCUITest tap) | owner check (finger, Pencil, trackpad) |
