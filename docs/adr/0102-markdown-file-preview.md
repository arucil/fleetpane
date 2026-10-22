# 0102: Explicit native Markdown file preview

Status: implemented, 2026-10-22; covered Linux native workflows accepted in
[verification](../verification/2026-10-22-markdown-preview.md). See remaining limits below.

The bounded workspace reader already returns complete UTF-8 text up to 256 KiB.
The Files view previously displayed Markdown only in the numbered, read-only
Kit Editor. Add explicit **Source** and **Preview** buttons for `.md` and
`.markdown` files, matching extensions without case sensitivity.

## State and source identity

Source remains the default on every file load, reload and history navigation.
It keeps the existing EditorState, original-line validation, cursor anchor,
wrapping, and source history semantics. Switching presentation does not recreate
or update the editor, reread the file, or add a history entry. Wrap applies only
to Source. Preview does not claim to map rendered paragraphs to source lines.

On the first Preview click for a loaded file, create and retain one
`gpui_kit::base::text::TextViewState::markdown` entity and its observation
subscription. Render it with `TextView::new`, selection and internal scrolling.
Later renders and Source/Preview toggles reuse that state. Kit performs its own
background parsing for larger documents; the observer repaints on completion.
Kit's parser errors remain visible as “Failed to parse content” with details.
Do not catch parsing failures or replace bad content with empty text.

Starting a filesystem navigation or reload clears the old Markdown entity,
observer and placeholder asset before awaiting the reply, including failed
reads. A new non-text/non-Markdown file cannot inherit the old preview.
Preview state is window-local and is not persisted. Existing engine limits,
saved-root identity checks and bounded filesystem workers remain authoritative.

**Copy full text** always copies the loaded `Readback::content` original string,
including Markdown syntax, CRLF, Unicode and trailing whitespace. It does not
copy rendered text or the editor's normalized representation. Ordinary preview
selection uses Kit's plain-text selection behavior. **Reload file** still uses
the existing `Action::Read` and `Workspace::read_file` path and creates fresh
presentation state from the new confined readback.

## Explicit links and confinement

Only a `TextView::on_link_click` callback activates links. Decode with the shared
`fleetpane_session::files::Link::parse`; preserve its scheme, percent-decoding
and positive source-line validation. HTTP(S) opens through the existing system
URL action only after a click. Other unsupported schemes, remote file
authorities and invalid fragments show the shared error in the Files view.

Resolve local relative targets through shared `Target::in_directory`, using the
displayed file's parent under the saved workspace root. Do not derive the base
from the directory listing, editable path input or process working directory.
Preserve the decoded line on the resolved target and dispatch `Action::Open`
through `Workspace::open_path`. The engine checks the saved root identity,
workspace boundary and descriptor-relative no-follow access. It rejects parent
components (even a `../` that stays inside the root), symlinks and special files.
Do not normalize traversal away or retry an escape under another base. Failed
opens show an error and clear the failed file preview.

`file.md#L2`, `file.md#L2-L4` and `file.md:2` open Source at the original line;
range selection is not implemented. Missing source lines retain the loaded raw
text and Copy with the existing visible error. Pure fragments such as `#L2`
and heading anchors such as `#heading` are not supported by the shared decoder
and show errors. Click callbacks also check current file identity, presentation
and busy state before acting. File navigation does not invoke a model or claim
session control.

## No document image fetching

Inspection of pinned Kit 0.7.3 found that Base's default `ImageNode::source`
returns URI-backed resources for document URLs and decodes data images. Markdown
also accepts inline and block HTML images. Leaving the default in place would
allow Preview layout/rendering to fetch remote images.

Use Kit's public Base `TextView::image_source` override. Its documented and
implemented contract covers every document image in rendering and intrinsic
size measurement, and pending/failed loads never fall back to the original
document URL. The component compatibility facade does not forward this API;
use `gpui_kit::base::text::{TextView, TextViewState}` directly, retaining Kit's
installed theme and code-highlighting defaults. No vendor/dependency change is
needed.

The resolver ignores every document-supplied image URI and returns a retained
in-memory SVG placeholder, including for HTML, local, remote and data images.
It performs no filesystem access, network work or base64 decoding. The view
visibly states that images are disabled and represented by placeholders. The
placeholder uses fixed application bytes; its asset is removed with its cached
preview. No document-provided image URL reaches the resource loader through
this view. Explicit links around images still use the same click handler.

## Verification and remaining limits

| Workflow | Implementation and evidence | Native status |
| --- | --- | --- |
| Recognize Markdown files | Case-insensitive extension tests, plain/unknown extension refusal | Covered Linux fixture workflows verified |
| Source/Preview and original Copy | Retained independent entities; original Readback string stays the Copy input | Covered Linux fixture workflows verified |
| Local links and source lines | Semantic tests for displayed-directory resolution, UTF-8, percent escapes, absolute/file URLs and line metadata | Covered Linux fixture workflows verified |
| Unsupported links and confinement | Decoder error tests; traversal/absolute escape targets are preserved for the existing engine to reject | Covered Linux fixture workflows verified |
| No implicit image I/O | Pinned Kit source inspection; authoritative in-memory source override for layout and paint | Local probe recorded no automatic requests; native placeholders verified |
| Reload/loading/error cleanup | Cache cleared at navigation admission; fresh existing bounded read path | Covered Linux fixture workflows verified |

Commands for this slice, all from the repository root (only the owned Rust file
is formatted; other workers own concurrent source changes):

```sh
rustfmt --edition 2024 apps/desktop/src/files_view.rs
rustfmt --edition 2024 --check apps/desktop/src/files_view.rs
cargo test --manifest-path apps/desktop/Cargo.toml --locked -j2
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -j2 -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked -j2
```

The owned-file formatting check, 39 desktop tests, strict all-target clippy and
desktop build passed locally. Three new semantic tests cover extension matching
and shared link resolution/refusal, alongside the existing original-line test.

[Native verification](../verification/2026-10-22-markdown-preview.md) covers
explicit navigation, Source/Preview, original Copy, reload and image placeholders.
The 94 KiB document rendered, but its complete Copy was not separately accepted.

No automatic image loading, local embedded-image reads, heading navigation,
rendered-to-source position synchronization, Markdown editing, file watching,
search, large-file streaming, MDX execution or persisted presentation choice is
implemented. Rendering follows the pinned Kit Markdown implementation; this is
not a promise of browser/HTML or complete Markdown-extension parity. The
existing explicit Save a copy action remains separate; Preview and its links
add no file writes. Other platforms and IME remain unaccepted.
