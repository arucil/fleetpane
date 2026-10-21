# Provider session settings — 2026-10-22

Candidate on base `96d61a6438f1db353dbcc42862d4013ddeadb6d3` preserves and shows
provider-reported Codex model settings. [ADR 0081](../adr/0081-provider-session-model-settings.md)
defines the boundary. [Evidence](assets/session-settings/receipt.json) includes
binary/schema hashes, synthetic snapshots and an isolated installed-provider probe.

## Checks

Two regression tests fail before the implementation: metadata serialization
loses the model and the settings event is ignored. Both pass afterward. They
cover exact strings, foreign-session isolation, null effort, malformed/empty/
oversize fields and atomic refusal. All 258 core tests and 33 desktop tests pass,
with strict core/desktop Clippy, Windows cross-target core Clippy and native build.
The desktop metadata test verifies settings events do not rebuild content cards.
Existing 71-model capability and 5.2 MB history process smokes pass unchanged.

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu --locked -- -D warnings
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
python3 scripts/models-smoke.py
python3 scripts/history-smoke.py
```

Installed `codex-cli 0.163.0` regenerated its current JSON schema. A task-owned
app-server with private CODEX_HOME created a session using explicit model
`local-settings-probe` and high effort. The actual response contains both fields;
no prompt was sent and no external inference was tested. The process was stopped.

## Linux native interaction

Hidden workspace `fleetpane-settings-20261022` renders synthetic saved settings
and copies their exact 188-byte JSON. Sending `[settings-update]` emits a settings
event, clears effort and also emits a foreign-session event. The view displays
only the selected session's confirmed update and copies its exact 174-byte JSON.
The provider intentionally differs from catalog/default choices. Switching to
another session with missing metadata shows unavailable values and its original
question; it does not inherit the first session's settings.

A 31-byte multiline Chinese draft survives session switches and native window
replacement. Reopened Copy settings and draft clipboard match exactly. The
900×560 view keeps copied settings and Send reachable. The original Host UUID,
provider PID and complete independent pending request remain unchanged across
the tested interactions. [Clipboard readbacks](assets/session-settings/clipboard.json)
and screenshots are retained alongside the receipt.

Two initial fixture restarts were refused: one lacked its workspace directory,
then one changed the journal's workspace binding. The fixture was corrected to
reuse its original directory; neither refusal was hidden or changed in product
code. Initial tool arguments were also corrected before native interaction.
The task-owned desktop and Host were stopped; user runtimes were untouched.

## Limits

These values are current/persisted provider settings. They do not prove the model
that executed an individual turn. No new external-model, ACP configuration,
cross-platform native, IME or performance acceptance is claimed. Remote CI for
this new revision is separate from the previous revision's CI.
