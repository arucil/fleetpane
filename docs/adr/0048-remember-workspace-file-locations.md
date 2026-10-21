# ADR 0048: Remember workspace file locations without caching file contents

Status: implemented; local native acceptance is recorded separately.

Paseo replay 142 introduces a read-only file explorer; 143 moves its current
directory into session context so reopening does not always start at root.
Fleetpane already has bounded, descriptor-relative filesystem observations, but
closing its file view destroys directory and selection state. A native baseline
reproduces the return to root after leaving a Chinese-named directory.

Keep a window-local LRU of at most 32 locations in the workspace controller.
Each location contains only the last successful relative directory and selected
file path. Keys include the workspace UUID, Host socket and saved root identity
(path, device, inode). The same directory registered for another Host has an
independent location; saving a changed identity invalidates the old entry.
Do not retain hidden file views, textareas, directory entries or file bytes.

Closing the view emits its location. Reopening first lists that directory and
then reads the remembered file through the existing bounded filesystem engine.
Refresh folder follows the same sequence and preserves successful selection.
Explicit Root, Up, Go and directory clicks clear selection. Failed observations
remain visible; no automatic fallback to root, symlink following or retry is
introduced. A failed directory restore keeps the attempted path in the editor.
A missing selected file leaves the successfully loaded directory visible and
clears its unavailable preview. Returning from another screen focuses the visible
file path input, and leaving files focuses the workspace input.

This retains navigation only for the current window. Restart persistence,
cross-window coordination, directory/preview scroll restoration, Back/Forward,
deep links, file editing, media previews and recursive watching/search remain
separate work. Existing worker, byte, entry, deadline and root-confinement limits
remain authoritative. The native file view does not call or mutate Session Hosts.
