# Retractable Linux system popups

Accepted 2026-10-22. Continue ATT-01–05 after the durable completion and pending
claims in ADR 0114/0115. System presentation consumes those same batches; it does
not observe provider deltas independently or introduce another delivery cursor.

Add `system_popups` to the validated device-local alert policy, default false.
Old JSON remains readable and false is omitted on serialization. Existing CAS,
scope and failed-save rules apply. Linux exposes an opt-in Alerts button; other
platforms retain the setting without attempting unsupported system delivery.
Enabling, reopening and reconnecting never replay consumed claims.

Use the freedesktop Notifications D-Bus interface through zbus, already present
in the pinned desktop dependency graph. The installed GPUI Linux implementation
cannot retract its system notifications and starts a blocking waiter per
notification; its wrapper does not meet this lifecycle or workload. Kit remains
the in-app presentation. No vendor toolkit changes are needed.

Each Workbench lazily starts one dedicated transport worker, retaining at most
two server handles: one pending aggregate and one result aggregate. Latest-state
watch channels coalesce updates; neither channel grows with event count.
Connection/calls have explicit three-second deadlines. Notifications use only
counts, an eight-second expiry, and `suppress-sound` and `transient` hints. Hints
and presentation are controlled by the notification server; this is not proof of
silence, display or exclusion from system history on every desktop.

Get capabilities before offering an action. Each action carries a fresh popup
generation, so an old action arriving after a same-ID replacement cannot open
the replacement. Validate the current desired generation again on the UI thread.
The explicit action opens the corresponding all-Host Attention page without
claiming control, opening a provider session, answering, reviewing or sending.
Existing popup, composition, export and close guards apply. There is no activation
token/Wayland focus claim; an explicit button is used instead of assuming every
server supplies a default body action.

Pin the notification service's unique D-Bus owner. Server-ID reuse after restart
must never close another client's popup or accept another owner's action.
Owner loss/replacement and transport/protocol errors stop delivery, close owned
handles where possible and produce a visible Alerts error. Explicit Reconnect
discards presentation state and permits future events. No automatic transport
retry or replay is introduced. An unavailable service does not affect the real
queue or Host observations.

A newly observed conflicting mute, popup disable, resolved pending occurrence,
reviewed/evicted result or unavailable source retracts the whole affected
aggregate. Other-window policy changes follow the existing two-second polling
boundary. Successful policy persistence precedes reconciliation. Normal window
close drains existing saves, closes its transport and waits off the UI thread
for bounded cleanup before removing the window. Abrupt termination cannot run
cleanup; OS expiry remains server-controlled.

Claims remain at most once and precede presentation. A close, crash, busy policy,
server failure, coalesced batch or OS suppression can lose a presentation. This
is not an acknowledged inbox, guaranteed delivery, a durable Host feed or
cross-device notification coordination. Tray/dock badges, configurable sound,
macOS/Windows system transports and native Wayland acceptance remain separate.

Reference: [freedesktop Desktop Notifications Specification 1.3](https://specifications.freedesktop.org/notification/latest-single/),
GetCapabilities, Notify, CloseNotification, signals and hints. Validation is in
[the Linux acceptance receipt](../verification/2026-10-22-system-popups.md).
