# Native image drops — 2026-10-22

The Host view accepts local image paths from the native drag/drop system. Picker
and drop use one importer. The new test exercises ordered additions, duplicate,
corrupt trailing file, symlink, directory, empty and oversized batches while
preserving the original image selection. Desktop tests pass 36 tests; desktop
format, build and strict Clippy pass. `python3 scripts/images-smoke.py` passes
Codex/ACP exact-payload, recovery and unsupported-capability checks.

An owned X11 workspace uses a real GTK drag source offering `text/uri-list`.
The anonymous target initially received no callback; the stable GPUI element ID
fixes it. A Chinese filename containing literal percent/hash characters imports
with exact raw hash and dimensions. A valid image followed by a corrupt one is
refused with visible error and identical draft revision. Adding three valid
images preserves order; a fifth is refused without changing the saved draft.

Normal close and reopen retain all four images and the exact 27-byte multiline
Chinese draft. At 900×560 the image strip scrolls and Send remains reachable.
One explicit Send delivers the exact text and four `localImage` files whose raw
hashes match the saved assets. A final-build drop works after that send and leaves
one image-only draft. The window closes normally; the owned workspace stops.
[Raw evidence and hashes](assets/image-drop/receipt.json) record the fixtures.

Native evidence covers Linux X11 and a synthetic Codex provider. ACP uses the
same drop/import path but has only process-level image evidence here. Wayland,
macOS/Windows, IME, slow-import close, initial new-session attachments and remote
URI fetching remain unaccepted. A rejected batch can leave validated immutable
assets in the bounded local store; it does not partially edit the draft.
