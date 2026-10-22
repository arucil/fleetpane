# Native provider choice search — 2026-10-22

Codex next-message models and ACP select configuration use Kit searchable lists.
Name, exact provider ID and description all filter; labels retain a second ID
line and current selection. The synthetic 72-entry Codex fixture includes two
identical display names and one hidden entry. Native screenshots cover unique
description, ID, duplicate-name and hidden/no-match queries, keyboard selection,
Escape, scrolling and 900×560.

The retained provider transport records exactly one prior explicit synthetic
Send with model `wire-6`, provider effort `future-effort` and unchanged Chinese
multiline text. Later search does not send. ACP selection has a durable
`acp/config/set` receipt for `model-a`; its rendered confirmed value changes.
Taking control from a separate CLI while a palette is open makes confirmation
fail visibly and return to read-only status instead of using the old lease.

Normal window close/reopen keeps the 21-byte ACP and 32-byte Codex drafts;
clipboard readback matches both strings. An initial copy probe still targeted the
palette's old search field and returned `model-a`; that was not draft evidence.
After explicitly opening saved history and focusing the composer, exact draft
Copy succeeds. The independent pending ACP request and provider PID are unchanged.

`cargo test --manifest-path apps/desktop/Cargo.toml --locked -j 2` passes 36 tests.
Strict all-target desktop Clippy, formatting and native build pass. Existing
shared model validation remains the selection boundary. [Receipt and synthetic
artifacts](assets/model-search/receipt.json) bind source and binary identities.
This does not verify installed providers, external models, IME or other platforms.
Catalog refresh while open, favorites and persisted new-session defaults remain
open. Codex next-message overrides remain window-local.
