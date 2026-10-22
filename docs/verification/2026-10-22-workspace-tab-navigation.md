# Workspace tab keyboard and menu acceptance

2026-10-22, Linux x86_64 isolated X11 :90, GPUI Kit 0.7.3, debug unoptimized
Mesa software renderer. Source base `fd7c6c14789c1185c68a4763c902da3bf5e14967`
plus the tab-navigation working tree. This verifies the bounded workflows below.
[Identity/state receipt](assets/workspace-tab-navigation/receipt.json),
[artifact hashes](assets/workspace-tab-navigation/manifest.json),
[decision](../adr/0108-workspace-tab-navigation.md).

## Native behavior

- Ctrl+PageUp/PageDown selects and wraps; Ctrl+1–8 selects an existing position
  and Ctrl+9 selects the last tab. Missing position 8 with three tabs leaves the
  current tab unchanged. Ctrl+Shift+PageUp/PageDown moves the selected UUID one
  position without wrapping or replacing its view. The
  [Tabs menu](assets/workspace-tab-navigation/fixture-tab-menu.png) exposes the
  same actions; clicking Next opens the original
  [DeepSeek source](assets/workspace-tab-navigation/fixture-menu-next-selected.png).
- Two separately keyed Chinese multiline drafts retain exact clipboard bytes
  after native selection/reordering. [900×560 beta](assets/workspace-tab-navigation/fixture-small-beta.png)
  keeps the tab/menu, composer and Send visible. Twelve registered fixture tabs
  demonstrate automatic strip scrolling to the [last](assets/workspace-tab-navigation/fixture-overflow-last.png)
  and [first](assets/workspace-tab-navigation/fixture-overflow-first.png) selections.
- With terminal keyboard input enabled and the screen focused, selection and
  reordering preserve shell PID 3168929, terminal UUID, input sequence **0** and
  exact output bytes. All retained Host snapshot fields, including mutation
  journal counts, match before/after. These chords never enter the PTY.
  [Native terminal](assets/workspace-tab-navigation/fixture-terminal-keyboard-reorder.png).
- An independent ACP turn is held on one original permission. Closing all three
  tabs with Ctrl+Shift+W preserves that request, active turn, Host/provider IDs,
  lease and mutation counts. The real shell stays open at input sequence 0.
  The [last view returns to the generic Host](assets/workspace-tab-navigation/fixture-last-tab-closed.png).
  A close during initial history reads was correctly refused; explicit retry
  after that read finished closed the view.
- An injected draft-update failure leaves all three tabs open after the close
  chord and [shows the reason](assets/workspace-tab-navigation/fixture-draft-close-refused.png).
  Removing the trigger and choosing Retry draft save writes the exact retained
  text. An independent layout-update failure removes only the live view, keeps
  the previous persisted record and [exposes the save error](assets/workspace-tab-navigation/fixture-close-save-failure.png).
  Reload after removing that trigger restores the saved layout.
- Ctrl+PageDown while the session switcher is focused leaves the underlying tab
  unchanged. QA also reproduced popup-menu key leakage during implementation;
  the final context guard keeps the tab unchanged with the
  [Kit menu focused](assets/workspace-tab-navigation/real-popup-focus-isolation.png).

## Installed OpenCodex and DeepSeek

Private homes used installed Codex 0.163.0 via the existing OpenCodex loopback
proxy (`gpt-6.1-sol`) and DSH 0.2.0-rc.2 (`deepseek-v4-flash`). Two Codex and one
DeepSeek sessions each completed a request to return only a synthetic token.
The same alpha path on both Hosts retains independent workspace/session IDs.
Existing user sessions and the shared proxy were not changed.

The three native clipboard copies are exactly `真实 Alpha 草稿\n保留` (26 UTF-8
bytes), `真实 Beta 草稿\n保留` (25), and `真实 DeepSeek 草稿\n保留` (29); `\n`
denotes a real newline. Cross-Host keyboard navigation and reordering preserve
these texts, Host/provider PIDs, owner leases, activity and mutation journal
counts. [Small DeepSeek](assets/workspace-tab-navigation/real-small-real-deepseek.png)
and [small beta](assets/workspace-tab-navigation/real-small-real-beta.png) show
the correct distinct history and draft at 900×560.

After an actual window-manager close, the final compiled binary restores ordered
tabs **beta, alpha, DeepSeek**, selected beta, and all three exact drafts. Draft
copying after replacement uses one explicit control claim per Host; restoration
itself does not claim or resume. [Reopened beta](assets/workspace-tab-navigation/real-reopened-beta.png),
[reopened DeepSeek](assets/workspace-tab-navigation/real-reopened-deepseek.png).
The original Hosts/provider processes remain alive until explicit QA cleanup.

The installed Codex CLI `session ... wait` check returned JSON-RPC `-32601`,
`list_turns is not supported yet`. This is an observed compatibility gap in
the existing turn-list-based wait path, not a passing check. Session `open`
readback returned the completed turn and expected token; bounded Host activity
observation plus that readback supplied completion evidence for this tab QA.
No wait implementation was changed or accepted by this work.

## Checks and limits

Two desktop tests cover exact modifiers and selection/reorder boundaries. Final
results: **41 desktop tests and 319 workspace tests passed**, with both strict
Clippy runs, formatting and desktop build passing. Raw final logs are stored
losslessly in this directory as `.log.gz`.

```sh
cargo fmt --all --check
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
cargo test --workspace --locked
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
```

All owned desktops, Hosts, providers and shells were stopped, and the private
DeepSeek credential copy was removed. Other workspaces/runtimes were untouched.
This is not IME, macOS/Windows, release-performance or scale acceptance. Held-key,
source Find and native dialog guards are implemented but were not separately
exercised here. Drag ordering, split panes, scroll restoration, configuration/media/
question drafts and terminal text persistence after closing a tab remain open.
