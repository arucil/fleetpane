# Supplied resource rendering — 2026-10-21

ACP/MCP `resource` and `resource_link` content now has typed projections and
native cards. The initial `cargo test -p fleetpane-session
embedded_resources_and_links_are_recognized_in_tool_results` failed with
`resource rendered as raw JSON`; it passes after implementation.

The [receipt](assets/resources/receipt.json) binds source files and compiled
artifacts to local evidence. Selected synthetic screenshots and process/clipboard
receipts are included alongside it. Private raw evidence is at
`/tmp/fleetpane-replay-151-20261021`; none is external-provider data.

## Engine and process checks

- `cargo test --workspace --locked`: 165 tests passed.
- `cargo test --manifest-path apps/desktop/Cargo.toml --locked`: 16 tests passed.
- Strict `cargo clippy --all-targets --locked -- -D warnings` for root workspace
  and desktop passed, as did both builds, format checks and `git diff --check`.
- `python3 scripts/resources-smoke.py --evidence-dir DIRECTORY`: 16 cases passed,
  eight each through actual Codex-shaped MCP and ACP Host I/O. Cases cover exact
  text, long text, Markdown, empty text, binary, links, invalid and future content.
  Every supplied block matched after reload. ACP Host/provider replacement
  recovered all eight exact turn histories without a prompt replay.
- `python3 scripts/acp-tools-smoke.py`: existing pending/update/replacement,
  falsy-output, stable-ID and archived-snapshot checks still passed. Python fixture
  modules compile. Unknown metadata and both payload fields are covered in Rust.

These are synthetic provider transports. The MCP fixture's desktop history is
seeded from its captured completed results; this does not establish new real
Codex persistence behavior. The ACP restart assertion uses Fleetpane's existing
observed archive at the same private Host endpoint.

## Native acceptance

Hidden workspace `fleetpane-resources-20261021`, 1300 × 900 native window on a
1400 × 1000 display, GPUI Kit debug build:

- MCP cards: literal text; Chinese Markdown heading, emphasis, table and code;
  long-text bounded detail scrolling and visible truncation; empty contents;
  binary encoded length; resource link name/title/MIME/zero size; explicit
  malformed-content error; inspectable future JSON with false/zero values.
- Ten clipboard reads matched source strings exactly, including CRLF, trailing
  spaces, Chinese, empty string, base64 and an opaque URI. Short text is 28 bytes,
  long text 13,676 bytes, Markdown 137 bytes, binary base64 8 bytes and URI 38 bytes.
- Reopened ACP archive cards showed text, long text and a link. Native Reload
  conversation then re-expansion retained the exact 28-byte text. The fixture's
  prompt counter remained eight throughout archived inspection/reload; Resume
  context was not invoked.

One initial ACP desktop setup used a new control-socket directory and therefore
a separate empty archive. That attempt is excluded from ACP acceptance. It was
corrected to the original task-owned endpoint. The first smoke attempt also had
a harness-only nonexistent snapshot-key lookup; the final script waits for the
actual opened turn's completed status. No production workaround was introduced.

Both native apps exited normally with code zero. All three native Host/provider
groups and the smoke's three groups stopped; task sockets disappeared and the
hidden workspace stopped. Existing user sessions and desktop were untouched.

## Limits

No resource fetching/browsing, binary decoding/preview, URI validation, audio
playback or outgoing embedded-context creation is provided. Copy retains original
data even when preview metadata/text is shortened. A malformed block currently
surfaces the existing whole-result projection error; sibling content remains in
full-item Copy. This is Linux synthetic acceptance, not fresh external model,
remote CI, macOS/Windows, performance or complete Paseo parity acceptance.
