# Local alert policy acceptance — 2026-10-22

Implemented and verified locally: global, Host, exact session and event-type mute,
private persistence, shared CLI and native attention counts. This is partial
ATT-04/05; OS notification, sound and tray/dock delivery are not implemented.

Base `193e8e292ff33107c826a9add9132908c8be922a`; first native-run desktop SHA-256
`39e16ae0072b2c68392c074f54449ff0d008811ac1c062775efeabd8a151bbbd`.
The then-running `/proc/4093786/exe` and built file matched. All providers in this run
are synthetic Codex/ACP fixtures, independently hosted in task-owned directories.
No external model call or installed-provider acceptance is implied.

The isolated `fleetpane-alert-policy-20261022` X11 workspace covers:

- Four original pending requests on three Hosts (including colliding numeric ACP
  request IDs) and two explicit Codex results: one completed and one failed.
  The all-enabled count is six; global mute removes the count while preserving
  all six source observations. The existing Host status still describes its real
  pending state. It is not an unread notification badge.
- Event-type mute excludes the three approvals independently. Muting Codex
  leaves the other Hosts' two approvals counted. Exact session mute excludes its
  question only, producing five alerts; session unmute restores it. Scope changes
  and original identities are checked through private-store readback.
- Global mute preserves narrower rules on unmute. The original queue filters,
  Copy and Next request remain independent; Ctrl+Shift+J opens the exact muted
  question source and still requires explicit takeover and original answering.
- The failed result remains failed and unreviewed while muted. Zero local review
  receipts are present. Snapshot host/provider PIDs, operation IDs/journal total,
  original pending payloads, outcomes, activity and admission remain unchanged;
  explicit conversation reads/control gestures can change observation sequence
  and controller ownership. A full snapshot equality check also passed before
  those gestures.
- Reopening the desktop retains policy and the original session identity. A
  44-byte Chinese multiline draft, including both final spaces, survives normal
  Alt+F4 replacement; native clipboard readback and local stored text agree.
- Real SQLite trigger failure rejects a native save, retains the previous
  revision and displays an error. Alt+F4 keeps the window open with the exact
  failure reason. Removing the task-owned fault and explicit Reload alerts
  clears both notices; normal close succeeds. `workspace_close_window` forcibly
  removes the X window and is not normal-close evidence.
- 900×560 supports alert settings and retained source navigation. Collapse Alerts
  for the full queue/conversation area; the panel is bounded and scrollable. The
  final header reuses the existing Host preference row, preserving conversation
  space. No actual IME or keyboard-to-terminal acceptance in this slice.

[Native screenshots, readback and receipts](assets/notification-policy/receipt.json)
retain synthetic evidence and tested binary identity. The readback file includes
local fixture paths and exact synthetic IDs, not credentials or real sessions.
Owned desktop/Host/provider processes terminate and residual task-owned sockets
are removed after verifying Host exit. The isolated workspace is stopped.

After the final Unix-only module guards, the final desktop SHA-256 is
`f6f25cbc9e892cd8cce8feff8d65d96671b282dd9ff37dd873b8993d77e0ceaf` and
Host SHA-256 is
`796eb34adc050a7a76ca47bd1514ca0427717c75f68ca4ddf713dc25a31032d4`.
A fresh `fleetpane-alert-final-20261022` workspace reran global/event/Host/session
mute, failed-save revision retention, normal Alt+F4 failure guard, explicit reload,
unreviewed failed/completed results, normal replacement and 900×560 settings.
The running desktop executable matched the final built file. All three complete
Host snapshots, including owner and operation identities, equal the pre-UI
baseline; four pending requests and two unreviewed results remain unchanged.
[Final-binary receipt](assets/notification-policy/final-binary-receipt.json) and
[readback](assets/notification-policy/final-binary-readback.json) distinguish this
rerun from the earlier draft/Next-request coverage. Both desktop processes, all
three Hosts and providers exited; sockets are absent and the workspace stopped.

Passed commands:

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check --workspace --target x86_64-pc-windows-gnu --locked
cargo build -p fleetpane-host --locked
python3 scripts/notification-policy-smoke.py
python3 scripts/session-smoke.py
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
git diff --check
```

339 workspace and 43 desktop tests pass. Eight policy tests cover additive scopes,
endpoint/incarnation collisions, exact outcome/payload preservation, strict JSON,
schema-14 migration, size/capacity errors, revision exhaustion, concurrent writers,
failed storage and corrupted current schemas. A desktop state regression covers
late background reads during saves and explicit error acknowledgement. Existing
old-schema fixtures now remove the new table when constructing earlier schemas.
The Windows target check covers the shared workspace only, not the desktop or
Windows runtime. The CLI smoke validates persistence, no Host dependency, stale writes and invalid
input rejection before opening storage. Existing session smoke rechecks offered
decisions, questions and explicit request answers through the Host.

Still open: workspace-specific rules, persistent delivery/deduplication, request
creation times, OS popup/sound/tray/dock, actual notification permission/dismissal,
macOS/Windows, hostile endpoint timing, performance and hosted-CI acceptance.
All-Host polling is bounded by the existing eight-endpoint snapshot contract;
large real fleet overhead remains unmeasured. No Session Host upgrade continuity,
Paseo runtime takeover or replacement-readiness claim.
