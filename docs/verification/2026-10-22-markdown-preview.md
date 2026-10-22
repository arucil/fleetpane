# Native Markdown file preview

2026-10-22, Linux X11 hidden workspace `fp-model-search`, synthetic workspace files.
[ADR 0102](../adr/0102-markdown-file-preview.md) defines Source/Preview identity,
confined links and the authoritative in-memory image override.

Desktop formatting, 39 tests, strict all-target clippy and native build passed
with `--manifest-path apps/desktop/Cargo.toml --locked -j2` (format uses `--check`).
The exact same owned Rust file is included in the combined build/test receipt.

| Workflow | Native evidence and result |
| --- | --- |
| Markdown presentation | [Links, Chinese, table and disabled image placeholders](assets/markdown-preview/preview-links-images.png) |
| Explicit source link | [Displayed-file-relative link opens the target source](assets/markdown-preview/link-readback.png) |
| Small viewport | [900×560 preview](assets/markdown-preview/small-preview.png) |
| Refused scheme | [Visible error](assets/markdown-preview/small-scheme-error.png), no fallback navigation |
| Reload | [Changed source](assets/markdown-preview/reloaded-source.png), then [fresh preview](assets/markdown-preview/reloaded-preview.png) |
| Larger document | [Rendered and scrollable](assets/markdown-preview/large-preview.png); 94 KiB Copy not separately accepted |
| Original Copy | [Exact original content](assets/markdown-preview/clipboard-main.json), including syntax and line endings, matches captured README.MD bytes |
| No automatic images | Local loopback probe recorded zero requests while remote/HTML image fixtures rendered placeholders; this is focused probe evidence, not all-network instrumentation |
| Session isolation | [Before/after comparison](assets/markdown-preview/verification.json) preserves Codex/ACP Host/provider identities, pending requests, active turns, journal and operation identities |

No installed provider or model call was needed for file rendering. Heading-only
anchors, embedded images, Markdown editing, live file watching, large-file
streaming, other-platform behavior and IME are not accepted.
