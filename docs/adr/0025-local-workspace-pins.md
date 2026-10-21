# 0025: Durable local workspace pins

Accepted 2026-10-20.

Paseo's `workspace.pin.set.request` takes an explicit workspace identity and
boolean. Its sidebar acceptance checks one transition per action and preserves
the original workspace after unpinning. The reference is the inspected fork at
`a486b0558168c08890890e787d55cfb88c0d7256`, particularly
`packages/protocol/src/messages.ts` and
`packages/app/e2e/browser/sidebar-workspace-pin-shortcut.spec.ts`.

Fleetpane stores a pin against the catalog's stable workspace UUID. That UUID
already separates Host endpoint and directory identity; identical paths on two
Hosts can be pinned independently. Preferences belong to the private controller
store and are shared by windows using it. Pinning performs no Host RPC, provider
dispatch, directory inspection or Git operation. An offline or replaced saved
directory can still be unpinned. Selecting **Use workspace** follows the existing
directory-identity and configured-endpoint checks, prepares the named Host/cwd,
and leaves control acquisition and session creation explicit.

The shared engine supports explicit set/unset and a bounded independent pin list.
Repeated set requests neither create duplicates nor change order. The most
recently added pin appears first. The 32-pin budget is enforced in the same
SQLite transaction as insertion, across processes. At capacity, existing pins
remain idempotent and a new pin fails visibly. Unpin removes only the preference.
The paged workspace catalog keeps its original identity and ordering. Pins remain
accessible even when their entries are outside the currently loaded catalog page.

Controller store schema 5 adds `workspace_pins`, referencing `workspaces` with a
unique workspace ID. Schemas 1–4 upgrade transactionally while retaining draft
revisions, creation bindings and worktree records. Missing tables in a versioned
store are errors, not instructions to recreate state. Older controller binaries
reject schema 5; no reverse migration is provided. Host protocols, journals and
session processes are unchanged by the preference schema upgrade.

GUI and CLI call the same engine methods. The GUI applies the returned committed
list, rather than an optimistic toggle. **Newest** or reopening the workspace
catalog reloads changes made by another window/process; live preference broadcasts
are not implemented. The pin group and ordinary catalog rows share rendering and
explicit pin actions. There are no workspace tabs, drag ordering, pin shortcuts,
automatic selection restoration or remote preference synchronization in this slice.

See [verification](../verification/2026-10-20-workspace-pins.md).
