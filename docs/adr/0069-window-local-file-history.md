# Window-local file history

Accepted 2026-10-21. Extends [file locations](0048-remember-workspace-file-locations.md)
and [image previews](0067-bounded-workspace-image-previews.md).

Paseo history commit 194 introduced file Back navigation and directory scroll
recovery. A last-selected path alone cannot represent the sequence of locations
visited while inspecting an Agent's files. Fleetpane now keeps up to 32 visits
within one file view. Each visit contains the directory, optional file, optional
source line and top directory-entry path. No file contents or GPUI assets are
retained by this history. Closing the view discards its history; the existing
workspace/endpoint/root-scoped last-location cache remains independent.

Back and Forward use the existing confined engine listing/read commands. A
directory and optional file are read in one background operation, with input
disabled for its duration. The history cursor moves only after successful reads.
A missing file still allows displaying the freshly read directory and an explicit
error, clears the old preview and leaves the cursor unchanged. A rejected
directory does not load its files. A later click explicitly retries; no automatic
retry, provider mutation or path-access exception is added.

Accepting the same directory/file/line refreshes the current visit and preserves
Forward. A new visit after Back discards the forward branch. Eviction removes
the oldest metadata only. Source-line identity is retained within this view;
cross-view and process persistence remain outside the contract.

Directory scroll recovery uses the top visible entry's path rather than its old
ordinal, so new sorted entries do not select a different anchor. GPUI UniformList
virtualizes children; its base ScrollHandle child index is not a directory row
index. Capture uses uniform row geometry and actual offset, honoring a pending
explicit scroll. Restore finds that path in the new listing and scrolls it to
the top. If the anchor was removed, the list starts at the top. Fractional pixel
offset, file-preview scroll and recovery across closed views remain open.

Only Linux native workflows and shared offline file boundaries are accepted in
this slice. No gallery, editing, keyboard navigation binding, provider lifecycle
change or new macOS/Windows native acceptance is implied.
