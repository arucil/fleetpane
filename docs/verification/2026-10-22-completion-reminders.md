# Terminal-result reminder acceptance

2026-10-22, Linux x86_64, GPUI Kit 0.7.3, debug binaries, isolated X11 and Mesa
software rendering. Base `859ff20` plus this change. [Decision](../adr/0114-durable-completion-reminders.md),
[receipt](assets/completion-reminders/receipt.json),
[build/artifact hashes](assets/completion-reminders/manifest.json).

## Shared engine and process evidence

Schema 16 adds endpoint/incarnation completion cursors to device-local preferences.
Eight new regressions cover first observation with an empty or nonempty feed,
reopening, out-of-order snapshots, retained-feed changes, exact terminal kinds,
concurrent claimants, every mute scope, reviewed consumption, replacement Host
identity, invalid batches, transaction rollback, corrupt policy, the 256-cursor
limit, schema-15 migration, retained draft/policy and current-schema damage.
Legacy, faulted and unavailable Hosts cannot create an empty baseline.

The real `completion-alerts` CLI uses the same collection and transaction path.
`python3 scripts/completion-alert-smoke.py` creates task-owned Codex/ACP fixtures
with independent pending question/permission and terminal results. First reads
baseline silently; two concurrent processes receive **0 and 2** results in total.
Later reads and reopened processes receive none. Global mute consumes new results;
unmute does not replay them. An injected SQLite update refusal leaves every
cursor unchanged. Removing it allows the two unclaimed results exactly once.
Complete Host snapshots are identical around each read-only claim interval;
both original requests remain pending and failures remain failed/unreviewed.

The CLI explicitly consumes local claims; ordinary `attention` does not. Claims
do not establish system delivery. Review state belongs to the independent review
store, and native uses the latest policy from the preference transaction.

## Linux native fixture evidence

The first native launch observes six old terminal records and two original pending
requests without a toast. Later explicit owned sends produce one Kit aggregate:
**2 new turn results, 1 failed, 0 interrupted**.
[Reminder](assets/completion-reminders/reminder-stable.png).
Clicking it opens all-Host Turn results, retaining failures in red and every result
unreviewed; no control takeover or provider history read occurs for that jump.
[Click result](assets/completion-reminders/reminder-open.png).

Global mute through the shared preference CLI is observed by the desktop: two
new results remain in the queue but produce no reminder and zero alert count.
[Muted results](assets/completion-reminders/muted.png). Unmute restores the count
without a toast. A separate injected SQLite refusal is visible inside Alerts,
while the result queue still refreshes. [Failure](assets/completion-reminders/storage-failure.png).
Removing it restores one eligible aggregate and clears the error on the next
observation. [Recovery](assets/completion-reminders/storage-recovered.png).
Every policy-only/read-only interval compares complete original Host snapshots.

The reminder remains readable at **900×560**.
[Small window](assets/completion-reminders/small-reminder.png).
A later explicit Open source/take-control/history read is recorded separately;
it does not review the failed result. A native composer draft contains Chinese,
a real newline and two trailing spaces: **49 UTF-8 bytes**. Its entire saved
record/revision remains unchanged after normal closure and replacement, and
native Ctrl+L/Select all/Copy reads exactly the original text from the composer.

Results completed while the window was closed are claimed once on the next
launch for this already-baselined Host. A subsequent replacement receives none.
[Offline results](assets/completion-reminders/reopened-offline-results.png),
[next replacement](assets/completion-reminders/reopened-no-replay.png).
The final fixture retains two pending requests and 22 unreviewed terminal records.
No user draft is sent and no result is marked reviewed by these actions.

Early probe clicks missed an autohiding toast and hit the underlying Workspaces
button. The accepted click-through used a newly observed stable toast, then read
the resulting Turn results screen. Initial launch also used an unsupported probe
flag, corrected from current `--help`; neither failure was a product change.

## Installed OpenCodex and DeepSeek Harness

Independent private provider homes each create an observed real session before
the desktop first baselines their supported empty feeds. Both then complete an
actual no-tool model turn with a synthetic requested token; `wait` observes the
same completed turn and explicit history readback contains the token. Native
polling advances each saved cursor to the exact result sequence, and the complete
post-model Host snapshots remain identical during that consumption. The desktop
attention count advances to two without reviewing either result.
[Provider receipt](assets/completion-reminders/real-provider-receipt.json),
[native results count](assets/completion-reminders/real-both-results.png).

The initial real-provider screenshot attempts missed the transient toasts, so
real-provider toast appearance/click acceptance is **not claimed**. The fixture
native workflow above supplies that visual/interaction evidence. An additional
OpenCodex capture turn completed; a later DeepSeek capture attempt failed after
the probe had deleted its copied private credential file. That follow-up
is recorded as failed probe evidence, not a successful model check or a product
fix. No credentials, provider homes or raw private logs enter the public artifacts.

## Checks and limits

`cargo test --workspace --locked` passes **353 tests**; desktop tests pass **44**.
Both strict Clippy checks, formatting checks and builds pass. Process probes pass
for completion claims, original turn results/local review, alert policy, appearance,
preferred Host, last session, recent directories and Paseo import. Logs and binary
hashes accompany the manifest. The final main suite uses its normal test parallelism.
Earlier runs exposed obsolete migration fixture version/table assumptions, corrected
with schema 16. One early file-operations test hit existing global worker admission
while other tests ran; the final default suite passed without changing file behavior.

These are **at-most-once claims**, not guaranteed or acknowledged delivery. A crash,
closed window or active guard after transaction commit can suppress a reminder.
First sight of a Host silently baselines retained history; feed eviction can lose
unseen results. The 256-cursor limit refuses explicitly rather than evicting and
replaying history. Other devices/preference stores are independent.

OS popups, sound, tray/dock, pending-request reminders, durable delivery inbox,
cursor retirement and native acceptance on macOS/Windows remain open. No remote
CI result is inferred from these local checks; CI must be bound to the resulting
commit separately. No performance improvement is claimed. Task-owned
desktops, Hosts and providers are stopped after verification; existing user sessions
remain untouched.
