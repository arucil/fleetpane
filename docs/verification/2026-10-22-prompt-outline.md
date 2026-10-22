# Native navigation to loaded user prompts

Linux X11, 2026-10-22, hidden `fp-model-search` desktop; synthetic Codex and ACP
providers. [ADR](../adr/0106-loaded-prompt-navigation.md),
[source/binary receipt](assets/prompt-outline/receipt.json).

The shared projection preserves user-item order and exact turn/item identity;
known malformed prompts fail visibly. Two new tests cover duplicate IDs across
turns, Unicode/multiline previews, non-text input, stale identity, malformed
payloads and unchanged source content. All 313 workspace tests, 39 desktop tests,
both format checks, strict all-target Clippy and desktop/Host builds pass.

| Workflow | Native evidence |
| --- | --- |
| Ordinary loaded history | [15 prompts](assets/prompt-outline/menu.png) over 14 turns; assistant paragraphs do not enter the menu |
| Exact user message | [First prompt](assets/prompt-outline/first-prompt.png), then explicit latest action restores tail following |
| Same-turn follow-up | [Separate follow-up message](assets/prompt-outline/follow-up.png) |
| Non-text prompt | [Image-only message](assets/prompt-outline/image-only.png); fixture image is deliberately invalid and retains its visible error |
| Small viewport/long menu | [Scrolled to last entries](assets/prompt-outline/scrolled-menu.png), [prompt 14](assets/prompt-outline/prompt-14.png), composer and Send remain reachable |
| Empty menu | [Zero loaded user prompts](assets/prompt-outline/empty-menu.png) |
| Explicit paging | [One](assets/prompt-outline/paged-open.png), [two](assets/prompt-outline/paged-two.png), [three](assets/prompt-outline/paged-three.png) prompts; loading history is separate from menu selection |
| ACP pending request | [Menu](assets/prompt-outline/acp-menu.png), [exact prompt](assets/prompt-outline/acp-prompt.png), original pending permission retained |
| Draft retention | [Normal window replacement](assets/prompt-outline/reopened-draft.png), exact [42-byte clipboard](assets/prompt-outline/clipboard.json), including CRLF/trailing whitespace |

Navigation-only full snapshots are exactly equal before/after for the paged Host
and independent ACP Host; ACP's own menu/jump also preserves its full snapshot.
The original nonpaged sequence increases only with explicit session inspection;
no synthetic Send effects file exists. No permission was answered.

The ACP fixture Host predates the contextUsage implementation and correctly
rejects that unrelated new query; the error remains visible. This navigation
check does not accept old-Host context telemetry compatibility. An initial
window-launch wait expired while the process was still starting; subsequent
observation found its window and exact retained draft, without restarting it.
End did not jump to the final menu entry; the accepted last-entry check uses
scrolling and explicit selection. No End-key claim is made.

Full-session prompt indexing, automatic jump fetch, active-prompt rail, persisted
scroll, native forced Host replacement, IME, external models and other platforms
remain unverified or missing. Existing Host identity validation is not live
session transfer acceptance.
