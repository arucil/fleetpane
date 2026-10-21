# Bounded native composer

Later layout allocation: [ADR 0046](0046-share-conversation-space-with-details.md)
addresses the minimum-window conversation reading gap while retaining this input.

Status: implemented for the covered Linux UI slice, 2026-10-21.

Paseo history 111 adds dynamic input height. Fleetpane's fixed six-rem textarea
left unnecessary space for short drafts and could not adapt to multiline input.
Use GPUI Kit's existing auto-grow state, preserving its text, wrapping, selection,
scrolling and draft persistence. Growth starts at one row and stops at six rows.
Windows below 680 pixels use three rows so pending requests and Stop remain
reachable at the supported 900×560 minimum window size. A window-bounds observer
changes that limit only when crossing the breakpoint. It neither replaces text
nor rebuilds the input entity. Additional text scrolls inside the editor.

The conversation column permits vertical shrinking. The task/status/approval
region permits shrinking and owns its scrolling; the composer/button region
does not shrink. Permission details retain their existing maximum viewport share.
This lets intermediate information yield space while Send and Stop remain fixed.
The editor's own row limit governs its viewport: an outer CSS max-height alone
does not resize Kit's text layout and can paint text beyond its frame.

Existing exact-text saves, send receipts, permissions and Host ownership remain
the authority. Layout changes do not send or cancel work. This does not implement
attachments, keyboard submit preferences, IME acceptance, activity sorting or a
general pane/layout system. See [native verification](../verification/2026-10-21-bounded-composer.md).
