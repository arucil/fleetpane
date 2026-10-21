# Terminal fault-domain verification

2026-10-21, Linux; base `c91d269247207a08898e264c81083df4fbd4e745` plus earlier
uncommitted work and this change. No commit, push, remote CI or release claim.

## Reproduction and resulting behavior

Changed the existing real-PTY smoke to require Close after killing its private
fixture provider. The previous executable failed with `provider_unavailable:
provider output closed`; the [before log](assets/terminal-faults/before.log)
records the assertion. Shell ownership was correct; shared fault admission and
the journal's indiscriminate Unknown transition were the root cause.

[ADR 0029](../adr/0029-independent-terminal-fault-domains.md) separates provider
and Host/storage faults while preserving controller leases, exact operation UUID
deduplication, persistent drain and uncertain-input fences.

`scripts/terminal-smoke.py` now uses two independently owned Hosts, one Codex and
one ACP protocol fixture, with real `/bin/sh` PTYs and no external model calls:

- Saturate a non-reading PTY, verify its input receipt is still Accepted, kill
  only the fixture provider, then observe that terminal receipt finish rather
  than become Unknown. Its partial-write input fence remains visible.
- After provider failure, read the same shell/PID, resize, rename, input/execute
  a literal command, recover a deliberately lost reply and resubmit the same
  operation UUID. The command appends exactly once.
- Stale controller leases and unsupported terminal-prefixed methods remain
  rejected. Drain still rejects terminal creation and permits existing input.
  Ready allows a new terminal; explicit Close terminates owned shells.
- Take an exclusive SQLite lock to cause an actual journal lookup failure.
  The mutation never reaches the terminal; unlocking storage does not reopen
  admission. Snapshot escalates to Host fault and Close/admission changes remain
  fenced. Local output is still readable; explicit Host teardown cleans up.
- Retain earlier cwd, Unicode, resize, cursor-loss, bounded-input, capacity,
  foreground-close, process-exit and workspace-identity checks.

Three new Rust regressions cover scoped resident/durable Unknown transitions,
terminal completion after provider death, exact-target completion authorization,
unknown-method default ownership, stale leases and non-downgradable Host faults.

## Verification

Commands and source/binary/log hashes are bound in the
[receipt](assets/terminal-faults/receipt.json), alongside raw outputs:

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build -p fleetpane-host --locked
cargo fmt --all --check
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
python3 scripts/terminal-smoke.py
python3 scripts/session-smoke.py
python3 scripts/acp-smoke.py
python3 scripts/drain-smoke.py
python3 scripts/journal-smoke.py
python3 scripts/query-smoke.py
python3 scripts/runtime-smoke.py
python3 -m unittest discover -s scripts/tests -p '*_test.py'
uvx --offline ruff check scripts
uvx --offline ruff format --check scripts/terminal-smoke.py
git diff --check
```

104 workspace Rust tests, 7 desktop Rust tests and 13 Python reader tests passed.
These checks cover the changed engine/CLI contract; no native terminal renderer
was added or visually accepted here. No real OpenCodex/DeepSeek model run,
macOS/Windows runtime, full descendant sandbox, provider recovery, PTY Host
restart/handoff or performance acceptance is established.

Continuous selected-path Paseo review now reaches ordinal 34
`af67235997052b435ab3c2db1a9e50b0991fe8ab`; next is ordinal 35
`c5b24d3dfc7b4b55f951e5b7809f8e95374c2fa5`. The
[ledger](../reference/paseo-replay.json) names inspected paths, separate from
the 6,187-entry index. No all-file, dependency-payload or historical-runtime audit
is claimed. Artifact/voice additions are recorded; voice stays P2 and Git Diff
expansion stays deferred as instructed.
