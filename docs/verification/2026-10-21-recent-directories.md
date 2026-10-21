# Recent session directories — 2026-10-21

Implemented in the uncommitted working tree on base `c91d269`. The private
[receipt](assets/recent-directories/receipt.json) binds source and build hashes,
automated logs, native screenshots, readbacks, normal exits and process cleanup.
Paseo scoped-path review reaches ordinal 80; ordinal 81 is next. Earlier receipts
retain their historical cursors. [ADR0038](../adr/0038-recent-workspace-directories.md)
records the directory suggestion contract and limits.

## Implementation

The shared device Store keeps three exact paths per Host socket, ordered by most
recent use and deduplicated by endpoint/path. Transactions serialize concurrent
writers and retain at most 96 suggestions overall. Schema 7 adds a separate table;
existing migrations, drafts, pins, workspace identities and appearance remain.
Invalid or corrupt rows produce visible errors instead of empty-list recovery.

The native New session reply path records the provider's validated directory after
successful creation. Permission to create and the creation receipt remain in the
existing Host engine. Preference failure has its own error and does not change a
successful session into a failed/uncertain creation or cause another dispatch.
Rejects do not create suggestions. Cached native menus support Reload and Forget,
and selecting a path only prefills input. Prepared creation/workspace bindings keep
their directory. Async local work runs outside rendering; a creation save arriving
during a local job is queued, and normal window close waits for local work to settle.

The `recent STORE list SOCKET`, `remember SOCKET CWD` and `clear SOCKET` CLI use
the same Store methods and require no Host. Remember is an explicit preference
action; it is not evidence of session creation. Suggestions do not auto-register
saved workspaces or expand home shorthand. Existing absolute-path requirements
remain in effect.

## Automated verification

135 root tests (4 + 4 + 48 + 79), 9 desktop tests and 13 history-tool tests pass.
The three new Store tests cover exact Unicode/newline/space round trips, recency
replacement, retention, endpoint isolation, concurrent writers, schema-6 upgrade
and corruption/input refusal. Earlier downgrade fixtures now remove the new table
when constructing genuine old schemas; migration checks still verify preservation
and rejection of damaged current tables.

Root/desktop Clippy with warnings denied, Windows GNU cross-target Clippy, both
builds, formatting and whitespace checks pass. Cross-compilation is not Windows
runtime acceptance; these are local results, with no remote CI run.

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --locked --target x86_64-pc-windows-gnu -- -D warnings
cargo build --workspace --locked
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
python3 -m unittest discover -s scripts/tests -p '*_test.py'
python3 scripts/recent-directories-smoke.py
python3 scripts/appearance-smoke.py
python3 scripts/creations-smoke.py
python3 scripts/drafts-smoke.py
python3 scripts/workspace-pins-smoke.py
```

The new CLI process smoke also covers absent Host sockets, endpoint-specific
forget, malformed data refusal without overwrite and unchanged empty creation/
workspace tables. It is wired into CI. Existing creation, draft, appearance and
workspace pin smokes pass with the new controller schema.

## Native verification

Used hidden Linux workspaces `fleetpane-recent-20261021` and
`fleetpane-recent-offline-20261021`, task-owned Codex/ACP fixtures and private
controller state under `/tmp/fleetpane-recent-qa`. No real model request or shared
user process was used. The main native run used the product implementation before
later test-only migration fixture corrections. A rebuilt final binary was hashed
before the separate offline run. The receipt distinguishes these phases.

- Created five sessions using the native input/button: the Chinese alpha directory
  twice, then beta, gamma and delta. The dropdown retained delta/gamma/beta in that
  order. Reusing alpha deduplicated its suggestion while creating the explicitly
  requested second session. The default small window kept its controls accessible.
- Selecting alpha copied the exact Chinese path into input. Before/after readbacks
  retained session metadata, Host/provider/owner identities, operation IDs and
  preferences. Query IDs/sequences differ because the inspection itself uses query
  requests; those are not mutation evidence.
- Switched to a second Host with only its independently saved suggestion. Selected
  it and created one ACP session. Main Host suggestions and sessions stayed separate.
- A relative-path creation attempt displayed the existing absolute-directory error,
  added no Host mutation and left the three saved suggestions unchanged.
- Held an exclusive lock on the private device database, then created another
  native Codex session. Host readback confirmed the sixth session. The UI displayed
  Session created, but its recent directory could not be saved, alongside the
  existing independent draft-store error. After unlocking, explicit suggestion and
  empty-draft reloads cleared the errors with six sessions and no duplicate dispatch.
- Normal close and replacement retained exact suggestions and all seven sessions
  across both providers. Native Forget removed only the main Host's suggestions;
  both Host operation lists and all sessions were preserved.
- A third window started while the controller database was locked. Closing it
  showed Saving local state before closing while the recent read was outstanding.
  Releasing the test lock allowed normal exit 0.
- With both private Hosts stopped, the final rebuilt binary read the remaining
  second-Host suggestion and copied its exact path into input. New session stayed
  disabled; no Host was started. This interaction used a maximized window; the
  later recheck below also covers a fresh unresized window.

All four native windows exited normally with status 0. Both private Hosts and
providers, all windows and both hidden desktops are gone. The receipt checks their
14 known process IDs; both database lock helpers also exited. Stderr contains only
the hidden X server's Mesa DRI3 acceleration warnings. This is software-rendered
interaction evidence, not a latency or memory benchmark.

## Rendering observation correction and acceptance limits

The initial interpretation of two offline image previews as incomplete first
paint was incorrect. A [controlled recheck](2026-10-21-render-inspection.md)
confirmed complete pixels in those original PNGs, direct X11 capture and a fresh
unresized window using the identical binary. Consecutive previews omitted static
regions; there was no demonstrated application paint failure. Original evidence
and its receipt remain unchanged. No repaint workaround or renderer fix was added.

This slice does not establish new real OpenCodex/DeepSeek model behavior, remote
directory discovery, home expansion, automatic cross-window preference refresh,
Host process takeover, native Windows/macOS acceptance or full Paseo parity.
