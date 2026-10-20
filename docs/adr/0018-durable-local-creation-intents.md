# ADR 0018: Bind local creation intents before dispatch

Status: accepted for the local Unix prototype, 2026-10-20.

## Problem

ADR 0017 recommends a ready group member, but the pending UUID and preparation
guard live in the window. Closing the controller around `thread/start` loses
the connection between that request and its Host. Host UUID deduplication alone
cannot prevent a replacement request from executing on a second Host.

## Decision

The session engine stores immutable creation bindings in a private SQLite
database: UUID, group, workspace, exact endpoint, observed Host incarnation and
admission generation. Reusing a UUID with the same group/workspace returns the
original target even if priority changes; different group/workspace reuse fails.
SQLite transactions serialize concurrent binding and dispatch attempts.

Desktop **Prepare on NAME** commits the binding before opening the Host view.
**New session** uses its UUID through shared `Store::dispatch`. Editing the
workspace cannot change the binding. Taking control of an already owned Host
stays explicit. A fresh snapshot must match the saved incarnation/admission and
the controller lease. Host admission/lease checks remain the final mutation
authority; these snapshots do not reserve admission atomically.

Dispatch durably changes `prepared` to `attempted` with a conditional update
before provider submission. Exactly one caller may pass that transition. It is
never reversed, including on an error. Repeated sends are refused; the CLI checks
before claiming control. A crash between the commit and network dispatch can
leave an attempted intent without a Host receipt. This remains unresolved,
never permission to resend. This provides at most one submission per saved
intent, not exactly-once creation or automatic eventual completion.

Recovery reads only the bound endpoint's operation UUID and validates its method
and parameters. Pending, unknown and settled provider errors stay distinct.
A valid successful receipt recovers the session ID even if its event sequence
belongs to an older Host incarnation; old events are not replayed. Missing,
unavailable or malformed receipts remain visible errors. Checking never claims,
resends, resumes a session or searches another Host. Select the named Host to
open a recovered session from its existing session list.

**Saved group creations** pages 32 records with Older/Newest controls.
Unattempted records can be prepared again on their original configured endpoint;
attempted records offer **Check status**. Late preparation results cannot switch
away from an approval the user returned to during the task.

The desktop uses `CONFIG.creations/creations.sqlite3` beside `CONFIG.json`
(`Path::with_extension("creations")`); the CLI accepts that store directory
explicitly. Directory/file permissions are 0700/0600. SQLite symlink opening is
refused, rollback mode uses synchronous EXTRA, and schema/page size are checked.
The budget is 64 MiB (16,384 pages of 4 KiB), a 1 MiB cache, and no automatic
retention. I/O runs in awaited blocking tasks, outside rendering and polling.
Database errors stop creation rather than bypassing storage. No Host protocol
or journal schema change is needed (still v3/schema 2).

[ADR 0019](0019-existing-workspace-catalog.md) subsequently adds the workspace
catalog in local controller-store schema 2, preserving these creation records
and attempt flags during migration. This is independent of the Host journal
schema number.

## Limits

This persists the initial creation binding, not every session's ownership or all
desktop commands. Direct per-Host creation, messages, drafts and provider private
state do not gain persistence. It does not migrate live sessions, launch/retire
Hosts, retry across Hosts or restore provider state. A different UUID or store
is a separate intent; there is no semantic deduplication across them.
Keep the same store when moving/renaming a configuration if recovery is needed.
There is no automatic store discovery, retention UI or backup/restore workflow.

See [local verification](../verification/2026-10-20-creation-intents.md).
