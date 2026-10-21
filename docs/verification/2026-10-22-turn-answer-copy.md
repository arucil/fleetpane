# Loaded answer copy — 2026-10-22

A native turn header copies assistant text from that exact loaded turn. Shared
tests cover colliding IDs in different turns, Chinese/CRLF/trailing spaces,
multiple ACP text blocks, non-text/future blocks, excluded user/tools/plans/
reasoning, absent turns, malformed text and multiple empty messages.

An isolated Linux X11 desktop with a synthetic provider verifies an exact
60-byte mixed Markdown/ACP answer. A paginated turn copies its latest 16 loaded
messages as exactly 286 bytes; after explicit Load earlier, all 20 copy as exactly
358 bytes. The visible notice distinguishes loaded scope and excluded content.
A turn with only user input displays a refusal and preserves the prior clipboard.
The initial external xclip sentinel lost ownership; acceptance uses the app's own
Copy action for a stable sentinel. No result is claimed from the failed sentinel.

Copy alone leaves the complete Host snapshot identical. Snapshots across history
opens differ due to normal read operations and are not presented as mutation
proof. A 31-byte Chinese multiline draft survives copying, session switches,
900×560 interaction and normal window replacement; reopened answer is exact.
The owned workspace is stopped. [Receipt and raw evidence](assets/turn-answer-copy/receipt.json)
bind source and binary hashes, clipboard bytes, screenshots and test logs.

Workspace tests, 36 desktop tests, build, formatting and strict all-target Clippy
pass. Commands: `cargo test --workspace --locked`, `cargo clippy --workspace
--all-targets --locked -- -D warnings`, and the equivalent test/build/Clippy with
`--manifest-path apps/desktop/Cargo.toml`. No automatic earlier-page retrieval,
complete tool/media export, new external model or other-platform acceptance.
