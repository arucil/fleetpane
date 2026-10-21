# Clipboard image input — 2026-10-21

Implemented encoded image paste at the native message composer. It uses Kit's
existing hook and the same private asset store and revisioned draft as the
picker. Ordinary text paste is unchanged. See [ADR 0053](../adr/0053-clipboard-image-drafts.md)
and the [artifact/source receipt](assets/clipboard-images/receipt.json).

## Verification

- `cargo test --workspace --locked`: 165 tests passed. The attachment store test
  additionally exercises PNG/JPEG/WebP byte import, exact durable reload, identical
  file/clipboard asset identities, invalid data/name/size refusal and preservation
  of earlier assets. Existing quota/lock/file checks still pass.
- `cargo test --manifest-path apps/desktop/Cargo.toml --locked`: 19 tests passed.
  The new clipboard policy case checks text fallback, mixed image/text payloads,
  pre-copy count/size bounds and unsupported formats.
- Strict Clippy for the workspace and desktop, both format checks, Host/desktop
  builds and `git diff --check` passed. Raw logs are linked in the receipt.
- `python3 scripts/images-smoke.py`: supported ACP, Codex-shaped fixture and ACP
  without image capability passed, including file imports after the shared-store
  refactor. This process test uses the newly built Host.

An owned hidden Linux X11 workspace, `fleetpane-clipboard-20261021`, ran the real
desktop with synthetic providers. `xclip` supplied image MIME payloads only inside
that workspace; it is a test producer, not an application dependency.

1. The pre-change desktop left a PNG paste absent from its composer. The candidate
   shows a preview and saved image descriptor; [before](assets/clipboard-images/before-paste.png),
   [after](assets/clipboard-images/after-paste.png).
2. Native text paste retained exact Chinese, newline and two-space indentation
   alongside the image. Duplicate, damaged-image and unsupported-GIF pastes each
   showed an error and left the exact saved draft/revision unchanged. Four distinct
   images saved in order; the fifth was refused without changing the draft.
3. Normal close and replacement of the desktop retained all four images and exact
   text. Host PID 3054546, provider PID 3054549 and Host incarnation stayed the same.
   See [reopened draft](assets/clipboard-images/reopened.png).
4. Native Send dispatched text plus four images exactly once; a later image-only
   paste/Send dispatched once. Provider input text, ordered asset SHA-256 and byte
   counts matched saved drafts. Both confirmed sends cleared text/images. Raw wire
   input and the [assertion receipt](assets/clipboard-images/exact-send.json) are retained.
5. A separate synthetic ACP Host without advertised image capability refused native
   PNG paste, preserving its text-only draft at revision 1. See
   [capability refusal](assets/clipboard-images/unsupported-provider.png).

The first native Host retained the previous binary while the desktop used the
new byte-import implementation; its existing asset-load/send path was exercised.
The separate ACP Host and process smoke used the newly built Host. Both serving
binary hashes are recorded. Four desktop instances closed normally with status
0. All four owned Host/provider PIDs exited, both control sockets disappeared,
and the hidden workspace stopped.

## History inspection and limits

Paseo root review advances from 161 to 164. Eight selected paths were read: 162
replaces console-only outcomes with assertions; 163 moves/extends the stream
harness into Vitest; 164 extracts picker permission/pending-result handling and
a concurrent-open guard. Its historical crash was not reproduced. These source
reviews do not establish historical test/runtime results. See the
[separate review receipt](assets/paseo-replay/clipboard-images/receipt.json).

Native clipboard acceptance here covers PNG on Linux X11 and synthetic providers.
Other formats have core/policy checks, not native clipboard producer acceptance.
No fresh external Codex/DSH vision call, remote CI, Wayland/macOS/Windows, file-path
paste, drag/drop, mixed-text insertion, clipboard latency or oversized native
clipboard acceptance is claimed. A before-build executable hash was not captured;
the candidate binary and changed sources are hashed. Wider media, garbage
collection and Session Host retirement remain open.
