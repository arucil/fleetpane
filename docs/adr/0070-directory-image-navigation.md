# Directory image navigation

Accepted 2026-10-21.

Paseo replay entry 194 exposed a remaining file-browser workflow after single-file
previews and Back/Forward: moving between images in the same directory. Fleetpane
adds Previous image, Next image and a filename-based position/count to the existing
read-only file view.

Candidates are regular files with case-insensitive PNG/JPG/JPEG/WebP extensions,
in the existing directory listing order. This is a filename index, not proof that
every candidate decodes. Corrupt, unsupported-content and oversized candidates
keep the existing explicit preview notices; they are never silently skipped.
Extensionless images remain readable individually but do not enter this index.
Folders, symlinks and special files are excluded without reading their contents.

Each explicit step runs in the existing background command worker. It re-lists
the directory through the shared workspace engine, locates the current path in
that observation, selects one neighbor and reads just that file through the same
bounded, no-follow reader. The root device/inode check, 500-entry directory limit,
8 MiB/16-megapixel image limits and two-worker bound remain authoritative. Render
only filters already-loaded metadata; no thumbnail scans, decoding all files or
provider operations are added. There is no wrap at the endpoints.

On successful reads, the normal file selection, path, decoded image and bounded
Back/Forward history update together. File bytes are never retained in navigation
history. Replacing the selected image releases the previous GPUI image asset.
Direct path opens keep their existing demand-loaded directory behavior and offer
Load image list to explicitly observe the parent directory and reread the file.

If directory listing succeeds but neighbor selection/read fails, show the error,
retain the previous observed preview and history cursor, and retain the fresh
directory metadata. The message explicitly identifies the retained preview.
A missing current path requires explicit refresh or selection; directory changes
never silently reinterpret its index. Directory/root observation failures retain
the existing file-browser error path, which clears the preview. Reads/listings
are separate filesystem observations, not an atomic filesystem snapshot.

This slice provides sequential navigation and position/count. Thumbnail grids,
zoom, full-size media windows, keyboard image shortcuts, embedded Markdown images,
watchers, extensionless discovery and macOS/Windows native acceptance remain open.
