# Complete read-only source preview

Status: accepted, 2026-10-22.

Paseo ordinals 1727 and 1741 add numbered, highlighted file previews and
gutter sizing. Fleetpane's textarea showed only 16 KiB from the requested line,
so earlier source disappeared and displayed line numbers could not represent
the original file. The shared reader already bounds UTF-8 files to 256 KiB.

Use GPUI Kit's existing read-only Editor for the complete loaded text. Enable
its bundled language grammars explicitly, use its line gutter and scrolling,
and disable folding so displayed source stays discoverable. Unknown extensions
remain plain text. A window-local Wrap button changes soft wrapping without
reading the file or touching its contents. Keep file actions in wrapping rows.

Validate one-based source anchors against the original newline boundaries and
move the editor cursor to that original line. Do not slice the source. Missing
lines retain the loaded file and its exact Copy action with a visible error.
Copy full text continues to use the reader's original string, preserving CRLF,
Unicode and trailing whitespace independently of editor presentation.

This retains the shared confinement, file-size and worker limits. It adds no
editing, save/conflict handling, Markdown rendering, search workflow, large-file
streaming or persisted editor position/wrap preference. Native verification is
limited to the recorded Linux X11 fixture workflows.
