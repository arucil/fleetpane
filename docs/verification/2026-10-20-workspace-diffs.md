# Local workspace Diff verification — 2026-10-20

Covered slice: local Unix per-file read-only staged/unstaged comparisons through
the shared session engine, CLI and GPUI Kit desktop. This is not a complete Git
review/PR workflow or replacement release.

## Identity and commands

- Fleetpane base: `973b0ed883c25356a9f0849681333919d4497387`, dirty foundation on
  `feat/session-host-foundation`; no commit or push performed.
- Reference: local Paseo `a486b0558168c08890890e787d55cfb88c0d7256`.
- Rust 1.98.1, GPUI Kit 0.7.3; native debug binary on isolated Linux X11 with
  software Vulkan. No host desktop/browser was used.
- Task receipt, source/binary hashes, logs, CLI results and screenshots:
  `/tmp/fleetpane-diffs-20261020/receipt.json` and its directory.

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build -p fleetpane-host --locked
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
uvx --offline ruff check scripts
uvx --offline ruff format --check scripts/diffs-smoke.py
python3 scripts/diffs-smoke.py
python3 scripts/workspaces-smoke.py
git diff --check
```

After native QA, the generic busy label was simplified to “Working…” so file
reads do not display a worktree-creation deadline. Desktop tests, strict Clippy,
fmt and build were repeated; the completed views in the screenshots are unchanged.

Local result: 81 workspace tests and 4 desktop tests passed, both strict Clippy
checks passed, builds/fmt/Ruff passed, and both process smokes passed. The new
Unix smoke is included in CI; remote CI, macOS and Windows rendering were not run.

## Real Git behavior

The standalone smoke uses actual temporary repositories and the shipped CLI with
an offline Host endpoint. It verifies:

- A file with different HEAD, index and worktree contents produces distinct
  staged and unstaged patches, including from a saved subdirectory.
- Renames with newline/Unicode paths, literal glob-like names, deletion, binary,
  mode changes, intent-to-add, unborn HEAD and a real merge conflict.
- Untracked text, empty files and symlinks; a symlink to an external file displays
  its target path without displaying the target's contents. Untracked directory
  expansion and oversized inputs report unavailable explicitly.
- External diff/textconv configured to leave a marker are never executed;
  inherited `GIT_DIR`, `GIT_INDEX_FILE` and `GIT_EXTERNAL_DIFF` cannot redirect
  the observation. Index SHA-256 is identical before and after the reads.
- Non-UTF-8 content, output/line limits, missing paths, traversal/absolute paths
  and replaced repository identity fail visibly. A small read works after an
  oversized result. A Git wrapper exercising the production 10-second diff
  deadline is killed/reaped, and a later normal read succeeds.

The existing workspace smoke also passes after the common Git observation/status
changes: linked-worktree identity, replacement refusal, paging, bounded reads,
and synthetic Codex/ACP session cwd.

## Native interaction

The agent-owned hidden workspace `fleetpane-diffs-qa` ran the actual desktop,
backed by a real Git fixture and three isolated synthetic provider Hosts.
Screenshots and CLI readback show:

1. Select a saved subdirectory and open `sub/both.rs` in Unstaged scope. Kit
   displays index→working-tree changes with line numbers, Chinese content and
   red/green lines. [Unstaged](assets/workspace-diff-unstaged.png).
2. Copy patch; clipboard content exactly equals the CLI result (213 UTF-8 bytes).
   Switch to Staged; the same file displays original→index, excluding worktree
   contents. [Staged](assets/workspace-diff-staged.png).
3. Open a Chinese rename with metadata and no text changes; binary changes show
   an explicit message. Untracked Markdown displays added lines, and a new empty
   file displays its added-file mode/metadata.
   [Rename](assets/workspace-diff-rename.png),
   [untracked](assets/workspace-diff-untracked.png),
   [binary](assets/workspace-diff-binary.png).
4. Open a 17,000-byte line: visible size error, old Diff cleared. Open an empty
   file afterward: normal rendering works. Delete that fixture file externally
   and Refresh diff: visible stale-path error, prior content gone.
   [Limit](assets/workspace-diff-limit.png),
   [stale path](assets/workspace-diff-stale.png).
5. Before/after complete snapshots of all three Hosts are identical. The old
   drained ACP Host retains the exact active thread/turn and pending permission;
   all Host/provider PIDs and identities remain unchanged, new-Host journals
   remain empty, and Git index hash stays identical. Returning to the old Host
   displays the same pending badge and drain banner. These are synthetic
   isolation fixtures, not live OpenCodex/DeepSeek model calls.

The desktop, private Hosts/provider processes and hidden workspace were stopped
at completion; receipts and fixture files remain for review. No user's running
Paseo, Codex app server or DSH process was restarted or adopted.

## Remaining limits

Commit/base-ref review, stage/commit/push, conflict resolution, untracked-directory
expansion and file editing remain open. Observations are live and not atomic;
identity checks are not a filesystem security sandbox. Large input/process RSS,
large-repository latency, IME, cross-platform GUI behavior and production-provider
concurrency were not measured by this slice. See [ADR 0021](../adr/0021-read-only-workspace-diffs.md).
