# Read-only session ancestry — 2026-10-21

Shared engine, CLI and Linux native inspection now expose parent-linked Codex
sessions, provider loaded contexts, archive membership and observed activity.
This is a prerequisite for safe archive work, not acceptance of archive mutation
or resource-complete shutdown. See [ADR 0056](../adr/0056-observe-session-ancestry.md)
and the source/artifact/log [receipt](assets/session-tree/receipt.json).

## Engine and process checks

- `cargo test --workspace --locked`: **176 passed**, including seven ancestry
  cases. Missing parent fields and hydrated history are rejected; forks stay
  independent; archived/ephemeral descendants and pending requests retain exact
  identities. Cycles, depth/text/byte limits, repeated cursors, overlapping
  catalogs and Host/owner/loaded-membership changes refuse partial adoption.
  Scripted transports assert only snapshots and bounded query requests occur.
- `cargo test --manifest-path apps/desktop/Cargo.toml --locked`: **19 passed**.
  Strict Clippy, format checks and builds passed for both Rust workspaces.
  Ruff check/format passed for the three changed Python fixtures/checks.
- `python3 scripts/session-tree-smoke.py`: **113** metadata records and **110**
  loaded IDs cover both pagination paths, a transitive archived grandchild,
  a loaded ephemeral child absent from history, independent fork and explicit
  orphan warning. In-tree and unrelated active approvals survive collection.
  The same observation works during drain. Missing lease, unsupported ACP,
  provider refusal, duplicate loaded IDs, wrong metadata identity and
  `--take-control` are refused without acquiring control or journaling a mutation.
- `python3 scripts/archive-restore-smoke.py`: existing archive/restore checks pass
  after the fixture extension.
- `python3 scripts/codex-session-smoke.py`: installed **Codex 0.163.0**, a new empty
  CODEX_HOME and a rejecting loopback HTTP service verify one real archived root's
  metadata and loaded inventory through the shared CLI. Host/provider identity,
  lease, journal, activity and pending requests remain unchanged. No model request
  occurs during inspection; the complete existing test makes two loopback
  requests. No external LLM/account, real spawned child or real resource teardown
  is claimed.

During test authoring, a mock snapshot incorrectly used protocol 1; it was fixed
to the runtime protocol constant before the passing run. No protocol check was
weakened. A deserialization token validates empty history without suppressing
compiler or Clippy warnings.

## Native behavior

Owned hidden Linux workspaces used synthetic provider history. The initial
`fleetpane-tree-20261021` run exercised:

1. Root inspection shows four nodes, seven scanned records, one other loaded
   context, the child’s observed active turn/pending approval and an incomplete
   parent-chain warning. JSON copy contains exact root, child, archived grandchild
   and ephemeral IDs; the fork is excluded.
   [Root view](assets/session-tree/final-tree.png).
2. Native scrolling reaches the archived depth-two child and ephemeral helper
   outside history; each Copy ID returns the exact original identity.
   [Archived child](assets/session-tree/descendants.png),
   [ephemeral helper](assets/session-tree/ephemeral.png).
3. Injected inventory failure retains the previous copied JSON byte-for-byte and
   visibly labels results stale. Explicit successful refresh removes the error.
   [Retained refusal](assets/session-tree/refused-retained.png).
4. Selecting the independent fork closes the inspector. Returning to the root
   preserves and copies the exact **49-byte** Chinese multiline draft, including
   trailing spaces, at revision 1. Reopening after the first desktop instance also
   retained this draft. [Fork selection](assets/session-tree/fork-selection.png).
5. At **1100 × 650**, inspecting the active child leaves Stop visible. Scrolling
   the interaction area reaches the existing Allow once/Decline controls while
   Stop and the composer remain visible. No approval or Stop was dispatched.
   [Small active view](assets/session-tree/small-active-child.png),
   [approval controls](assets/session-tree/small-active-approval.png).

Both desktop instances closed normally with exit code 0. Host **3288497** and
provider **3288553** survived desktop close/reopen. Final readback after closing
the active child's window still has both original pending requests, activity,
the final controller lease, the exact draft and only the two fixture-setup
`turn/start` mutations. The explicit takeover on window reopen is outside the
read-only inspection comparison; every inspection preserves its current owner.

The native Host was started before a validation-token refactor in session metadata
deserialization; its runtime query-channel source did not change. Both running-Host
and final CLI binary hashes are recorded separately. The first desktop was used to
refine row width and viewport height; subsequent screenshots cover those changes.

Final review found that read-only snapshot adoption also needed to detach a tree
when the Host incarnation changes. All adoption paths now share that invalidation.
An additional workspace, `fleetpane-tree-replace-20261021`, exercised the final
binary: a separate owned controller fences the desktop to read-only; refreshing
the tree borrows that controller's lease without changing it or journaling work.
Replacing the owned Host at the same socket changes Host identity and closes the
old inspector without taking control. [Before](assets/session-tree/read-only-before.png),
[detached](assets/session-tree/detached-after-replacement.png).

After explicit takeover of the replacement, final-binary checks again copy the
same four identities and preserve a 49-byte revision-1 draft, two pending requests
and exactly two fixture `turn/start` receipts through refresh failure and workspace
teardown. The linked root and refusal screenshots use this final binary. The
descendant/selection/small-window screenshots use the preceding binary; their
tree-renderer and collector sources are unchanged, and both artifact/source
bindings are recorded. The third desktop's saved manifest records SIGTERM from
workspace stop before graceful exit was confirmed; its normal close is not
claimed. Host/provider activity and drafts were read back unchanged afterward.

The owned Host/provider were then stopped and their socket removed; the hidden
workspace was stopped. No shared or user provider/session was touched.

## History and remaining scope

Replay **175–179** reads all changed text in **eight selected paths**, 13,217 diff
bytes, preserving the prior 174 ledger entries. It concerns voice interruption
and a plan-only checklist; voice remains missing. Exact exclusions/hashes are in
the [history receipt](assets/paseo-replay/session-tree/receipt.json).
Next is **180**, `7284efd0fe7401c1ccf8b06dddf8f28861d37386`.

These observations are non-atomic and limited to one local provider Host. No
background-resource census, active archive/stop, ACP ancestry, native voice,
Session Host update handoff, CI, macOS/Windows QA, full parity or performance gain
has been established by this slice.
