# Rich conversation rendering — 2026-10-19

Local working-tree evidence on `feat/session-host-foundation`, based on
`973b0ed883c25356a9f0849681333919d4497387`. Rust 1.98.1, GPUI Kit 0.7.3,
matching GPUI engine 0.3.9; debug builds. This follows the
[real-history rendering diagnosis](2026-10-19-real-workspace-rendering.md).
The implementation is described in [ADR 0006](../adr/0006-rich-conversation-items.md).

## Reproduction

Generate eight synthetic conversations and a local SVG, then start a throwaway
Host using the same provider protocol as the desktop:

```sh
python3 scripts/render-fixture.py /tmp/fleetpane-render-example
cargo build -p fleetpane-host --locked
cargo build --manifest-path apps/desktop/Cargo.toml --locked
target/debug/fleetpane-host serve /tmp/fleetpane-render-example/host/control.sock /tmp/fleetpane-render-example python3 scripts/provider-fixture.py --history /tmp/fleetpane-render-example/history.json
```

In another terminal:

```sh
apps/desktop/target/debug/fleetpane-desktop --host /tmp/fleetpane-render-example/host/control.sock
```

Use an isolated desktop and stop this Host after checking. No account, model or
external service is needed. Fixture commands are display data; selecting them
does not execute the commands. The synthetic provider also supports creating a
new conversation and echoes prompts in three streamed chunks.

## Native window results

The isolated Linux workspace used X11 at 1440 × 1000 and Mesa Lavapipe 26.2.5.
Windows were checked at 1200 × 900 and resized to 1000 × 720. Inputs and clipboard
operations stayed inside that workspace. Public screenshots below contain only
synthetic data.

| Trigger | Observed result | Evidence |
| --- | --- | --- |
| Open Markdown sample | Heading, emphasis, strike, inline code, list, quote, link, GFM table, Rust fence and Chinese glyphs rendered | [Markdown](assets/rendering-markdown.png) |
| Copy assistant Markdown | Clipboard equals original Markdown, including markers and newlines | Native clipboard readback |
| Open command sample | Compact command, status, duration; exit 1 is visibly distinct from exit 0; output initially collapsed | [Command cards](assets/rendering-commands.png) |
| Expand 2,000-line command output and scroll inside it | Output occupies a bounded region; header stays fixed; end shows the 16 KiB preview notice | [Preview boundary](assets/rendering-output-limit.png) |
| Copy the large command item | Parsed clipboard contains all 2,000 original output lines, including Unicode; 32,178 characters in the full JSON representation | Native clipboard readback |
| Expand MCP result and scroll details | Arguments, multiline text, tool image and structured result accessible; scrolling details leaves its header in place | [MCP image/result](assets/rendering-mcp.png) |
| Open file-change sample and scroll to its end | Update, add with a space in the path, and delete use line numbers and colored changes; the deleted line and missing-final-newline notice are reachable | [File changes](assets/rendering-diff.png) |
| Open image sample | Local SVG and user-message data URL image display with preserved proportions | [Images](assets/rendering-images.png) |
| Open image failure sample | Missing file and malformed base64 show explicit unavailable/error text | [Image failures](assets/rendering-image-errors.png) |
| Open reasoning/unknown sample and expand details | Empty reasoning omitted; nonempty summary and unknown payload retained and inspectable | [Reasoning/unknown](assets/rendering-reasoning.png) |
| Scroll up through 100 command cards | The oldest Markdown item is reachable | [History start](assets/rendering-history-start.png) |
| Paste multiline Chinese Markdown into a new synthetic session and send | Composer clipboard equals input; streamed response renders heading, emphasis, inline code and fence; assistant Copy equals the complete echoed source | [Streaming](assets/rendering-streaming.png) |
| Resize to 1000 × 720 | Markdown/table/code and composer remain usable; diff can still scroll to its last line after width invalidation | [Narrow window](assets/rendering-narrow.png), [resized diff](assets/rendering-diff-resize.png) |

The first streaming probe used synthetic key typing, which did not preserve its
Unicode/newline input. It was repeated with clipboard paste and exact composer
readback before sending. The accepted streaming evidence is from that repeat.
This is paste and rendering evidence, not IME acceptance.

## Real saved history

Reopened the same copied real conversation from the earlier diagnosis: 74 items
in two turns, including 24 command calls, 14 MCP calls, two file-change records,
two local-image records and 19 empty reasoning records. The fresh test Host used
the private copied `CODEX_HOME`; the original data and existing runtimes remained
untouched. No real prompt or command was sent.

The final build rendered real Markdown, compact command and MCP cards, an update
diff with the supplied line numbers, and the existing local PNG. Empty reasoning
records no longer occupied rows. A multiline real shell script exposed another
height problem during testing; its first-line summary now replaces the full
script on the collapsed card, while details and Copy retain the script.

Real tool arguments, source text, IDs and images remain outside this public
repository. The final real diff/image and wheel-comparison captures are retained
in the private local evidence directory. Synthetic screenshots demonstrate the
same public rendering paths without publishing that data.

## Scroll regression

After adding nested detail/diff viewports, a native test reproduced the inner
viewport and outer timeline both responding to one wheel gesture. The item header
could leave the window, making a diff appear empty. In pinned GPUI 0.3.9,
`elements/list.rs` registers its scrolling in the bubble phase explicitly so a
child can stop propagation. The generic div scroller does not consume the event.

Scrollable details now stop propagation using their retained scroll handle;
the diff viewport occludes underlying scroll hitboxes. Rebuilt and repeated the MCP, large output
and real diff checks. The tool header stayed in place while its inner content
scrolled. Scrolling the timeline gutter still navigated older conversation rows.
This is manual native regression evidence; no automated wheel-input test exists.

A separate first-open check found the multi-file diff stopped before its final
deleted line. The End key exposed that line, and wheel navigation then worked.
Kit's internal list uses zero overdraw; GPUI 0.3.9 `List::prepaint` discards cached
heights and initial uniform hints on the first layout or a width change. The
remaining offscreen rows consequently contributed zero to the scroll range.
The desktop now [patches the published Component 0.7.3 source](../../apps/desktop/vendor/README.md)
to use 600 px of overdraw. No engine/toolkit version upgrade is involved.
Native regression checks open a fresh diff without focusing its body, wheel to
the deleted line, wheel back, resize, and repeat. Public source and license are
retained with the patch; real history remains outside the repository.

## Checks

```sh
cargo fmt --all --check
cargo test --workspace --locked -j4
cargo clippy --workspace --all-targets --locked -j4 -- -D warnings
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
cargo test --manifest-path apps/desktop/Cargo.toml --locked -j4
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -j4 -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked -j4
python3 scripts/session-smoke.py
uvx --offline ruff check scripts/provider-fixture.py scripts/render-fixture.py
uvx --offline ruff format --check scripts/provider-fixture.py scripts/render-fixture.py
python3 -m py_compile scripts/provider-fixture.py scripts/render-fixture.py
git diff --check
```

Root workspace: **25 tests passed**. Desktop: **four tests passed**, covering
Kit patch parsing, Unicode preview/full-copy behavior, invalid image data and
long-command summaries. Shared content tests additionally cover known-invalid and
unknown items, mixed message attachments, MCP content, add/delete/update/rename
semantics and final newlines. Build, Clippy and the synthetic process smoke test
passed. Desktop pure tests are now included in the Linux CI job; remote CI was
not run as part of these local checks.

All test desktop processes exited 0 after Alt+F4. Final desktop logs contained
the known software-rendering DRI3 warning and no panic. The isolated workspace,
its two Hosts and their provider children were stopped; process readback found
no remaining process using the copied `CODEX_HOME`. Private evidence remains in
a local 0700 directory outside this public repository.

## Acceptance limits

Covered rendering works in the local Linux native window. This does not establish
all Markdown constructs, HTTP image loading, hostile image decode bounds, every
image format, full-size viewing, image upload/send, dedicated web-search cards,
terminal emulation, authenticated LLM/tool execution, IME, accessibility or other
desktop platforms. Histories retain the existing 500-item / 2 MiB limit and
256 KiB provider-frame cap. Large-history pagination and release responsiveness/
memory comparisons remain pending. Copy is complete within those transport bounds.
