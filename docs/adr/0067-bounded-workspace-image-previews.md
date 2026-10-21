# Bounded workspace image previews

Accepted 2026-10-21. Extends [read-only files](0026-read-only-workspace-files.md)
and [explicit navigation](0049-explicit-conversation-file-navigation.md).

Paseo history commit 194 adds gallery navigation. Fleetpane needs a complete
single-file image workflow before a gallery: opening a known image must work
without starting, claiming or resuming a provider, and must not bypass saved-root
identity checks by giving the GUI an arbitrary local path.

`Workspace::read_file` and `open_path` now return a tagged image result containing
validated PNG/JPEG/WebP identity (SHA-256, MIME, name, byte count, dimensions) and
base64 of the observed bytes. GUI and CLI share this read. No attachment import,
copy, persistent image cache, external fetch or Host mutation occurs.

The engine reads a 32-byte header through its existing descriptor-relative,
no-follow boundary. Detected image magic or a known image extension selects the
8 MiB image bound; other files retain the 256 KiB text bound. Extensions never
establish a valid image. Existing attachment inspection validates actual format,
8192-pixel dimensions, 16 Mi-pixel count and complete decoding under its 128 MiB
allocation limit. Unsupported/corrupt images are explicit unavailable results;
size refusals preserve their relevant byte limit. Original bytes are returned,
without transcoding. Reads recheck metadata and directory/file identity after
inspection. This observes a file, not an atomic snapshot against external writers.

Both file workers retain their permits across timeouts. UI commands remain
single-flight. The native command worker prepares GPUI image bytes off the UI
thread; rendering uses those bytes and cannot reopen a replaced path. The image
is absolutely positioned in a flexible bounded viewport with aspect-preserving
Contain, so intrinsic dimensions do not enlarge and clip the preview. Replacing
or closing a preview removes its GPUI asset. Failed reads clear the previous
preview. Source line anchors have explicit text-only semantics.

Base64 adds bounded memory/serialization overhead; it avoids divergent CLI/GUI
file access and is local only. No streaming/remote media protocol is implied.
Gallery thumbnails, Markdown-file embedded images, animation controls, full-size
zoom, other formats, image clipboard export, editing and performance budgets
remain separate work. Only Linux native behavior is accepted in this slice.
