# Terminal turn results — 2026-10-22

Candidate on base `ade3216f3b133e96154fd949b9a8e9933385d229` adds bounded Host
completion observations and exact local review receipts. [ADR 0080](../adr/0080-terminal-turn-results-and-local-review.md)
defines the contract. The [receipt](assets/turn-results/receipt.json) retains
binary hashes, before/after synthetic snapshots, saved reviews and cleanup.

## Engine and process checks

All 256 core tests and 33 desktop tests pass. Core and desktop strict Clippy,
desktop build and Windows cross-target core Clippy pass. Completion tests cover
event expiry, exact duplicate observations, conflicting outcomes, encoded-byte
eviction, unsupported old snapshots, identity isolation, schema-11 upgrade,
transactional failure and reopening saved reviews. A targeted negative control
restores GPUI's reveal helper: the source-navigation test fails because Tail
following remains active. Exact source was restored and all desktop tests pass.

The owned process smoke runs Codex and ACP fixtures. It observes completed,
failed, interrupted and refusal outcomes, leaves an offline Host explicit,
marks one exact record and four ACP records, tests idempotence, injects a SQLite
write failure and verifies rollback. Complete Host snapshots stay unchanged by
observation/review, including pending questions and permissions.

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu --locked -- -D warnings
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
python3 scripts/completion-smoke.py
```

Logs are retained beside the receipt. CI adds this process smoke for Unix runners;
these local results do not establish the new commit's remote CI result.

## Linux native interaction

Hidden workspace `fleetpane-results-20261022` ran two private synthetic Hosts and
one offline endpoint. The result view renders statuses, ACP stop reasons, original
Host/session/turn identities and unreviewed counts. Single review and Host-filtered
batch review retain the red failed outcome. Native Copy record returns the exact
393-byte original failed record, recorded in [clipboard evidence](assets/turn-results/clipboard.json).

Opening a source leaves control acquisition explicit. After takeover, the normal
history path renders the original failure; loading an older turn and opening the
result again keeps that turn anchored. The accepted frame is
[19-anchored-failure.png](assets/turn-results/19-anchored-failure.png).
A 900×560 window was exercised. Both independent pending requests remain visible
on returning to Attention. A 36-byte multiline Chinese draft survives source
navigation and native window replacement; the reopened native clipboard matches
exactly. Five review receipts also survive replacement. Source opening changes
only snapshot owner and event sequence: Host/provider identity, activity,
operation journal, admission, pending requests and completion feed are unchanged.

The hidden desktop was stopped (SIGTERM); no normal app-exit claim is made.
Both task-owned Host process groups were stopped and confirmed gone. Existing
user runtimes and the user's display were untouched.

## Limits

This is synthetic Linux native/process acceptance, without a new external-model,
IME, notification, cross-platform native or performance claim. Records describe
the current Host run and may be evicted. Source history remains bounded and may
need manual older-page loading. Host restart does not restore this result feed.
