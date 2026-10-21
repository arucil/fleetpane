# Offline rendering observation correction — 2026-10-21

The earlier report of incomplete offline first paint was a false positive from
consecutive image previews. Saved PNGs contain the full interface, including the
two captures described as blank in the recent-directories report. There is no
demonstrated Fleetpane rendering defect in those files. No application or toolkit
repaint change was made. The internal cause of the preview discrepancy is outside
this acceptance claim.

The [receipt](assets/render-inspection/receipt.json) retains full PNGs, a crop of
the original offline capture, OCR output, diagnostic scene counts, the final build
log and process cleanup. Direct X11 capture and the workspace screenshot both
contain the sidebar, header, connection error and composer. Reading the saved image
with OCR and viewing a crop distinguished actual pixels from the preview. OCR is
used for label presence, not exact text acceptance.

Temporary Render logging, a minimal native probe and a task-local GPUI diagnostic
copy confirmed that caret frames continued generating the complete scene (531
glyphs and 85/86 quads). All diagnostics were removed. The original three product
files and desktop lockfile were compared byte-for-byte with their pre-diagnostic
copies. The final rebuilt desktop SHA-256 is unchanged:
`1128b0d170a32824dca72242d0d2b5d077a443cdd5cec7b6a5574e5f500a4e1a`.
No Cargo registry source was edited.

With that binary, a fresh unresized 1100 × 720 window switched between two absent
Host sockets, read the second endpoint's suggestion, opened its menu and prefilled
the workspace input. Native Ctrl+A/C returned `/tmp/fleetpane-render-qa` exactly;
New session remained disabled. No provider/Host was launched or model called.
All five native windows, including diagnostics, exited normally; the hidden
workspace stopped and all eight known task process IDs are gone. The probe's
window-discovery wait expired despite its visible window; it was inspected and
closed normally rather than counted as an app launch failure.

`cargo build --manifest-path apps/desktop/Cargo.toml --locked` passed after removal
of all diagnostics. Product source and artifact are unchanged, so earlier product
test results remain applicable; no new remote CI or platform acceptance is claimed.
