# Device-local project display names

2026-10-23. Accepted.

Paseo at `a486b0558168c08890890e787d55cfb88c0d7256` provides a custom project
name and reset (`packages/app/src/projects/edit-form.ts`,
`packages/app/src/components/project-edit-sheet.tsx`). Fleetpane adopts that
behavior without copying code or adding project icon management.

The existing project UUID already groups a Git main checkout with its linked
worktrees by the saved common-directory identity and exact Host endpoint.
Folder registrations and different endpoints have independent projects. Schema
19 adds `project_names(project, revision, value)` with a project foreign key;
all pre-existing rows remain unchanged. Older controller binaries reject schema
19. Host protocol, journals and running provider processes do not change.

`Store::rename_project(project_id, expected_revision, value)` compares revisions
and writes in one immediate SQLite transaction. `None` explicitly resets the
name; retaining an incremented revision prevents stale reset/rename overwrites.
Names preserve exact Unicode and must be 1–256 UTF-8 bytes on one line, without
surrounding whitespace or control characters. Reads reject malformed saved
values, oversized names, invalid revisions and missing current-schema tables.
A stale revision or failed transaction preserves the saved value. There is no
automatic save retry, Host RPC or directory inspection.

Workspace reads include both independent label records. Group headings and tab
prefixes use the project label; individual workspace labels remain separate.
Default Git labels use the common-directory leaf, or its parent for `.git`.
An ordinary folder named `.git` keeps that name. The group still shows its saved
path. Bounded metadata merges compare each label's revision independently and
propagate project names to sibling tabs without changing workspace/session IDs,
paths, positions, pins, pending requests or drafts.

The native group action shares the existing name editor and GUI/CLI Store
command. Save errors remain visible, disable further name saves and block normal
window close. **Reload name** discards the edit and reads current saved metadata;
**Close name editor** explicitly acknowledges the failure without a write. Close
waits for in-flight name actions. **Newest** refreshes another window's labels;
there is no automatic other-window synchronization.

The CLI exposes `workspace STORE rename-project PROJECT_UUID REVISION NAME`
and `workspace STORE reset-project-name PROJECT_UUID REVISION`. `workspace list`
provides the project UUID and current `project_name` revision/value. The existing
workspace-name smoke covers these commands; no CI job is added.

[Acceptance](../verification/2026-10-23-project-names.md) separates engine/CLI
checks from Linux native fixture evidence. Project icons, other platforms,
installed-model acceptance and release performance remain outside this slice.
