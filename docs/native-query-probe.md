# Captured native query probe

This is an additive `test-support` API on `retirement_probe::NativeClient`.
It observes the same retained main-thread native object that `capture(window)`
returned. Replacing the current client does not retarget a captured probe.
This helper provides no retirement admission, readiness or OS delivery guarantee.

```rust
pub struct NativeQuery<T> { pub client: u64, pub value: T }
pub enum NativeQueryError { IntegerOutOfRange, NegativeOffset, ReversedRange }
// Methods on NativeClient:
pub fn selected_range_utf16(&self)
    -> Result<NativeQuery<Option<Range<usize>>>, NativeQueryError>;
pub fn document_length_utf16(&self)
    -> Result<NativeQuery<Option<usize>>, NativeQueryError>;
pub fn position_offset_utf16(&self, from: usize, delta: isize)
    -> Result<NativeQuery<Option<usize>>, NativeQueryError>;
pub fn select_utf16(&self, range: Range<usize>)
    -> Result<NativeQuery<bool>, NativeQueryError>; // SETUP ONLY
```

The three observers never set selection, request focus or replace a handler.
They sample the captured identity before invoking actual UIKit selectors, and
measure offsets from actual positions. Nil stays `None`; native zero stays
`Some(0)`, including when a retired object returns an empty snapshot. Numeric
errors remain distinct from nil. `NSInteger` is the native `isize` alias; unsigned
indexes are checked before passing them to native signed arguments. The helper
does not synthesize document bounds, add offsets or read position/range ivars.

`select_utf16` is setup only. Invalid numbers or reversed ranges fail before
mutation; nil native positions/range return false without a setter. Otherwise
it sets selection once and reads actual selection back independently. A false
result is not repaired or retried. Never call it after Cut or Escape to make an
observation match the expected selection.

## Adopter protocol

Record exact SDK/consumer heads and `client` for every query. Keep the same
captured client for each before/after pair. Set up `abcdef` at caret 3; observe
selection `3..3`, length 6, position `(3,+1)=4`, `(3,+3)=6`, `(3,+4)=None`, then
the unchanged selection, with no setter between observations.

For `pre:変😀中:post`, mark `4..8`, selection `5..7`, observe length 13.
After actual native Cut, the FIRST query must observe selection `5..5` before
any setter/focus/handler operation; length is 11, mark `4..6`, text
`pre:変中:post`, `(5,+6)=11`, beyond end nil. Check selection again after queries.
These are required consumer observations, not results supplied by this commit.
Actual Escape, its approved exact caret oracle, wrong-Cut-caret negative and
the exact P13 D6 removal/assertion/restore belong to the P13 owner. Native unmark
commits by default and must not be substituted for Escape/discard.

## Authored fixtures and pending execution

`tests/native_queries.rs` uses actual Objective-C selectors on a controlled SDK
PlatformInputHandler. It covers document/UTF-16 bounds, setup refusal, nil,
negative/reversed native offsets, retained old identity and actual retired zero.
Scoped test IMP overrides count actual setters, inject nil/error responses and
restore original methods on unwind. The main-thread initializer does not pump a
run loop. This fixture is SDK-host evidence only; its post-edit state is assigned
directly, not produced by the consumer's native Cut/Escape implementation.

`scripts/check-native-query-mutations.py` defines four separate negatives:
Q05 current recapture, query selection repair, hiding retired zero, and
normalizing reversed setup. The retired-zero negative is separate from Q05.
It records the head, source/mutant hashes, raw logs and exit statuses; requires
baseline, the named executed assertion, exact-byte restore and rerun. It does
not implement or claim the consumer D6 negative. Compilation/setup failures
do not count as a killed mutant. All these native fixtures remain UNRUN at the
source stage; a script is not a runtime result.

Q05 registers the fixture's actual `Rc<TextInputView>` solely for a mutant-only
lookup. The baseline helper has no lookup dependency. After real
`begin_retirement`/`finish_retirement`, the old retained object has a different
identity from the installed current object. Current is a positive control with
native length 6; old must return its original identity and actual native zero.
The single-method mutant makes `document_length_utf16` capture that real current
object for both identity and native selectors. It must fail
`p13-q05-retained-client-not-current`, which compares the complete old identity
and native value. The fixture does not fake a client, expected length, or query
return. Its registry is test-only and restored on unwind. All Q05 baseline,
mutant and exact-restore runtime observations remain UNRUN until a fresh lease.

Only after a fresh exclusive mini lease, use an isolated exact-head clone,
the existing absolute 6 GiB guard and simulator runner. Required commands:

```sh
cargo fmt --check --all
RUSTFLAGS='-D warnings' cargo clippy --locked --all-features --all-targets --target aarch64-apple-ios -- -D warnings
RUSTFLAGS='-D warnings' cargo clippy --locked --all-features --all-targets --target aarch64-apple-ios-sim -- -D warnings
cargo test --locked --all-features --target aarch64-apple-ios-sim
python3 scripts/check-native-query-mutations.py
```

The caller must set `CARGO_TARGET_AARCH64_APPLE_IOS_SIM_RUNNER` to the absolute
runner, `HERDR_RETIREMENT_LEASE=exclusive` only after the actual grant, and a new
task-owned `HERDR_QUERY_EVIDENCE_DIR`. Run the relevant receiving cases in the
real consumer separately. Source review cannot accept S1–S6, Z1–Z7 or B2.

## Provenance and scope

Original additive implementation against this repository's retained NativeClient
and native selector implementations at base `789fb43b947b8c1b120c375cdf7fadeb1514e759`.
The admitted query plan SHA256 is
`9d025a3fc4c3fee32e7dc51ab8545842cdb4604be6971b94ed3b1b09909128b1`.
Objective-C ABI checked against local objc2 0.6.5 and objc2-ui-kit 0.3.2 sources.
No production selector, document-length fallback, retirement behavior or consumer
source changes are part of this helper commit. The independent B1 source stage
and subsequent native lease have their own review gates.
