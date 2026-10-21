# 0050 — Observe pending requests across Hosts

Status: accepted for the local pending-request slice, 2026-10-21.

Per-session review controls already preserve original provider requests and
controller leases. Finding a request should not require opening every Host or
acquiring control. Paseo history 145–150 also illustrates why a manager's cached
state, local permission acknowledgement or restored session configuration does
not establish a provider decision or execution continuity.

## Decision

Keep attention collection in the toolkit-independent session crate. Both
`fleetpane-host attention CONFIG.json` and the native **Attention** screen use
that collector. It probes all configured endpoints, including unopened views,
through the existing bounded Host snapshot API. Configuration order is retained;
the existing eight-Host and 64-pending-request-per-Host limits apply. Unknown
methods remain visible. Unavailable, faulted and malformed Hosts have explicit
errors, including when no available snapshot contains a request.

Each row retains endpoint, Host incarnation, original numeric/string request ID,
method, thread/turn identity when supplied, title and the complete raw request.
The timestamp describes collection, not request creation. There is no inferred
request age, session title or success state. Native rows are virtualized; filters
select approvals, questions or other requests. Collection is single-flight,
asynchronous and repeated every two seconds only while the screen is open.
Rendering performs no Host I/O. Entering the queue blurs the retained composer.

**Open source** rechecks the Host incarnation and entire original request before
selecting its exact configured endpoint. Missing targets and resolved or changed
requests produce errors. Leaving the screen invalidates the navigation token so
a late confirmation cannot switch screens. That guard is implemented; a forced
late-reply ordering has not been accepted in this slice.

Source navigation retains the conversation/draft entity. It never claims a Host,
resumes context or answers. A read-only source requires explicit **Take control**
and **Open pending session**. The latter uses the existing non-resuming Inspect
command and its draft/busy/ownership checks. An already selected session keeps
its history and draft. Decisions remain in the original question/permission UI,
with exact offered options, request ID and fenced controller lease.

## Correct observational connections

Native verification reproduced two pre-existing connection defects. Connecting
to an unowned Host implicitly claimed it; a view without a controller connection
also skipped all polling and retained resolved requests indefinitely. Claim now
requires explicit takeover. Non-owning views periodically read snapshots without
history reads or provider actions. One observation is in flight at a time, and
the existing revision fence discards results superseded by a command or connect.
Incarnation changes clear old review entities; observation failures have their
own visible error and successful observation clears that error only.

This polling keeps request/ownership state fresh; it does not reload read-only
conversation history or import provider transcripts. Existing owned event polling
and Host lifecycle remain independent of the window.

## Verification and limits

See [engine, process and native acceptance](../verification/2026-10-21-attention-queue.md).
Tests cover cross-Host colliding IDs, original request validation, unknown
methods and unavailable/malformed snapshots. A real-process fixture holds
requests for 125 seconds, rejects stale/duplicate decisions and reads the exact
provider answer. Native acceptance covers observational navigation, external
resolution, original approval/question controls and exact draft retention.

This is a partial implementation of PRD ATT-01–05: pending requests only.
Completion/failure/unread queues, persisted request times, keyboard-next,
notification deduplication, configurable mute/tray/dock/sound, remote transport
and background alerts remain separate work. There is no new provider lifecycle
daemon, automatic approval/rejection timeout or automatic session recreation.
