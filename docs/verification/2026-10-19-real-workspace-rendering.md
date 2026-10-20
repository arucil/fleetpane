# Real saved-workspace rendering checks — 2026-10-19

This records the initial diagnosis and scroll fix. The later
[rich-rendering implementation and native recheck](2026-10-19-rich-rendering.md)
addresses the missing renderers below; the original observations remain here.

Local working-tree evidence on `feat/session-host-foundation`, based on
`973b0ed883c25356a9f0849681333919d4497387`. Rust 1.98.1, GPUI Kit 0.7.3,
matching GPUI engine 0.3.9 and installed Codex 0.163.0. This is rendering
verification of existing real history, not acceptance of new LLM/tool execution.

## Data and loading path

Selected six saved Codex histories from real workspace sessions. Original
rollouts and databases were read only. A separate temporary `CODEX_HOME` held
copies of the selected rollouts and their indexed records from `state_5.sqlite`
and `thread_history_1.sqlite`. Only matching rows were copied, using SQLite
read transactions; no full copy of the large history database was needed.
The copied metadata points to copied rollout paths. No credentials were copied,
no prompt was sent and no existing Paseo/Codex process was restarted or claimed.

JSONL-only copies did not contain all paginated history items. Including the
indexed history changed the representative thread/read result to **74 items in
2 turns**. This is evidence that this existing populated paginated history is
readable, despite the earlier new-runtime probe's `list_turns is not supported
yet` error. It does not establish support for paginating large histories.
Older legacy samples also exposed only the items reconstructed by Codex's reader;
raw rollout tool counts must not be treated as counts of displayed items.

The representative conversation contained:

| Item type | Count |
| --- | ---: |
| `userMessage` | 2 |
| `agentMessage` | 7 |
| `commandExecution` | 24 |
| `mcpToolCall` | 14 |
| `fileChange` | 2 |
| `imageView` | 2 |
| `webSearch` | 4 |
| `reasoning` | 19 |

Both image records reference local files that existed during the check. The
assistant messages contain Markdown, including bold/inline-code markers, links
and a fenced block. These records were not replaced by synthetic equivalents.

A fresh Fleetpane Host ran against the private copy. The desktop used its normal
Host list/open/resume flow; the default directory returned the representative
session. Subagent samples were not forced into the normal directory by editing
their metadata. GUI input and captures stayed in the isolated Linux workspace
`fleetpane-render-20261019`: X11 1280 × 800, window 1100 × 720, temporary Mesa
Lavapipe 26.2.5. Private IDs, source text, tool arguments, images and screenshots
are retained outside this public repository.

## Observed rendering

| Component | Observed behavior | Result |
| --- | --- | --- |
| Session list and conversation | Real session selected; its history appeared; Chinese glyphs visible | Basic loading passed for this sample |
| Assistant Markdown | Literal bold and backtick markers visible in ordinary text | Rich rendering missing; tables and every Markdown construct were not separately accepted |
| Command execution | Long JSON, escaped output/newlines and command strings; Copy action available | Tool card, terminal/code formatting and compact status missing |
| MCP tool call | Native window displayed arguments, duration and result fields as JSON | Tool card and structured result presentation missing |
| File change | JSON containing changes and patch text | Diff rendering missing |
| Local image view | `imageView` row displayed only a JSON path; source file existed | Image preview missing; missing source files did not cause this result |
| Reasoning | Empty content/summary arrays still produced a visible JSON row | Empty-item presentation needs work |
| Web search | Present in decoded history | Dedicated visual behavior not separately accepted |

Source inspection agrees with the window evidence: `Item::text()` returns plain
assistant/user text and serializes other fields as JSON. The Host timeline passes
that string to a GPUI text element. There is no Markdown, image, tool-card or diff
renderer in that path. Preserving provider data is useful for inspection, but
does not satisfy rich-component rendering acceptance.

## Scroll regression found and fixed

Before the fix, scrolling upward in this history stopped at the earliest
measured row near the end. Further wheel events did not reveal earlier items;
scrolling down still worked. The timeline had initialized
`ListState::new(..., ListAlignment::Bottom, px(0.))`.

In the pinned GPUI implementation, physical scrolling uses measured row heights.
Leading overdraw measures rows before the visible region. Zero overdraw skipped
that measurement, leaving older rows at zero height and creating the observed
scroll boundary. This was a list configuration problem, not missing history.

The timeline now uses one window viewport of overdraw. It still virtualizes the
list; it does not eagerly measure the entire conversation. Rebuilt and reopened
the desktop against the same private Host/history, explicitly took control, and
scrolled back to earlier file-change, image and MCP rows that were unreachable
before. This native before/after check is the regression evidence; there is no
automated GPUI scroll test yet.

## Checks and acceptance limits

```sh
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
cargo build --manifest-path apps/desktop/Cargo.toml --locked -j4
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -j4 -- -D warnings
git diff --check
```

Build and Clippy passed after the scroll change. Both desktop processes exited
0 after Alt+F4. The known software-rendering DRI3 warning appeared; no desktop
panic appeared. The isolated workspace and its own Host/provider processes were
stopped after verification. Private captures and the copied data were retained
locally for review, outside the repository.

Real saved-history loading and scrolling passed for this sample. Rich rendering
**did not pass**: Markdown, tools, diffs and image previews need implementation.
User-uploaded images, remote images, malformed/oversized attachments, every
Markdown construct, web-search cards, active LLM streaming, large-history
pagination, IME, accessibility, other desktop platforms, remote CI and performance
under load were not accepted by these checks.
