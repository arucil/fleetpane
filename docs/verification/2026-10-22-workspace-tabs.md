# Workspace tabs and bounded layout restoration

2026-10-22, Linux x86_64 isolated X11 workspace, GPUI Kit 0.7.3, debug unoptimized
build on source base `97bf5f7` plus the workspace-tab working tree. This accepts
the workflows below, not full Paseo parity or another platform.
[Identity and state receipt](assets/workspace-tabs/receipt.json).

The [catalog](assets/workspace-tabs/catalog.png) now groups its loaded workspaces
by project. [Open workspace](assets/workspace-tabs/catalog-detail.png) selects
an existing tab or creates a tab with an exact registered root and Host source;
there are at most 32. Tabs retain Conversation/Files/Terminal views independently.
Schema 14 saves tab order/selection, selected session, file path, selected terminal
and sidebar size/visibility. It preserves the existing independent draft store.

## Native and installed-provider observations

Independent private provider homes used installed Codex 0.163.0 through the
existing OpenCodex loopback proxy (`gpt-6.1-sol`) and DSH 0.2.0-rc.2
(`deepseek-v4-flash`). Three real completed sessions returned synthetic tokens:
OpenCodex alpha/beta and DeepSeek alpha. The alpha directory is deliberately the
same path across both Hosts; its workspace UUID, source and history stay distinct.
No existing user's home/session or shared Paseo daemon was changed.

- [Reopened alpha](assets/workspace-tabs/reopen.png),
  [beta](assets/workspace-tabs/beta.png) and
  [DeepSeek](assets/workspace-tabs/deepseek.png) recover the correct history.
  Exact native clipboard copies after replacement are 38, 37 and 43 UTF-8 bytes:
  `OpenCodex ALPHA 中文草稿\n第二行`, `OpenCodex BETA 独立草稿\n第二行`,
  `DeepSeek 中文草稿，仅保存\n不发送`; here `\n` denotes an actual newline.
  SQLite contains the same three independently keyed strings.
- Alpha restores its 400 px sidebar, while beta and DeepSeek retain 320 px.
  Generic Host width remains window-local. [900×560](assets/workspace-tabs/small.png)
  retains tabs, panel selectors, composer and Send.
- [Saved file selection](assets/workspace-tabs/files.png) reopens `notes.md`
  and freshly reads its exact workspace root. This does not persist editor mode,
  source Find, file navigation history, scroll or zoom.
- Explicit creation/input starts [one real shell](assets/workspace-tabs/shell.png).
  A later desktop [restores Terminal](assets/workspace-tabs/terminal-reopen.png)
  with the same shell PID 2658954, UUID and input sequence 1, retaining
  `REAL-TAB-PTY-ALIVE`. Host/provider PID, owner, pending requests, activity and
  journal totals match before/after. Reopening does not claim or write input.
- A second shell command sleeps 10 seconds before creating a marker. Its tab is
  removed while the marker is absent. The command finishes afterward with the
  exact marker and same live shell, while [other tabs remain open](assets/workspace-tabs/tab-closed.png).
  [Readback](assets/workspace-tabs/tab-close.json) binds the tab-removal observation
  and later marker timestamp. Closing a tab sends neither Stop nor terminal Close.
- One explicit claim of the Codex Host is shared across both Codex tabs, avoiding
  mutual ownership revocation. Separate DeepSeek control remains explicit.
  Before terminal creation, native navigation/replacement keeps Host/provider IDs,
  pending requests, active work and mutation journal counts unchanged; history
  query sequence numbers may advance. This does not accept provider resume,
  approval answering or sends during this particular tab check.

## Failure behavior and regression checks

An injected SQLite update trigger fails layout saving after a tab switch.
The [visible error](assets/workspace-tabs/save-error.png) retains the entire
previous layout/revision. Clicking the actual window-manager close button
[refuses closure](assets/workspace-tabs/close-refusal.png). Explicit
[Keep saved layout](assets/workspace-tabs/keep-saved.png) pauses layout writes;
a subsequent normal close succeeds with the old database record unchanged.
The MCP `workspace_close_window` action bypasses this application's close guard;
its earlier close is not normal-close acceptance. The window-manager check above
is the relevant evidence.

During verification, restoring history initially left an unqueried sidebar
showing no sessions. Workspace restoration now queries the scoped directory,
and an unobserved directory is labelled as unqueried rather than empty. A
workspace claim no longer implicitly resumes a saved unloaded provider context.

Six layout tests cover reopen/CAS, injected atomic write failure, corruption,
maximum tab count/selection, duplicate and cross-workspace session refusal,
escaping file paths, root/socket/UUID replacement, allowlisted query classification
and schema-13 upgrade preserving earlier drafts. Existing migration fixtures now
remove the schema-14 table when constructing historical databases.

Final validation: **319 workspace tests + 39 desktop tests passed**, including
the six layout regressions. Both strict Clippy runs, both formatting checks and
the desktop build passed. The [final-build reopening](assets/workspace-tabs/final-build.png)
was checked again after making workspace control acquisition keep resume explicit.

Validation commands (raw logs are stored losslessly as `.log.gz`, with
[byte counts and SHA-256](assets/workspace-tabs/raw-log-manifest.json)):

```sh
cargo fmt --all --check
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
cargo test --workspace --locked
cargo test -p fleetpane-session workspace_layout --locked
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
python3 scripts/workspaces-smoke.py
python3 scripts/last-session-smoke.py
python3 scripts/paseo-import-smoke.py
```

The process smoke checks retain catalog identity, Unicode paths, linked worktrees,
provider cwd isolation, last-session migration and offline Paseo history import.
Updated schema fixtures also pass appearance, Host-choice and recent-directory
process smoke checks.
They do not independently verify native tab interactions.

## Measurement and limits

An [XTest/XGetImage probe](assets/workspace-tabs/latency-probe.py) measures tab
selection frame changes in this hidden display. [Raw measurements](assets/workspace-tabs/latency.json)
retain all 30 samples, including first beta activation at **448.6 ms**. The next
29 cached switches have median **8.2 ms**, p95 **37.7 ms**, maximum **38.7 ms**.
The probe includes 2 ms pointer settling and 2 ms sampling; it measures the tab
background change, not complete history readiness. Earlier probe attempts failed
to deliver/correctly observe input; they were fixed without changing the app.

Observed RSS is about 262 MiB for the debug desktop, 7/8 MiB for the two Hosts,
and 35/43 MiB for their providers. These are three small sessions and one shell,
not a high-concurrency release benchmark. There is no same-workload Paseo baseline
and no performance target claim.

Conversation scroll, split panes, drag ordering/shortcuts, cross-window live
layout synchronization, actual IME, remote sources and other platforms remain
open. Terminal text drafts survive switching while their view is retained but
are not persisted when the tab/window closes. Large catalogs retain their
existing explicit page/limit behavior; 32 fully rendered workspaces have not
been stress-tested. Cleanup stopped all four owned Host processes and their provider/shell children,
removed the private DSH credential copy and stopped the isolated desktop. Shared
OpenCodex/Paseo and user sessions were untouched. [Readback](assets/workspace-tabs/cleanup.json).
