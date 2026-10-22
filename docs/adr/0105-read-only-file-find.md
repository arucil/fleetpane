# Read-only file Find uses the existing Kit editor

2026-10-22. Before adding a new search implementation, source inspection and
native Ctrl+F confirmed that Kit 0.7.3 `EditorState::new` enables search by
default. The earlier missing-search status was too broad.

Expose a **Find** button for loaded text files. It explicitly switches Markdown
Preview to the retained Source entity, focuses that editor, and invokes its
public `open_search(false)` action. The existing panel handles query selection,
case matching, occurrence counts, previous/next, keyboard actions, scrolling and
Escape; no custom matcher, subscription, parsing or Host command is needed.
The button is disabled while loading or when source anchors have no valid
editor. Binary/images/oversized files do not get a text Find button.

Search uses only the already loaded, complete bounded UTF-8 source. It does not
search the workspace, query a provider, reread files or add navigation history.
Source/Preview toggles retain the source editor and its search state. New file
loads/reloads create a new entity, so their matches cannot reuse old content.
The editor remains read-only. Kit checks `is_replaceable` from read-only state
both for panel controls and replacement engine methods; Find never enables
Replace. Copy full text continues to use the original Readback, including CRLF
and trailing whitespace.

Native verification is recorded separately. Workspace-wide/terminal/Markdown
rendered-text search, regex, persisted queries, editing, IME and other platforms
are not implied by source Find.
