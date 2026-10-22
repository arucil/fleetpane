# View an already supplied conversation image

Accepted 2026-10-22. Paseo history 2516,
`3838a8b969d87fae95e3461bab62c482704b4daf`, adds an attachment lightbox.
The scoped receipt in `docs/verification/assets/paseo-replay/2351-2580/receipts.tar.gz`
(`receipts/2516/selected.patch`, attachment-lightbox.tsx lines 15–44) establishes
metadata identity, explicit image errors and Escape cleanup. It does not establish
full historical behavior or native acceptance.

## Decision

Each conversation image whose existing source construction succeeds gets an
explicit **View image** button. The button clones that exact `ImageSource` into a
window-local Kit dialog. It does not navigate a file browser, expand a row, send a
message, modify a draft, claim a Host or answer a pending question. Row Copy and
details retain their existing meanings. No files or attachment-store entries are
written. Local paths keep their existing GPUI loader; they are not an immutable
byte snapshot, and an external file replacement remains possible.

The existing `image_url` boundary parsed data URLs but accepted every other string
as a GPUI resource URI. Refuse those non-data URLs explicitly rather than allowing
this viewer to initiate network requests. Malformed data URLs, unsupported MIME
and base64 errors remain explicit construction failures without a View button.
Construction success is not proof of successful image decoding: missing local
files and corrupt encoded images retain the toolkit's loading/failure display in
both the inline preview and dialog. There is no invented replacement image or
success status. Parsing remains in row preparation, outside rendering.

Use `WindowExt::open_dialog` on the existing Kit/Base Root installed by
`gpui_kit::open_window`. Kit 0.7.3 already supplies modal focus containment,
Escape/backdrop dismissal, the close icon, viewport bounds and restoration of the
previous focus handle. Keep those defaults and add a labeled Close button through
`WindowExt::close_dialog`. No custom focus trap, key listener or timer is needed.
The dialog builder may run again; create the viewer entity once before opening it.
Closing drops its presentation state and leaves all Host/composer entities intact.

Fit is the initial mode, using Contain inside an absolutely positioned, bounded
image viewport. The dialog width and body height follow the current window;
intrinsic dimensions cannot enlarge the Fit body. The optional 100% mode uses
GPUI's intrinsic image layout inside a viewport with scrolling on both axes.
Returning to Fit resets the scroller identity. No custom scale math, dimension
parsing or arbitrary zoom steps are necessary for this slice.

Show the supplied label and a bounded source excerpt. **Copy source** retains the
complete local path or original data URL. For encoded tool images, it copies the
data URL built from the supplied MIME and base64, which is also the exact input to
the existing source parser. It does not claim a file, hash or newly decoded bytes.

Native fit/scroll, Escape/Close focus, draft preservation, row controls and
independent pending-question acceptance belong to isolated desktop QA. Existing
`scripts/render-fixture.py` provides local, data-URL and MCP images plus missing
paths and malformed data in histories 03, 05 and 06; run its generated histories
with `scripts/provider-fixture.py --history PATH/history.json` on a disposable Host.
Other platforms, installed providers and arbitrary zoom remain unverified.

Covered native behavior is now accepted in
[Linux fixture verification](../verification/2026-10-22-image-viewer.md), including
100% two-axis large-image scrolling, small Fit, exact Copy and focus restoration.
