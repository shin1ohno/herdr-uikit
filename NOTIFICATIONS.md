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
reinterpreted. Foreground presentation is Banner + List for an explicit OS route.
The delegate is strongly held and routes response callbacks onto the main queue.

At most one request is in flight; overflow returns Busy. `remove_notifications`
invalidates in-flight submissions and removes only supplied bounded identifiers.
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
