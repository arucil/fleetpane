# Last viewed session recovery — 2026-10-22

Covered shared/CLI and Linux X11 fixture workflows pass locally. Each exact
endpoint remembers its Host UUID, session ID, directory and archive membership.
Startup renders a saved choice without taking control or opening history. The
explicit action revalidates bounded metadata membership and uses Inspect with
resume=false; it never restores provider context or sends a prompt.

`cargo test --workspace --locked` passes 298 tests. The separate desktop suite
passes 37, including attempt-consumption regression coverage. Both formatters,
strict workspace/desktop Clippy and builds pass. Store/observation smoke scripts
cover Unicode/reopen, endpoint isolation, competing writes, injected write
failure, corrupt record preservation, schema-12 migration, third-page active and
second-page archived targets, changed cwd/archive membership/Host and missing
session refusal. Appearance, preferred Host, recent-directory and offline Paseo
migration smokes also pass with schema 13. [Logs/digests](assets/last-session/receipt.json).

The owned native fixture used two Codex Hosts with 237 active and 237 archived
sessions each, an ACP Host waiting for permission, an unowned Host and an offline
endpoint. After an explicit takeover, Open last session rendered the original
Codex A same-001 history and recovered the exact 40-byte Chinese draft including
two trailing spaces before its newline. Clipboard readback and 900×560 rendering
confirmed the draft; no Send was used. Startup snapshot readback was completely
unchanged. Explicit inspection changes read journal sequence; takeover changes
the selected Host owner only. Independent Hosts retain complete snapshots,
including the original ACP permission and active turn.

A separate CLI writer installed revision 2 while the native view retained
revision 1. Selecting another session produced a visible conflict without
overwriting revision 2. Normal titlebar close refused with the explicit error.
Reload choice retained the current selection; explicitly opening the saved
same-002 source then allowed normal close with exit 0. A subsequent launch used
an explicitly prepared archived-120 choice: its second-page membership and
original archived history rendered, with read-only notice and disabled Send at
900×560. Normal titlebar close again exited 0. The independent Host/provider
PIDs, pending requests, activity, faults and mutation IDs remained unchanged.

Earlier low-level close-window automation destroyed windows without graceful
client exit; those owned remnants were stopped during isolated-workspace
cleanup. They are not counted as normal-close acceptance. The final titlebar
closes above have explicit process exit readback. Fixture processes and hidden
desktop were stopped after evidence capture.

No installed-provider, macOS/Windows, IME, remote CI, automatic context resume,
scroll/layout recovery or measured performance acceptance is claimed.
[Architecture](../adr/0096-explicit-last-session-recovery.md).
