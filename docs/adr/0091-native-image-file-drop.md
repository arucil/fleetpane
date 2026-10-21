# Native image file drops

Paseo history 598 introduces web file drops. Fleetpane uses GPUI's typed
`ExternalPaths` on the existing Host view, with a stable element ID for its
interactive state. The Linux X11 fixture delivered selection data but the
anonymous container lost its drop callback; assigning the ID makes the real
GTK-to-GPUI drop work.

Picker and drop paths share the same bounded importer and existing immutable
Host-local image store. Decode the actual file bytes, retain ordered names and
identities, and update the draft only if the entire batch validates. Limits stay
four images, 8 MiB each, PNG/JPEG/WebP with existing dimension and duplicate
checks. Invalid batches preserve exact text, images and draft revision. Assets
validated before a later failure may remain in the bounded store.

Import is a background operation. Existing draft navigation and normal close
guards retain its lifetime; sending remains a separate explicit action. A file
drop never creates a session, sends a message or contacts the provider. No web
FileReader, MIME fallback, silent unsupported-file filtering or external URL
fetching is added. Other platform drops require separate native acceptance.
