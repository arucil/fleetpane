# Durable terminal-result reminder claims

Accepted 2026-10-22. ATT-05 requires replay and reconnect not to repeat completed
turn notifications. Build on the Host's explicit terminal feed, device-local
alert policy (ADR 0111/0113), and separate controller review receipts. Do not
turn stream deltas, a read operation, or a process exit into invented completion.

The shared session engine accepts bounded complete retained feeds for up to eight
available Hosts. Each feed has the exact configured endpoint and Host incarnation
UUID; each result carries its original validated sequence and turn identity.
Unavailable, faulted and legacy Hosts without a supported feed do not manufacture
empty observations. A supported empty feed does establish a baseline.

Schema 16 adds `completion_alert_cursors` to the existing private local store.
For each endpoint/incarnation, the first observation silently establishes the
highest retained sequence. Later observations claim only higher sequences. The
cursor advances over muted and reviewed records too, so unmute does not replay
them. Older observations never move the cursor backwards, and controller/desktop
replacement retains it. Replacement Hosts use an independent baseline. Gaps or
Host feed eviction cannot reconstruct lost records or establish delivery.

Read the latest local policy, select eligible records and advance every cursor
in one immediate SQLite transaction. Concurrent windows/processes sharing that
directory cannot claim the same result twice. Different preference directories
or devices remain independent. A failed write rolls the whole batch back and
produces an inspectable error; it does not hide the true queue. Review receipts,
pending requests and Host state are independent and unchanged by claims.

Claims precede presentation: this is **at most once**, not proof of delivery.
A process/window closing after commit, OS termination, a concurrent save/error
guard, or a newer policy can suppress a claimed toast. Prefer that limitation
over replaying alerts. No claim is marked reviewed. Keep at most 256 saved
endpoint/incarnation cursors; exhaustion refuses the whole transaction with a
visible error. Do not evict cursors and silently replay history. Cursor pruning,
an inbox and acknowledged or cross-device delivery need a separate contract.

Desktop uses the existing two-second bounded Host inventory. The GUI aggregates
new eligible results into one replaceable GPUI Kit toast with only counts of
completed/failed/interrupted results. It contains no prompt, path or provider
payload. Clicking opens all-Host Turn results without claiming control, opening
a provider conversation or reviewing any outcome. Existing modal, composition,
export and close guards apply. A newly observed conflicting mute retracts the
aggregate; unmute never resurrects it. Policy changes in other windows are
observed asynchronously, as with existing attention counts.

Use Kit's explicit `NotificationDelivery::InApp`. Inspection of the installed
GPUI Linux 0.3.9 implementation shows no sound control, no retained dismissal
handle and one blocking action waiter per OS notification. Its default transport
does not satisfy complete mute/retraction or the bounded notification workload.
This is why OS delivery is still a separate gap. No OS popup/sound/tray/dock or
pending-request notification delivery is added or claimed here.

The `completion-alerts HOSTS_JSON PREFERENCES_STORE [REVIEW_STORE]` CLI routes
through the same collect/project/claim API. It explicitly mutates the local
notification cursors; ordinary `attention` remains observation-only. This
provides process-level evidence of concurrency and storage rollback without a
second implementation. It must not be described as a dry-run command.
