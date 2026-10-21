# Provider archive restoration — 2026-10-21

Archived Codex sessions can now be listed, read and restored through shared
commands, CLI and the native desktop. Active archive mutation still requires the
tree/explicit-stop work described in [ADR 0055](../adr/0055-provider-archive-restoration.md).
The [receipt](assets/archive-restoration/receipt.json) binds source/artifact hashes,
logs, provider readbacks and native screenshots.

## Checks

- `cargo test --workspace --locked`: 169 passed. New cases reject crossing active/
  archive paging scopes, check exact restore-response identity and reject restore
  during drain. Desktop tests: 19 passed. Strict Clippy, format checks and builds
  for both Rust workspaces passed.
- `python3 scripts/archive-restore-smoke.py`: 107 archived synthetic sessions over
  two pages, combined title/cwd/archive filters, failure retention, drain refusal,
  exact durable receipt deduplication and explicit resume passed. An unrelated
  executing session and its pending permission remain unchanged. ACP refuses
  unsupported provider archive listing without creating a journal mutation.
- Existing directory and session process smoke passed after the fixture changes.
- `python3 scripts/codex-session-smoke.py`: installed Codex 0.163.0 with a new empty
  `CODEX_HOME` and a loopback rejecting HTTP service passed. An owned completed
  failed turn was archived for test setup. Its original ID, exact history items,
  name and cwd filter were preserved; shared archive listing/restoration worked.
  Restore caused no model request and returned `notLoaded`. Explicit resume then
  allowed the existing next-turn/model-option check. Only two local HTTP requests
  occurred; no account or external model service was used.

The Codex test's raw archive call is fixture preparation against one owned idle
session. It is not product acceptance of safe archive mutation or session trees.
An initial test-harness error used the wrong journal response field; it was
corrected to the actual `outcome.response` schema before the passing run.

## Native behavior

An owned hidden Linux X11 workspace, `fleetpane-restore-20261021`, ran the native
desktop with synthetic provider history. Host PID 3164084 and provider PID 3164086
survived two normal desktop instances.

1. Archived lists two records separately from the ordinary list. Opening one
   reads Markdown/code/user history and copies exact 81-byte assistant Markdown.
   The Host journal still contains zero mutations, confirming no automatic resume.
   [Archived history](assets/archive-restoration/archived-open.png).
2. Restore refusal shows the provider error, keeps the row and historical content,
   and preserves the exact Chinese multiline draft at revision 1. Send stays
   disabled. [Refusal](assets/archive-restoration/restore-refused.png).
3. Another archive gets its own exact saved draft. Restore removes only that row
   from Archived, preserves both drafts byte-for-byte, and shows Resume context.
   No turn starts. [Restored](assets/archive-restoration/restored-not-resumed.png).
4. Explicit Resume retains both draft revisions and history. Native Send then
   sends the original Chinese/newline/two-space text exactly once, appends one
   completed turn and clears only the confirmed draft. The failed archive draft
   remains unchanged. [Sent](assets/archive-restoration/sent.png).
5. Normal desktop close/reopen lists the restored session under Sessions; the
   failed record remains under Archived with its exact saved draft. Native
   Ctrl-A/C copies that draft as the original 36 bytes.
   [Reopened Sessions](assets/archive-restoration/reopened-sessions.png),
   [retained draft](assets/archive-restoration/reopened-refusal-draft.png).

Final journal methods are two `thread/unarchive` calls (one rejected), one
explicit `thread/resume`, and one `turn/start`. Both desktop apps exited with
status 0; owned Host/provider/socket and hidden workspace were cleaned up.

## History and limits

Paseo root review advances from 168 to 174: registry deletion intent; fallback
tool ID uniqueness; forked test workers; image-picker media type change; actual
Cancel/Delete plumbing; and JSON response validation. All 13 selected paths'
changed text was read, with excluded plan paths listed in the
[review receipt](assets/paseo-replay/archive-restoration/receipt.json). Historical
tests were not run. The JSON helper still accepts non-finite numbers and does not
reliably reject cycles; Rust continues to use typed `serde_json::Value` boundaries.

Archive mutation, worktree deletion, tree impact, descendant restoration, provider
crash recovery and resource-complete Host update remain open. Native acceptance
uses a synthetic Codex-shaped provider; actual Codex acceptance is process/CLI,
not an external-model desktop test. ACP provider archive restoration, forced lost
restore replies in native UI, macOS/Windows/Wayland, remote CI, notifications and
performance remain unaccepted. The active goal remains incomplete.
