# Connected local notifications

GPUI 0.3.12 hooks `show_system_notification` and
`on_system_notification_response` are implemented with UserNotifications.
No APNs registration, entitlement, App ID or provisioning changes are included.
The existing caller bundle must already be configured by its owner.

`notification_authorization` queries the actual OS asynchronously.
`request_notification_authorization` may present a prompt and must only be
called by a user's explicit UI action. Posting never calls it. Non-authorized
states (including provisional and ephemeral) are distinct typed failures.
Workers must never answer prompts or modify OS permission databases.

`post_notification` returns completion through a callback: `Ok` means the OS
accepted a request, not proof that it displayed a banner. GPUI's void hook logs
only typed rejection variants. No payload text is logged. Requests are silent;
consumer sound policy is independent. Actions are rejected rather than silently
reinterpreted. Foreground presentation is Banner + List only while the retained per-tag predicate is current; missing or invalidated tags return empty presentation options.
The delegate is strongly held and routes response callbacks onto the main queue.

At most one request is in flight; overflow returns Busy. `remove_notifications`
synchronously invalidates the supplied identifiers, including in-flight submissions, then schedules their native removal. Unrelated tags remain valid.
It must be called when a consumer's live connection context is invalidated.
Consumers must use fresh opaque tags and revalidate host, boot, generation,
coherent projection and pane before acting on responses. The SDK never connects.

Native compilation and real receiving-center tests are pending the exclusive
controller lease. Injected authorization and malformed-payload tests are not OS
banner proof. The ignored native query test requires an app-bundled harness and
never requests permission. Physical grant/deny/display/suspension remain owner
checks; no test grants permissions automatically.

Consumer integration: shin1ohno/yebisu branch feat/p14-connected-notifications,
base dc2511fe695935cdce60ba6bb3cfb4288a425778. Consumer must pin the exact reviewed
SDK commit before merging; SDK8/9 and other writers' branches are not included.

`post_notification_when(notification, current, callback)` additionally checks a
nonblocking consumer predicate after authorization, before add, after OS completion, and at willPresent. Response dispatch on the main queue uses issued-tag ownership instead of this short display predicate. Use
it for connection-bound effects: invalidate the consumer generation immediately
on disconnect and reject aged events. Native callbacks never infer that an old
mailbox remains current. Each retained ticket has an atomic invalidation guard. The registry holds at most 64 tickets; duplicate/live tags and capacity exhaustion return Busy. Callers remove tags on eviction and drop. No consumer code runs under the registry mutex; the mutex protects only bounded bookkeeping. Native responses invalidate display tickets; the consumer consumes its actionable record once. Duplicate issued-tag callbacks also reach that guard for a refusal status. Use fresh tags, never reuse an identifier across outstanding OS callbacks.

Resolution-only check: Cargo 1.99.1 `cargo update --package herdr-uikit` added only
objc2-user-notifications 0.3.2; no existing package version changed. Native
compilation and delivery remain unverified.

## Source repair and native obligations

The controlled callback-order tests call the same `complete_presentation` used by
the delegate and assert the exact options plus exactly one completion. Sequence:
add guard passes, disconnect, willPresent, add completion, native removal. Positive
current-tag, missing-tag, explicit removal, unrelated-tag, capacity, retention and
reentrant-predicate controls are included. These tests are AUTHORED, not executed
native evidence. Removing the predicate check must make the no-show assertion fail;
restore and rerun under the same exclusive lease. No source-only pass is acceptance.

The OS can present background notifications without asking willPresent. This
foreground callback fence is not a claim of atomic cancellation of background OS
delivery. Foreground/Inactive/Background ordering and receiving banners must be
observed with the real center; until then the connected-only native gate is open.
OS permission revocation is enforced by the OS, not inferred from the earlier
query snapshot. The permission query test remains distinct from a real grant or
from a user seeing a notification. No worker answers a dialog.


## D5 response lifetime

A tag receives an issuance receipt immediately before authorized native add,
not at registration or permission query. Native add failure revokes its receipt.
Up to 64 recent issued tags are retained for 600 seconds from issuance, with
lazy expiry on issue/response lookup and FIFO eviction. Taps never extend this
window. Receipts hold weak ticket references, not consumer predicates or payloads.
Display removal/drop releases predicates immediately; receipts let removed,
expired-action and duplicate taps reach the consumer's bounded refusal status.
Unissued, evicted or receipt-expired tags fail closed with a typed SDK warning
without logging the tag. They never invoke a consumer action. This receipt
window is not an action grant: yebisu retains its original 300-second limit.
Fresh opaque tags must never be reused, including after display removal.

The test-support-only NotificationResponseFixture calls the same response::dispatch
used inside the delegate's main-queue closure, and the same complete_presentation
as willPresent. It uses isolated registries/injected time, not a fake OS success.
Four SDK response tests cover six seconds, removed/expired/duplicate tags,
denied/unissued/dismissal/action handling, 64 receipts, 600-second lazy expiry,
native-error revocation and release of captured consumer state. Six consumer
integration tests use this boundary with real Smoke validation and a private
wire peer/FIFO barrier. No production public API or default policy is added.
All Rust/native executions and actual delegate/center/banner/sound remain UNRUN.
