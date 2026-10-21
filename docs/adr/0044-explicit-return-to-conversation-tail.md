# ADR 0044: Explicit return to the conversation tail

Status: implemented; Linux native verification recorded separately.

Paseo replay ordinal 127 adds a return-to-bottom control when its scroll view is
away from the end. Fleetpane already uses Kit's virtual variable-height list in
tail-follow mode. User scrolling pauses following; row changes preserve the
reading position. There was no explicit control to resume following.

Show **Jump to latest** beside history paging when loaded rows exist and the list
is not following its tail. Read this intent from `ListState::is_following_tail`,
not a pixel-distance threshold: geometric end detection can be unknown while
virtual rows have unmeasured heights. Kit already notifies the owning view on
scroll, so no extra scroll observer, duplicated follow flag or polling is needed.

Clicking sets `FollowMode::Tail`. Kit anchors at the end and follows the bottom
of a growing last row; the button disappears. This is local navigation, usable
while work is running or the controller is read-only. It does not fetch history,
resume a provider context, send a prompt, clear a draft or answer a permission.
Existing page/cache limits and explicit Reload behavior still apply. New session
selection retains its existing reset-to-tail behavior.

The control is in the navigation row rather than over message content or tool
actions. It works independently of whether that provider exposes history paging.
It does not guarantee a minimum timeline height in small windows, animate the
scroll, count unread messages or preserve scroll positions across window restart.

[Verification](../verification/2026-10-21-history-navigation.md).
