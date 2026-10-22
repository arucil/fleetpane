# Conversation image viewer

2026-10-22, Linux X11 hidden workspace `fp-model-search`, debug Rust desktop.
Synthetic local and supplied data-URL images; no model calls or live user sessions.
[ADR 0104](../adr/0104-conversation-image-viewer.md) records toolkit ownership and
source semantics. Desktop formatting, 39 tests, strict all-target clippy and build
passed with the separate manifest and `--locked -j2`; final combined logs are
retained alongside the context-usage evidence.

| Workflow | Native evidence and result |
| --- | --- |
| Supplied encoded image | [Fit dialog](assets/image-viewer/dialog-fit.png) and [100%](assets/image-viewer/actual-after-frame.png), distinct geometry |
| Escape and focus | [Dialog closed, Chinese multiline draft retained and typing resumes](assets/image-viewer/escaped.png) |
| Copy source | Exact original 406-byte data URL and 43-byte local Unicode path; no rewritten URL or decoded-byte claim |
| Reopen | [Normal window replacement retains the unsent draft](assets/image-viewer/images-reopen-ready.png) |
| Large local image | [2000×1200 intrinsic view scrolled to bottom-right marker](assets/image-viewer/large-scrolled.png), both axes supported |
| Small Fit | [900×560 modal remains bounded and its controls usable](assets/image-viewer/large-small-fit.png); Close returns to conversation |
| Refused remote URL | [Visible construction error and no View action](assets/image-viewer/large-failures.png), no implicit URI resource |
| Host isolation | [Complete Image Host snapshot unchanged](assets/image-viewer/verification.json) across viewing/copy/mode changes; independent pending Hosts remained running |

Missing local files display the toolkit's explicit failure text. Source paths
remain live external file references; they are not immutable byte snapshots.
Arbitrary zoom, downloads, external image networking, other platforms, full
accessibility and installed-provider image acceptance remain outside this slice.
