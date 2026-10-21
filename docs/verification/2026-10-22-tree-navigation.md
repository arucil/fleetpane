# Tree history navigation — 2026-10-22

Implemented explicit **Open session** from a known tree node and **Back to source
session** in the destination. [ADR 0082](../adr/0082-explicit-session-tree-navigation.md)
records the contract; [receipt](assets/tree-navigation/receipt.json) binds the
source/binaries, selected historical diffs and native observations.

Shared engine tests check Host, tree, parent, session, cwd, archive membership,
unknown membership and removed identities; display names and activity can change.
`cargo test --workspace --locked` passed 259 tests; desktop tests passed 33.
Strict Clippy and formatting passed both workspaces, as did Windows GNU engine
Clippy and native builds. `python3 scripts/session-tree-smoke.py` passed its 113
catalog/110 loaded record metadata, paging, lease and fault checks.

In the owned hidden Linux workspace `fleetpane-tree-nav-20261022`:

- The tree displays root, child, archived depth-two descendant and an ephemeral
  node. Independent fork/orphan remain outside the displayed subtree. Unknown
  membership disables opening. [Tree](assets/tree-navigation/final-tree.png).
- Opening the child exposes its original approval and **Back to source session**.
  The final binary restores its exact 32-byte unsent Chinese draft after window
  replacement. Returning to the root restores and copies its exact 38-byte draft.
  [Child](assets/tree-navigation/final-child.png).
- Opening the archived descendant preserves read-only membership, shows the
  restoration notice at the top of details and copies the original 47-byte
  Markdown source. No restore or resume was sent.
  [Archived history](assets/tree-navigation/final-archive-ready.png).
- Injecting an inventory rejection on the final binary leaves the root and draft
  selected, with a visible provider error. [Refusal](assets/tree-navigation/final-refused.png).
- At 900×560 the return action, selected draft and Stop remain visible. Existing
  large headers leave little history/detail space; approval controls at that size
  were not accepted. [Small window](assets/tree-navigation/final-small-real.png).

The final baseline/readback preserve Host and provider IDs/PIDs, owner, admission,
pending request, activity, result attention, operation IDs, mutation count and
fault state. Read query sequences advance. Fixture setup produced two sends
before that baseline; tree navigation adds no mutations. Both desktop instances
closed normally and the task-owned workspace/Host/provider were stopped.

The initial fixture launch lacked its state directory and exited before opening
a socket. Creating that owned directory fixed the setup. Native trial also
reproduced inherited interaction scroll hiding a destination archive notice;
the final binary resets detail scroll on session changes and the archived image
above verifies the repair. No live user session was used.

History review advances continuously from 334 to 376. Selected paths/hunks are
recorded individually; source checks are separate from plan-only pass claims,
including the empty orchestrator test and later contradicted dark-theme pass.
Fleetpane retains its native cards. Root-only filtering, MCP creation, provider
orchestration, ACP ancestry, atomic snapshots, macOS/Windows native acceptance and
overall Paseo parity remain open. Git Diff remains deferred.
