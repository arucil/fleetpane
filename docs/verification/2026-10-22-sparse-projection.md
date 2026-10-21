# Sparse conversation projection — 2026-10-22

The candidate on base `0668fb4b1d985602b26b7e97b31f7d70f943fa49` projects only
accepted content-event targets during polling. It preserves the full projection
for structural changes. [ADR 0079](../adr/0079-sparse-conversation-row-projection.md)
defines the boundary. The [receipt](assets/sparse-projection/receipt.json) records
source/binary hashes, logs, synthetic captures, clipboard readbacks and cleanup.

## Checks

All 32 desktop tests pass. Four new tests compare the incremental result to the
same full projection used for history/reload. A 400-item fixture applies two
Unicode/Markdown deltas per poll for 40 polls: exactly one item is projected per
poll and 400 other rows retain their Copy string allocations. Additional cases
cover hidden/visible reasoning, tool failure and raw `false`, malformed known
content, structural completion, metadata, foreign events, history rejection and
opaque IDs containing separators.

A negative control temporarily routes item invalidations through the legacy full
reconciliation policy. The targeted sparse-projection test fails. This is a
policy negative control, not a test run against the unchanged parent source.
The source was restored and all 32 tests passed afterward.

Strict desktop Clippy, the desktop build, 15 Python history-tool tests and the
history/ACP-tool process smokes pass. The latter verify bounded multi-megabyte
history and exact ACP replacement/clear/falsy/restart behavior with private
fixtures. Core sources did not change; the preceding core results are not
reported as a fresh core test run here.

```sh
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
python3 -m unittest discover -s scripts/tests -p '*_test.py'
python3 scripts/history-smoke.py
python3 scripts/acp-tools-smoke.py
```

## Native observation

Two synthetic providers and independent Hosts ran in hidden workspace
`fleetpane-sparse-20261022`, with a 1440×900 display. No user session or external
model was used. The Codex fixture supplies retained command/Markdown content and
gated reasoning deltas. The ACP fixture supplies gated tool patches.

Reasoning becomes visible, retains its expanded state through later deltas and
copies both summary parts and raw detail exactly, including whitespace: 141
UTF-8 bytes. The ACP tool retains its expanded state while moving from pending
to in-progress. Its Copy payload changes from the exact 598-byte intermediate
object to the exact 283-byte cleared/completed object with empty collections and
numeric-zero output. A separate failed tool displays red and copies its original
224-byte object with `rawOutput: false`. These reads exercise the native Copy
buttons, not an API approximation of their output.

Independent 48-byte Codex and 35-byte ACP multiline Chinese drafts survive Host
switches and native app replacement. After explicit takeover/reopening, native
clipboard readback matches both original drafts. Reopened Codex command Copy
retains its 179-byte source object. A 900×560 ACP window displays the failed tool,
draft and Send control. Source Host IDs and provider PIDs remain unchanged across
the desktop replacement; both providers complete their explicitly gated turns.

One earlier fixture configuration omitted its required version and the initial
desktop launch correctly exited 2; that fixture was corrected before acceptance.
Several immediate or misplaced test clicks did not send/copy the intended target;
only the recorded successful clipboard readbacks above count as evidence. A CLI
read explicitly acquired the Codex control lease during observation, so its old
native view correctly became read-only and retained a previous-observation banner.
The later explicit takeover and reopen refreshed the completed conversation.

The hidden workspace was stopped and all task-owned Host/provider/desktop PIDs
were confirmed gone. Window close requests preceded teardown; their immediate
readback still reported the apps running, so this record does not claim normal
desktop exit codes. Cleanup never touched the user's desktop or running sessions.

## Limits

This is Linux native fixture behavior, operation-count and exact-content
evidence. It does not measure latency, CPU, total memory, real fleet capacity or
a release-build comparison with Paseo. There is no new IME, notification, remote,
cross-platform native or external-model acceptance. Remote CI for the new commit
must be checked separately from the passed parent run `39646024228`.
