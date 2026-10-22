# Navigate observed user prompts without fetching history

Accepted, 2026-10-22. Paseo's frozen `ce67efa9` chat-outline model and hook
separate exact Host/session/timeline identity from scrolling and avoid outline
work on every streamed delta. Its full daemon prompt index and fetch scheduling
are broader than Fleetpane's current bounded history contract.

Fleetpane provides **Loaded prompts** beside earlier-history and latest actions.
The menu computes only when opened, from the selected shared Thread, preserving
turn/item order. It contains explicit `userMessage` entries only; assistant,
tool and unknown future types are excluded. Preview text collapses whitespace
and is bounded to 90 Unicode scalar values; original content is unchanged.
Image-only/empty messages remain navigable and say they have no text. A malformed
known prompt gives a visible menu error rather than silently disappearing.

Selection revalidates the original Host incarnation, session, turn, user item and
rendered row. It scrolls directly to that row with a ListOffset, suspending tail
follow. **Jump to latest** restores tail mode. No provider action, control lease,
context resume, send, history fetch or implicit review is performed. Existing
history loading remains explicit; reopening the menu reflects the loaded page.
Only navigation controls wrap to preserve reachability in smaller viewports.

No full-session prompt index, automatic jump fetch, active-prompt rail, unread
counter, persisted scroll, search or cross-platform acceptance is implied.
