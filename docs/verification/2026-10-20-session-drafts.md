# Local session draft verification — 2026-10-20

Covered slice: saved text for existing Host-backed sessions, shared CLI/native
revision checks, durable send identity and conservative receipt recovery. Further
Git Diff work is deferred at the user's request.

## Identity and checks

- Fleetpane base `973b0ed883c25356a9f0849681333919d4497387`, dirty foundation on
  `feat/session-host-foundation`; no commit or push performed.
- Inspected Paseo reference `a486b0558168c08890890e787d55cfb88c0d7256`, scoped
  draft identity, persistence coalescing and lifecycle state; no code/assets copied.
- Rust 1.98.1; GPUI Kit 0.7.3. Native debug binary rendered in isolated Linux X11
  with software Vulkan, without using the host desktop or browser.
- Task-owned receipt, source/binary hashes, CLI results, snapshots and logs:
  `/tmp/fleetpane-drafts-20261020/receipt.json` and its directory.

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
uvx --offline ruff format --check scripts/drafts-smoke.py scripts/provider-fixture.py
python3 scripts/drafts-smoke.py
python3 scripts/creations-smoke.py
python3 scripts/workspaces-smoke.py
python3 scripts/session-smoke.py
git diff --check
```

Local result: 84 workspace tests and 4 desktop tests passed; both strict Clippy
checks, builds, formatting and Ruff passed. Draft, creation, workspace and session
process smokes passed. The new draft smoke is included in Unix CI; remote CI
was not run.

## Shared engine and process evidence

- Exact multiline/Unicode/whitespace text survives store reopen. Socket, Host
  incarnation, session and cwd separate records. Two concurrent writers have one
  winner; an empty tombstone retains its revision and refuses stale writes.
- Schema 3 migrates to 4 without losing a worktree job; the existing schema-1/2
  migration tests also pass. A schema-4 database missing its drafts table is
  refused. Permissions remain 0600 and text over 64 KiB is rejected.
- Synthetic Codex and ACP sends clear saved text only after a matching success
  receipt. Provider rejection restores editable text. Pending/unknown and
  mismatched receipts retain the attempt; receipt checking does not claim,
  dispatch or change the Host snapshot. Historical matching success can settle
  the draft without replaying timeline events.
- Kill the controller after a fsynced synthetic send effect: the attempt survives,
  editing/repeated send are refused, and checking returns pending with exactly
  one effect. Independent session reads remain responsive. Restart the Host
  using its original provider configuration: the old attempt is unknown and is
  never replayed. Recovery lists are bounded to 32 records with row cursors.

During verification, the held-send fixture initially blocked its stdin loop,
which also blocked session reads. It now withholds only that RPC response; the
process smoke includes an independent read while the send is pending. Provider
query responses advance the event sequence by design; the no-change assertion
specifically surrounds receipt-only draft checking.

## Native behavior

All inputs and providers below are synthetic. Screenshots are in the receipt
directory; selected captures are checked in under `assets/session-drafts/`.

1. Two DeepSeek ACP sessions in the same cwd save distinct Chinese multiline
   drafts. Switching back restores the original text, including indentation.
   A Codex session stores a third independent draft.
2. SIGKILL only the task-owned desktop after its saved indication; replace it
   and explicitly reopen the original sessions. Chinese text/line breaks/indent
   match clipboard readback exactly, including the independent Codex draft.
3. Save from another controller, then edit the stale native copy. The UI reports
   conflict, retains the local text for Copy, disables Send and blocks Alt+F4.
   Explicit retry does not overwrite the other revision. Explicitly discarding
   local edits and reloading restores the other version exactly; closing works.
4. Send that restored text through native ACP. The rendered assistant echoes it,
   and durable draft text/attempt are empty at revision 4.
5. Hold a SQLite writer lock, change Codex text A → B → A while a save is in
   flight, then close. The window displays “Saving drafts before closing…” and
   remains open until the writes complete. It then closes with original A saved
   at revision 3, proving the latest value was not skipped.
6. Kill a CLI controller after an actual synthetic send dispatch, then open that
   session in the desktop. It recovers the send attempt with its exact Chinese
   text. Check send status reports pending; Copy returns exact text, editing is
   read-only and Send is disabled. A paste cannot replace it. Host snapshot is
   identical across the check and there is exactly one fsynced send effect.
7. Original Old/New DeepSeek and Codex Host/provider PIDs and incarnations remain
   unchanged across desktop replacements. The Old Host's complete snapshot,
   including its active turn, pending permission and drain state, matches before
   and after. No shared/live provider process was stopped or adopted.

Selected native captures: [restored draft](assets/session-drafts/recovered-first.png),
[concurrent save conflict](assets/session-drafts/conflict.png),
[waiting for save before close](assets/session-drafts/closing-save.png),
and [pending send recovery](assets/session-drafts/pending-checked.png).

All task-owned native windows, the isolated `fleetpane-drafts-qa` desktop and
synthetic Host/provider processes were stopped after verification; their control
sockets are absent. Receipts and synthetic evidence files are retained.

## Scope and remaining acceptance

This establishes the covered local synthetic workflows. No new real OpenCodex/
DeepSeek model request, remote CI, macOS/Windows rendering, high-concurrency load
or power-loss durability test was run. Forced termination can recover only the
last confirmed save. Host incarnation changes require CLI listing/copy recovery;
unknown sends remain conservative and cannot be resent from this draft. Model
options, question answers, attachments, pre-creation text and selected layout are
not persisted by this feature. This is not a donut-paseo parity/release claim.
