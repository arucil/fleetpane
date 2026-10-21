# Clipboard images join the revisioned draft

PRD AG-04 and UX-03 require inspectable image attachments and image paste within
provider capabilities. The existing composer only connected the native file
picker. GPUI Kit 0.7.3 already exposes `Textarea::on_paste`, and its pinned Linux
X11 backend returns encoded `ClipboardEntry::Image` payloads. No toolkit patch is
needed. Paseo replay 164 concerns picker permission/lifecycle recovery; it does
not implement this clipboard feature.

## Decision

Use the Kit hook only at the message composer. Plain text and other non-image
payloads fall through to the input's existing behavior. An image payload is
consumed as attachments, without inserting accompanying clipboard text or sending
a message. Accept PNG/JPEG/WebP, up to four images and 8 MiB each, under the
existing provider capability and draft-operation gates. Validate batch count,
declared format and byte bounds before copying image buffers.

Add `attachments::Store::import_bytes` alongside file import. Both converge on
the same encoded-data inspection, content hash, private bounded store, lock and
durable non-replacing publication. Actual decoded bytes determine MIME and
dimensions; a clipboard label is not a trusted identity. Decode and filesystem
work stay on the existing bounded worker path. No temporary input file or shell
clipboard process is part of the application.

Commit the complete successful batch through the existing revisioned text/image
draft. Import blocks navigation/send and closing waits for completion. Existing
text may still be edited while importing; completion uses the latest composer
text. Rejection keeps the prior selection and text. Successfully published assets
from a later-rejected batch may remain in the already bounded store, as with the
picker; this does not introduce automatic asset deletion.

Preview, Remove, save/reopen, pre-admission validation and receipt-bound send all
reuse the current attachment pipeline. The clipboard action grants no automatic
send or retry. Copied file paths, drag/drop, remote uploads, full media viewing,
clipboard conversion and additional platform acceptance remain separate work.

See [Linux native and provider evidence](../verification/2026-10-21-clipboard-images.md).
