# Native workspace tab drag acceptance

2026-10-22, Linux x86_64, isolated X11 `:92`, GPUI Kit 0.7.3, debug build and
Mesa software rendering. Source base `e4e39764790aebc04c6f2b84493d862f97b2e2f2`
plus this change. [Decision](../adr/0112-workspace-tab-drag-order.md),
[final readback](assets/workspace-tab-drag/final-readback.json),
[artifact hashes](assets/workspace-tab-drag/manifest.json).

Three task-owned synthetic Hosts (one Codex, two ACP) expose four workspaces,
including the same Chinese path on different Hosts. Original pending requests
are a two-page Codex question, a Codex approval and two ACP permissions. These
are deterministic fixtures, not installed OpenCodex/DeepSeek model acceptance.

## Native results

- Dragging the active first tab to the last and dragging an inactive tab across
  Hosts preserve the selected workspace UUID, exact session identity and loaded
  `notes.md` source. Dropping outside the strip leaves the saved revision/order
  unchanged. See [active move](assets/workspace-tab-drag/final-first-to-last.png)
  and [inactive move](assets/workspace-tab-drag/final-inactive-drag.png).
- An injected SQLite update trigger refuses a drag save. The live strip changes,
  while revision 10 and its exact previous layout remain stored. Alt+F4 leaves
  the window open with [the actual save error](assets/workspace-tab-drag/final-close-refused.png).
  Removing the trigger and clicking Reload saved layout restores the stored
  order and file location. QA found a stale close-refusal message after reload;
  the matching layout-error cleanup fixes it. The final build's
  [recovery](assets/workspace-tab-drag/final-reloaded.png) clears both messages.
- With real PTY keyboard input enabled and its screen focused, dragging the
  selected tab retains the Terminal panel, terminal UUID, shell PID **198160**,
  full terminal Info and input sequence **0**. No pointer gesture enters the
  shell. [Final terminal drag](assets/workspace-tab-drag/final-terminal-after-drag.png).
- Normal window-manager closure and replacement retain the exact ordered layout,
  selected Terminal panel and terminal identity without a claim. Its initial
  asynchronous read restores output; keyboard input remains off until explicit
  control. At [900×560](assets/workspace-tab-drag/final-small.png), a right-to-left
  drag preserves the active terminal and [order](assets/workspace-tab-drag/final-small-drag.png).
  Dragging the active conversation while a question is pending keeps that same
  [question](assets/workspace-tab-drag/final-small-question-after-drag.png).
- All four draft records retain their exact text, revision and absent send
  attempt: each is **36 UTF-8 bytes**, with Chinese, newline and two trailing
  spaces. After replacement and another drag, beta's native clipboard is exactly
  `drag 1 中文\nkeep trailing spaces  ` (the `\n` denotes a real newline).
  [Rendered draft](assets/workspace-tab-drag/final-beta-draft.png).
- Readback retains all original Host/provider identities, active turns, four
  pending request payloads, admission/fault states, mutation IDs/journal totals
  and empty results. Codex ownership changes only for explicit QA Take control
  actions, never a drag or restoration. No answer, stop, send or shell command is
  issued. Final normal close retains exact revision **18**. All owned desktops,
  Hosts, providers and PTY exit during [cleanup](assets/workspace-tab-drag/cleanup.json);
  their sockets are removed. Other sessions/workspaces are untouched.

## Checks and limits

Final desktop binary SHA-256 is
`51b55486bce284110612582cba44aff5b10d8f0766936f8623fce36368829b79`;
the final running executable matches it. The earlier exploratory screenshots used
a different build and are excluded from the final-image acceptance above.
**44 desktop tests** and **6 shared layout tests** pass; strict desktop Clippy,
both format checks and desktop build pass. Compressed raw logs accompany the
receipt. The new desktop regression checks both directions, intervening order,
same identity and stale/missing source/target boundaries.

```sh
cargo fmt --all --check
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo test -p fleetpane-session workspace_layout --locked
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
```

The previous exact-head [CI run](https://github.com/arucil/fleetpane/actions/runs/39758600958)
passes all four jobs; it covers the CI repairs, not this subsequent desktop change.
The new head needs its own CI result. This is not other-platform GUI, real model,
IME, scale or release-performance acceptance. Foreign Workbench rejection and
stale payload handling are implemented; only identity boundaries have unit proof.
Drag-edge autoscroll, cross-window moves/detach, splits and conversation scroll
restoration remain open. Existing Tabs menu/keyboard navigation handles overflow.
