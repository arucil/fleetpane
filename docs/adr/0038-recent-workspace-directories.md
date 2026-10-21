# ADR 0038: Remember recent session directories per Host

Status: implemented; local acceptance recorded separately.

Paseo replay ordinal 80 adds three recent working-directory suggestions. Its hook
sets local state before persistence and stores an attempted path before the
creation result. Fleetpane has saved workspaces and explicit pins, but direct
session creation did not remember the last directory after replacing a window.

Keep three suggestions per exact configured Host socket in the existing private
controller database, schema 7. Store exact absolute UTF-8 paths without trimming
or filesystem canonicalization. These are suggestions for a new session, not
saved workspace identities or evidence that a directory still exists. The existing
creation path performs its normal validation when the user chooses New session.
Do not expand a remote path using the controller's home directory. Current input
continues to require an absolute path; home shorthand remains separate work.

Record the returned directory only after a successful, validated native creation
reply. This common reply path covers direct, routed and saved-workspace creation.
Rejects and uncertain outcomes do not optimistically change suggestions. A failed
preference save is displayed independently of the successful creation and never
authorizes a second creation. CLI remember is an explicit preference action and
does not claim that a session was created.

Serialize recency updates with an immediate SQLite transaction. Exact endpoint/path
pairs deduplicate and move to the front; retain at most three per endpoint and
96 overall. Global eviction discards the oldest suggestions only. Reopening the
controller preserves recency across Host incarnations at the same endpoint. A
different socket never inherits another Host's paths. Corrupt rows and database
failures are explicit; no empty-list fallback or automatic operation retry.

The native menu loads outside rendering, retains its cached list, and has explicit
Reload and Forget actions. Selection only prefills the input; it does not claim a
Host, create/resume a session or mark a path recently used. Prepared workspace or
creation bindings disable replacement suggestions. An in-flight local read/clear
queues a successful creation's remember operation, and normal close waits for the
local job. Remember errors do not prevent closing after the job settles.

The shared Store methods power the GUI and `recent STORE list|remember|clear`
CLI. Those commands require no Host RPC and work while a Host is absent. Forget
removes only the selected endpoint's suggestions, preserving sessions, workspace
registrations, pins, drafts, operation receipts and appearance preferences.

This does not auto-register workspaces, import provider history into device
preferences, add remote path discovery, implement home expansion, synchronize
menus between windows automatically or provide a session-launch approval policy.

[Local verification](../verification/2026-10-21-recent-directories.md).
