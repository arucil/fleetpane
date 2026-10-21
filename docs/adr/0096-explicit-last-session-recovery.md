# Explicit last viewed session recovery

Accepted 2026-10-22. Paseo history 1385–1430 adds persistent workspace views;
Fleetpane first preserves one inspected session per exact local Host endpoint.
This is a bounded useful slice of view recovery, not workspace tabs or layout.

The configuration-local controller store records Host UUID, opaque session ID,
absolute working directory and provider archive membership. Display names are
not identity. Schema 13 adds one revisioned record per endpoint, capped at 64
endpoints and 16 KiB per record. Immediate SQLite transactions require the last
observed revision; corrupt data and failed/conflicting writes remain explicit.
Schema migration preserves earlier controller records. Older binaries refuse an
upgraded store; no downgrade is provided.

Startup loads only local preference state. It does not claim control, inspect a
provider context, resume a session or send input. The user first takes control,
then selects Open last session. A shared observational check pages the saved
workspace/archive directory, at most 800 metadata rows over 10 seconds. Exact
Host/controller identity is checked before and after the read; absent, moved,
archived/unarchived and replaced sources require an explicit fresh selection.
The desktop then uses the existing Inspect command with resume=false, validates
the returned source, and retains the saved archive membership. Opening archived
history does not restore the provider session. Context resume, Restore and Send
remain independent actions.

All storage/Host I/O runs outside rendering. Coalesced local saves retain the
latest requested selection. Normal window close waits for local writes and
surfaces a save error. Reload choice reads the competing record without changing
the selected session; Remember current explicitly writes that current source.
Keep previous choice drops this view's unsaved preference request while retaining
the stored record. Restoration state is consumed before reply validation, so a
failed attempt cannot leak archive membership into a later manual inspection.

Draft keys remain independently bound to Host/session/directory. Navigation
requires draft readiness and never copies another session's draft. No automatic
provider mutation, scroll restoration, complete layout recovery, remote support
or implicit compatibility downgrade is introduced. [Local evidence](../verification/2026-10-22-last-session.md).
