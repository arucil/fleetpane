# Exact-turn waiting during first-turn startup

2026-10-22, Linux CLI/Host on base `b0a3e07`. The earlier
[workspace-tab run](2026-10-22-workspace-tab-navigation.md) exposed installed
Codex 0.163.0 returning `list_turns is not supported yet` when waiting immediately
after sending. [Receipt](assets/wait-first-turn/receipt.json) binds the final
source and Host binary, commands, preserved raw logs and owned process readbacks.

## Cause and correction

Two distinct ordering boundaries caused the failure:

- The wait implementation always queried paged history, even when the exact
  thread/turn was already running in the Host snapshot. Codex's first-turn
  history can remain unavailable while its startup work proceeds.
- The installed Codex returns the in-progress `turn/start` reply **before** its
  `turn/started` notification. The old Host activity tracker watched only
  notifications. Captured sequence 5 contains the successful exact start reply,
  yet the old snapshot still has no active turn. A snapshot-only wait correction
  therefore still failed in the real process check.

The Host now associates a successful start reply with the admitted thread. It
retains completion identities only while that start awaits its reply, with an
explicit bound; a delayed stale reply cannot reactivate a completed turn or
replace a newer turn. Exact active waits use Host snapshots/events directly.
Other/missing/incomplete identities retain the existing bounded history lookup.
No provider retry, fallback full-history read or error suppression was added.
The protocol and journal schemas are unchanged. Existing running Hosts need a
separate explicit upgrade to get the new activity behavior; no shared Host was
restarted for these checks.

## Regression and process evidence

The active-identity regression failed before the wait change. After that change,
the installed Codex still failed; a separate runtime regression reproduced its
empty activity after a successful start reply and failed before the Host fix.
Both now pass. Ten wait tests and the runtime suite cover exact identity,
old-target lookup, fault/timeout, controller replacement, completion-before-read,
completion-before-start-reply, newer-turn preservation, invalid replies, rejected
starts, repeating cursors and bounded retention.

`python3 scripts/wait-smoke.py --real-codex --evidence-dir PRIVATE_DIRECTORY`
passes with the final binary. It covers Codex/ACP protocol fixtures, failures,
refusal, target-only attention, observer SIGINT, explicit Stop/control change,
provider/Host loss and ACP unknown history after restart. Installed Codex uses
one gated loopback SSE response: immediately after Send, its exact active
identity is present, a 75 ms wait times out without mutation, then wait reports
the completed turn and a settled history read returns the same identity. Exactly
one model request is made. The gate test uses a synthetic model response.

Separate **real model** checks use installed Codex 0.163.0 through the existing
OpenCodex loopback proxy (`gpt-6.1-sol`) and DSH 0.2.0-rc.2 through ACP
(`deepseek-v4-flash`). Each uses a private provider home and synthetic workspace,
with two explicitly sent turns. All four immediately started waits finish with
the exact expected assistant token. Each completed target can be queried again;
while the second turn runs, waiting on the first still returns the first turn.
Before/after snapshots retain Host/provider PIDs, owner, journal count and
operation IDs, pending requests and admission state. Read queries may advance
the event sequence. The initial live-check extractor assumed Codex agentMessage
for ACP too; native ACP content is acpMessage. Correcting the test extractor and
rerunning the four turns passes; no product rendering/history change was needed.

All owned observer/Host processes terminate. Provider PIDs are gone or zombie,
sockets are removed and the private DSH credential copy is deleted. Existing
Paseo sessions, provider homes and the shared proxy are unchanged.

## Checks and limits

Passed:

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build -p fleetpane-host --locked
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
python3 scripts/wait-smoke.py --real-codex --evidence-dir PRIVATE_DIRECTORY
python3 scripts/runtime-smoke.py --codex --drain
```

327 workspace tests and 41 desktop tests pass. Desktop compilation/tests cover
the shared activity consumer; no new native UI or visual acceptance is claimed.
Remote transports, other provider versions, legacy/unavailable history storage,
expired cursors, sustained load and other platforms remain outside this run.
No performance, seamless Host update, hosted CI or release claim.
