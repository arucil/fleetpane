# 0006: Typed conversation content and retained native renderers

Status: implemented for the covered item types, 2026-10-19. Extends ADR 0005.

## Problem and decision

The first timeline rendered `Item::text()` in a plain text element. This preserved
data but displayed literal Markdown, tool JSON and image paths. Real saved history
confirmed that the missing previews were a presentation problem.

Decode supported content in `fleetpane-session::content`, independent of GPUI.
Keep the original item fields for inspection and copying. Malformed known content
produces a visible error; unknown items and parts remain inspectable. Only empty
reasoning items are omitted. A nonempty reasoning summary is expandable.

The desktop retains one `ItemView` per stable thread/turn/item identity. It reuses
GPUI Kit `TextViewState` for streamed Markdown and `DiffState` for file changes;
states are prepared when content changes, outside the render method. The existing
variable-height, virtualized timeline continues to use viewport-sized overdraw.
This is not history pagination or a release performance claim.

`ItemView` retains an observation subscription alongside each `DiffState` and
forwards its notifications to the row. Kit's diff measures its viewport after
layout and notifies its state to repaint at the measured width; it is a
`RenderOnce` component, not a separate rendered entity. Without the subscription,
the initial zero-width list stays blank until another event redraws the row.
The subscription also carries collapse/selection changes and is dropped when
the block is replaced. The [live OpenCodex check](../verification/2026-10-19-opencodex-live.md)
records the blank first paint, root cause and native regression result.

Assistant messages and plans use Kit Markdown. User text remains literal. Command
cards expose status, exit code and duration; the first line, up to 160 characters,
summarizes long scripts. Full scripts and output are expandable. MCP cards expose
the server/tool and status, with arguments, text, image and structured results in
details. Unknown result parts remain JSON. Command output is monospace text, not
terminal emulation.

Codex file-change `diff` has different meanings by kind: add/delete carry complete
contents, while updates carry unified hunks. The adapter synthesizes headers and
add/delete line markers from supplied data, preserving final-newline semantics,
quoted paths and update move targets. It removes Codex's exact trailing `Moved to:`
display annotation when the same target is present in `kind.move_path`. It never
reads working-tree files to invent a patch. The upstream mapping is documented in
[Codex item builders](https://github.com/openai/codex/blob/main/codex-rs/app-server-protocol/src/protocol/item_builders.rs).
Kit parses and renders the resulting unified diff. Invalid patches show an error
and supplied patch text for inspection.

Local images resolve against the thread workspace; GPUI loads/decodes them
asynchronously. Data URL images and base64 MCP images are validated before being
passed to GPUI. Previews preserve aspect ratio in a fixed viewport, with loading
and unavailable states. HTTP image sources are supported by the image API but
have not received rendered-window acceptance.

## Bounds and scrolling

Text sections show at most 16 KiB with an explicit truncation notice. Copy retains
the complete original item representation within the existing history/transport
bounds. Image and diff viewports have fixed heights; expanded tool details have
a maximum height and their own scroll handle. This bounds presentation height,
not source image decode size or total retained GPU memory.

The engine bubbles wheel events to the timeline after child handlers. Its scroll
containers do not automatically consume those events. Native checks reproduced
the details/diff and outer timeline scrolling together. Scrollable details now
consume wheel events; non-scrollable details allow the outer timeline to scroll.
The diff viewport occludes underlying scroll hitboxes while its children keep
their own input handling. The row header and timeline gutter remain
available for navigating conversation history. This fixes event ownership instead
of adding delays or rebuilding the list to hide the jump.

Kit 0.7.3 also initializes its internal diff list with zero overdraw. The engine
discards initial row-height hints on first layout and width changes, leaving
offscreen rows at zero height. Native checks reproduced a lower boundary before
the deleted line; End could reveal it, proving the patch data was present.
The desktop uses a [local Component patch](../../apps/desktop/vendor/README.md)
with 600 px of list overdraw. This is the only upstream source change; it keeps
Kit 0.7.3 and engine 0.3.9. It measures nearby rows as they become reachable without
measuring the entire patch. First-open and resize wheel checks cover the fix.

## Evidence and limits

See [rich-rendering verification](../verification/2026-10-19-rich-rendering.md) for
deterministic fixtures, real saved-history checks, screenshots and commands.
Markdown constructs beyond the covered sample, remote images, image upload/send,
full-size image inspection, dedicated web-search cards, terminal behavior, large
history pagination, other platforms and release load measurements remain separate
work. Existing runtime and Host upgrade contracts are unchanged.
