# Device-local alert policy and truthful attention

Accepted 2026-10-22. PRD ATT-04/05 requires complete mute without losing the
ability to inspect unresolved requests. The chronological Paseo ledger identifies
notification routing (including entry 2573); it does not authorize adopting its
daemon ownership model or treating every streamed delta as an alert.

Use a typed, UI-independent local policy with a global mute and additive event,
Host-endpoint and exact session scopes. Session identity includes endpoint, Host
incarnation UUID and original provider thread ID. Host rename/reordering does not
change endpoint policy. A replacement Host never inherits session rules. Global
unmute preserves narrower rules. Keep at most six unique kinds, 64 Host endpoints,
512 sessions and 512 KiB of serialized preferences; reject malformed, duplicate,
unknown and excessive values explicitly. Unknown provider requests remain a
separate `other` category and retain their original payloads.

Store preferences in schema 15 of the existing private local SQLite store,
separate from Host-owned execution and controller review receipts. GUI and CLI
use the same validation and compare-and-set write. GUI uses `--preferences-dir`
or the existing device preference default, shared with appearance. A successful
save precedes policy display. Background refresh observes other-window changes;
it cannot release an in-flight save, overwrite a newer revision or dismiss a save
failure. A failed save retains the previous record and requires explicit reload.
Normal window close waits for a write or displays its failure. OS-forced process
termination cannot provide that guarantee.

Observe configured Hosts every two seconds while the workbench window is open,
using the existing bounded snapshot inventory without acquiring control. This
never opens provider conversations. The Attention alert count includes allowed
pending requests and allowed unreviewed terminal results. It is separate from the
raw queue, diagnostic pending counts and explicit review counts. Missing reads
remain unavailable rather than a zero. Muted rows remain selectable, copyable,
answerable through their original controls and available to Next request. Muting
does not mark results reviewed or change failed/interrupted outcomes.

This slice implements local policy and counts only. OS notification delivery,
sound, tray/dock integration, workspace-specific rules, delivery deduplication,
request age and persistent delivery inbox remain open. Do not advertise complete
ATT-04/05 acceptance until those outputs share this policy and have independent
runtime evidence. CLI policy edits alone do not deliver notifications.
