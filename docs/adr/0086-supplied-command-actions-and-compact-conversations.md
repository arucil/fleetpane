# 0086: Supplied command actions and compact conversations

Accepted 2026-10-22.

Use current Codex `commandActions` as optional provider descriptions. Preserve
all read/list/search/unclassified actions in order, retain future payloads and
show malformed known actions as invalid items. Keep unavailable metadata distinct
from an explicitly empty list. Never guess actions from shell syntax or treat a
combined command output as the contents of its first file. Copy retains original
item fields, including unknown extensions and null exit codes.

Each supplied read offers explicit navigation through existing bounded workspace
file commands. Opening a link observes current local bytes, independently of the
historical command output; it does not execute the command.

Below 680 pixels of window height, move optional session settings, rename, tasks,
MCP discovery and composer settings into explicit Session options. Keep errors,
attached-image state, pending questions, permissions, composer and Stop reachable.
The options use the existing bounded interaction scroll region. Larger windows
retain their expanded controls. This is presentation state, not a preference or
a change to Host ownership.
