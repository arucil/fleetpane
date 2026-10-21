# Local image attachments

Paseo root replay 114 (`a3224175ee96cff50bf31ecde71c30df0e0f2e79`) adds
image selection, thumbnails and ACP prompt content. Its UI enables image-only
Send but still rejects empty text inside the handler; it clears the selection
before conversion, and failed conversions are logged and omitted. Fleetpane
needs image-only messages and durable failure recovery alongside text drafts.

## Decision

Keep original PNG, JPEG or WebP bytes in a private local asset directory named
by appending `.attachments` to the complete Host socket path. Replacing a file
extension would alias endpoints such as `host.sock` and `host.other`; a regression
test requires distinct asset roots. Assets use SHA-256 filenames and their actual
format, independently of the selected filename's extension. The descriptor records
hash, MIME, byte count, decoded dimensions and original name; it contains no path
supplied by a controller.

Import rejects symlinks, non-regular files, changed reads, invalid/undecodable data,
unsupported formats and excessive sizes. Each image is at most 8 MiB, 8192 pixels
per dimension and 16 Mi pixels. A draft accepts four distinct images. A private
store lock, 256 MiB / 4096-file quota, private temporary file, fsync and atomic
non-replacing hard-link publication preserve prior assets on failure. Publication
sets files to 0400; this remains a trusted same-user filesystem, not protection
against an adversarial local account changing its own files. No automatic asset
deletion or garbage collector is introduced.

Text and image descriptors form one revisioned draft. Empty image lists omit the
new JSON field, preserving exact serialized CAS comparisons for earlier text-only
rows. Older binaries reject nonempty image drafts via their existing unknown-field
rule. Send binds the exact text, images and operation UUID before dispatch. A
confirmed receipt clears both; rejection retains both, and pending/unknown outcomes
retain the bound attempt without resending. Text-only edits preserve images. Live
Codex follow-ups refuse attached images rather than dropping them.

The native picker imports on a worker and commits the entire selection together.
An import failure leaves the current selection and text intact. Preview and Remove
operate on that revisioned content. Selecting files and saving selected bytes are
separate states: navigation/send wait for either, while close waits for durable
imports/saves. Picker errors are visible and do not disable ordinary text saving.

Control requests and operation receipts contain only descriptors. On a connection
task, the Host opens and revalidates saved bytes before admission. A previously
recorded UUID still returns its original receipt if an asset is later missing.
Two import/load workers and two prepared-image sends bound queued image work;
decoding does not run on the UI or serial Host state loop.

Codex receives `localImage` paths. ACP receives image content only when its v1
initialize result advertises `agentCapabilities.promptCapabilities.image: true`.
Omitted, false and malformed capabilities do not enable it. The adapter records
small local image references in observed history and expands base64 only on the
outgoing provider pipe. That pipe has a separate limit for four 8 MiB images;
existing control, incoming provider, event, receipt and history limits stay intact.
`fleetpaneAttachmentVersion: 1` advertises this additive local Host facility.

The ACP contract follows the official
[v1 schema](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/schema/v1/schema.json)
and [prompt-turn documentation](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/protocol/v1/prompt-turn.mdx).
The installed Codex generated `UserInput` schema and actual app-server/loopback
transport verify its `localImage` contract. ACP v2 capability objects are not used.

## Acceptance and limits

[Verification](../verification/2026-10-21-image-attachments.md) distinguishes native
fixture behavior, actual Codex transport to a local model stub, and real DSH
capability negotiation. Installed DSH 0.2.0-rc.2 explicitly reports image support
as false; it remains disabled. No real external model's visual understanding is
claimed.

This implements local file selection/send. Remote upload, clipboard image paste,
drag/drop, full-size media inspection, additional image formats, attachment-aware
live steering, asset GC and cross-device routing remain open. An ACP provider
echoing a large base64 image in an incoming update can still exceed the existing
256 KiB provider frame bound; that compatibility is not accepted by this slice.
Large-image memory/latency under concurrent real agents is unmeasured. Small
windows keep Send reachable but still have no reserved minimum timeline height.
This does not implement automatic Host retirement or runtime migration.
