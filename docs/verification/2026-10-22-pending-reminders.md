# Pending request reminder acceptance

2026-10-22, Linux x86_64, debug binaries, GPUI Kit 0.7.3, isolated X11/Mesa
software rendering. Base `3ac1ed2` plus this change.
[Contract](../adr/0115-pending-request-reminders.md),
[receipt and build hashes](assets/pending-reminders/receipt.json),
[artifact manifest](assets/pending-reminders/manifest.json).

## Engine and process evidence

Host regressions cover exact duplicates, provider resolution, local answer,
identical ID reuse after resolution, ordinary event-ring expiry and old snapshots
without optional metadata. Attention confirms the exact occurrence and validates
metadata before projection; observation labels distinguish unavailable and
clock-ahead times. Selection clears when the occurrence changes.

Six pending-claim regressions cover nonempty/empty first baselines, reopening,
out-of-order reads, a newer empty snapshot before a stale pending one, replacement
incarnation, concurrent store handles, every existing mute scope, unmute without
replay, invalid batches, injected writes, corrupt policy, atomic capacity refusal,
legacy/faulted observations, schema-16 upgrade, retained completion cursors/drafts
and damaged current schema. The 256-row limit rolls an earlier update and a new
baseline back when a later insertion cannot fit.

`python3 scripts/pending-alert-smoke.py` exercises real owned Codex/ACP fixture
processes through the new CLI. Two competing CLI processes claim **0 and 2**
requests, spanning approval/question with original sequences/times. Reopening
claims none. Global mute consumes new requests and unmute does not replay them.
An injected update refusal preserves every saved cursor; removing it claims the
two requests once. Complete Host snapshots are identical around read-only claim
intervals. A separate explicit permission answer removes its original occurrence,
without repeating remaining reminders. [Process result](assets/pending-reminders/process.json).

## Linux native fixture evidence

Existing pending requests baseline silently. A later new request produces a
Kit reminder; [rendered approval](assets/pending-reminders/reminder.png) and
[click](assets/pending-reminders/reminder-click.png) show the pending queue.
Host observations remain unchanged by claiming or opening Attention. Queue rows
show the original requests and time since Host observation. Nearby asynchronous
arrivals can fall in separate polls and replace the aggregate; the toast counts
describe its latest claimed batch, not the whole outstanding queue.

Global mute suppresses new reminders while the real pending queue grows and its
rows remain actionable. [Muted queue](assets/pending-reminders/muted.png).
Unmute does not replay them. A real SQLite trigger refusal is visible inside
Alerts while the original queue still refreshes.
[Failure](assets/pending-reminders/storage-failure.png).
Removing it yields one aggregate for the two unclaimed occurrences and clears
the error. [Recovery](assets/pending-reminders/storage-recovered.png).
Full snapshots remain identical over that failure/recovery observation interval.

At **900×560**, the reminder and pending controls remain readable.
[Small reminder](assets/pending-reminders/small-reminder.png).
[Small pending view](assets/pending-reminders/small.png).
Normal close/replacement does not replay claimed requests.
[Replacement](assets/pending-reminders/reopened.png).
After explicit take-control/open-last-session, the original native composer
retains **44 UTF-8 bytes**, Chinese, one real newline and two trailing spaces;
its saved record and revision are identical and native Select all/Copy returns
the exact text. [Draft](assets/pending-reminders/draft.png),
[replacement draft](assets/pending-reminders/draft-reopened.png).
The draft is never sent. Those explicit source reads/takeovers are separate from
the snapshot-preserving reminder intervals.

Early captures ran before a frame or missed a short-lived toast. The probe also
initially compared an intermediate provider sequence for exact equality; it now
waits for the new request and a cursor at or beyond that observation. No product
retry or timeout was added for that instrumentation issue.

## Installed OpenCodex and DeepSeek Harness

Two independent private provider homes create sessions before native empty-feed
baseline. Actual model turns each produce an approval: OpenCodex
`item/commandExecution/requestApproval` and DeepSeek `session/request_permission`.
Both use original request ID **0** and remain distinct by endpoint/incarnation.
Native saved cursors match Host occurrence sequences **28** and **9**; complete
pending Host snapshots remain identical during native observation. Both requests
and their observation ages render independently in Attention.
[Native real requests](assets/pending-reminders/real-requests.png),
[sanitized provider receipt](assets/pending-reminders/real-provider-receipt.json).
Real-provider transient toast captures are not accepted; synthetic native evidence
above establishes toast appearance and click behavior. No new real structured
question/answer or approval-decision acceptance is inferred here.

## Checks and limits

`cargo test --workspace --locked`: **361 pass**. Desktop tests: **44 pass**.
Both strict Clippy checks, both fmt checks, `git diff --check` and both builds pass.
Process checks also pass for completion claims/results, policy, original Attention,
ACP cancellation, appearance, Host/last-session/recent-directory stores and Paseo
import. [Check results](assets/pending-reminders/checks.txt).
One early full-suite attempt hit the unchanged scratch writer/collector test's
OS lock refusal; subsequent default-parallel suites passed without altering that
module. Early compile feedback required explicit boundary-error conversion,
boxing the enlarged response and updating schema fixtures; warnings were not
suppressed. CI is tracked separately against the resulting commit.

At-most-once local claims can be lost after commit. First contact is deliberately
silent; requests resolved between polls cannot produce a reminder. Legacy Hosts
keep their queue without reminders/age. Observation times are wall-clock values.
OS output/sound/tray/dock, acknowledged/cross-device delivery, cursor retirement,
other platforms' native QA and performance comparisons remain open. All task-owned
desktops, Hosts/providers and copied private credentials are cleaned after QA;
existing user sessions remain untouched.
