# Pending request occurrence identity and reminders

Accepted 2026-10-22. Extend ATT-01/05 and ADR 0114's local, at-most-once
reminders to pending approvals, structured questions and other routable requests.
Use the original request; do not infer pending work from stream deltas or turns.

Provider IDs may be reused after resolution. Session Host now exposes optional
`pending_observations`, alongside unchanged `pending_requests`. Each active
request has its original JSON ID, first accepted event sequence and Host wall
clock observation time. An identical duplicate keeps the original occurrence;
local answer or provider resolution removes it. A later request reusing the ID,
even with identical contents, gets a new sequence. Metadata survives eviction of
the ordinary event ring, and is bounded to the existing 64 pending requests plus
64 KiB of serialized metadata. Exceeding the boundary is an explicit error.

The optional field is additive to protocol 3. Older Hosts deserialize without it
and retain the real queue, but do not provide occurrence reminders or observation
age. The field is not durable across Host replacement; incarnation UUID still
separates restarts. Box the Snapshot response in memory as its optional
capabilities grow; serde's wire envelope remains unchanged.

Attention validates complete occurrence metadata before projection. Exact source
confirmation and queue selection include the occurrence sequence when available,
so an old selection cannot silently become an identical later request. The queue
shows time since the **Host observed** the request, not provider creation time.
Missing metadata and a clock ahead of local observation time remain explicit.

Schema 17 adds `pending_alert_cursors` to the private preference store, keyed by
exact configured endpoint and Host incarnation. First contact baselines the
complete pending snapshot silently, including empty snapshots. Later claims use
the full snapshot's event boundary, not the greatest currently pending sequence:
an empty newer observation must prevent an older snapshot from reviving a
resolved request. The cursor never regresses. Requests resolved between polls
produce no reminder. Muted and unroutable requests also advance the cursor;
unmute never replays them. Nothing answers a request or acquires a lease.

Read policy, select eligible occurrences and advance all observed Hosts in one
immediate SQLite transaction. Two windows or processes sharing the preference
directory cannot claim the same occurrence twice. Failure rolls back the whole
pending batch. Keep at most 256 endpoint/incarnation cursors with explicit
capacity refusal; do not evict and replay. Completion claims and pending claims
are independent transactions with independent visible errors.

Desktop's existing bounded two-second inventory supplies both claim APIs off the
render thread. Use at most one replaceable Kit in-app pending toast, containing
only counts of approvals/questions; no prompt, command or path is included.
Clicking opens all-Host pending Attention, resetting Host and kind filters.
It does not take control, select a provider session, answer, or review results.
Existing close, modal and composition guards still apply. Re-observed resolution,
changed occurrence or conflicting mute retracts the pending aggregate. At most
one completion aggregate may coexist with it.

`pending-alerts HOSTS_JSON PREFERENCES_STORE` uses the same collection and claim
engine. It explicitly **consumes local claims**; ordinary `attention` remains
observation-only. Claims precede presentation, so crash/close or a policy/guard
change after commit can suppress a reminder. This is not acknowledged delivery
or a durable inbox. Different preference directories and devices are independent.

Use explicit `NotificationDelivery::InApp` for the transport reasons in ADR 0114.
OS popups, sound, tray/dock, durable delivery, cursor retirement and other-device
coordination remain separate work. Request observation time is not process age
and does not establish how long the model spent generating a question.
