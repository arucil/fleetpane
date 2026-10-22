# Linux system popup acceptance

2026-10-22. Base `6db5c5b` plus the changes introducing ADR 0116. That base's
[four CI jobs](https://github.com/arucil/fleetpane/actions/runs/39776846931)
passed; new-commit CI is a separate result. This is Linux X11 acceptance with
owned synthetic Codex/ACP processes, not new installed-provider/model acceptance.

## Shared policy and transport

The opt-in setting round-trips through the same private store/CAS API. Old JSON
defaults false and retains its original serialized shape; invalid non-boolean
values fail. System enablement never overrides any mute rule. Existing pending
and completion claim process probes still pass concurrent deduplication,
consumption while muted, rollback and unchanged complete Host snapshots.

Five private `dbus-daemon` transport regressions exercise actual zbus messages:

- A same-category replacement reuses the returned ID, retains count text and
  sends silence/transient hints and a bounded expiry. Old-generation actions
  are rejected; the current action carries the right category/generation.
- A server without `actions` still accepts replace/close, with an empty action
  array. No unsupported activation is assumed.
- A refused replacement returns its actual error and closes the previous ID.
- Replacing the well-known service owner stops delivery and cleans only the
  original unique owner; IDs are never sent to the new server.

Worker shutdown closes live owned IDs. No test silently skips a missing private
bus dependency; Linux desktop CI explicitly installs `dbus-daemon`.

## Native desktop and real notification server

An isolated Agent Workspace desktop (`:91`) runs a private D-Bus daemon and
Dunst **1.13.2** extracted under the task directory. No package is installed
globally and no notifications go to the user's session bus. Fleetpane connects
to two task-owned Host/provider fixtures plus one intentionally unavailable
endpoint. Existing pending/result feeds baseline silently.

The native Alerts button persists **System popups · on**. New questions and
permissions produce a count-only Dunst notification and the existing Kit toast.
[Before-signal-repair pending popup](assets/system-popups/final-pending-popup.png).
A real Dunst action opens all-Host Pending requests; result actions open Turn
results, without reviewing or answering.
[Pending navigation](assets/system-popups/pending-click.png),
[before-signal-repair result navigation](assets/system-popups/final-completion-click.png).
Snapshots remain unchanged over the observational popup/click intervals. Explicit
fixture CLI sends/takeovers used to generate new work are outside those intervals.

The private monitor records `Notify` and the actual `CloseNotification` when
global mute is saved. Further requests while muted produce no OS Notify; unmute
does not replay them. Disabling system popups also closes the existing popup;
reenabling does not replay events consumed while disabled. The independent queue
continues to grow and remains actionable. At **900×560** Alerts wraps/scrolls
without losing the conversation composer.
[Final small muted view](assets/system-popups/final-small-muted.png),
[mute readback](assets/system-popups/final-mute.json),
[disable readback](assets/system-popups/disabled-receipt.json).

Stopping the private Dunst service gives a visible Alerts error while the real
queue and draft survive. Starting a replacement and clicking **Reconnect popups**
does not replay old items; a later occurrence shows a new notification.
[Before-signal-repair error](assets/system-popups/final-service-error.png),
[recovery](assets/system-popups/final-service-recovered.png),
[readback](assets/system-popups/final-reconnect.json).

Normal Alt+F4 with a live popup closes the real server ID before Fleetpane exits.
Replacement preserves the original cursor and produces no Notify for old work.
[Close readback](assets/system-popups/final-close.json),
[replacement](assets/system-popups/final-no-replay.png).
Both desktop artifacts' SHA-256 values is in the [receipt](assets/system-popups/receipt.json).

A **39-byte UTF-8 draft**, Chinese, one actual newline and two trailing spaces,
is retained across notification navigation and normal window replacement. Native
Select all/Copy matches all bytes before and after replacement, with identical
saved key/revision/data; it is never sent.
[Replacement draft](assets/system-popups/final-draft-reopened.png),
[readback](assets/system-popups/draft-receipt.json).
An instrumentation-created last-session CAS conflict was explicitly reloaded,
and synthetic CLI sends revoked the GUI controller as designed; neither was
hidden with product retries. Early screenshots missed the short-lived popup and
the first capture helper required an unavailable `scrot`; accepted captures use
the workspace screenshot tool. Earlier binary captures are distinguished from
captures before the signal-order repair in the manifest.

## Final signal-order repair and native recheck

Review found that separate ActionInvoked/NotificationClosed streams could let
`select!` process close before an earlier click. A private-bus regression queues
both signals during a replacement call: it [fails before the repair](assets/system-popups/order-before.log.gz).
Use one ordered stream from the pinned notification owner. The new regression
runs 16 independent scenarios and the full 49-test desktop suite passes.

Rebuild and strict Clippy pass after the repair. A fresh private desktop (`:90`),
bus, Dunst and fixtures verify pending/result clicks with unchanged snapshots,
actual mute retraction, muted consumption/unmute without replay and 900×560.
[Final pending navigation](assets/system-popups/ordered-pending-click.png),
[result navigation](assets/system-popups/ordered-result-click.png),
[small view](assets/system-popups/ordered-small.png),
[mute](assets/system-popups/ordered-mute.json).
Normal close sends CloseNotification and the application registry reports exit
code 0. The probe waiting for `/proc` disappearance timed out on process reaping;
that instrumentation failure is retained [separately](assets/system-popups/ordered-close.json).
Final binary/hash and earlier-artifact draft/replacement coverage are separated
in the receipt and manifest; no second draft or installed-model run is inferred.
The final ordered transport has fresh native acceptance; the unchanged draft
path retains the earlier 39-byte acceptance. All owned runtimes are cleaned.

## Checks and limits

`cargo test --workspace --locked`: **362 pass**.
`cargo test --manifest-path apps/desktop/Cargo.toml --locked`: **49 pass**.
Both strict all-target Clippy checks, both fmt checks, `git diff --check`, Host
and desktop builds pass. `cargo check --workspace --target
x86_64-pc-windows-gnu --locked` passes; this is an engine check, not Windows GUI
acceptance. Python script tests: **22 pass**. Policy, pending and completion
claim process probes pass. [Check logs](assets/system-popups/checks.txt).

This does not establish sound behavior on other servers, physical audio output,
tray/dock support, native Wayland activation, macOS/Windows GUI behavior, a new
installed OpenCodex/DeepSeek model run, or release performance. Claims remain
at most once and can be lost after commit. Another window's changed policy is
observed asynchronously. A server may ignore silence/history/expiry hints or
refuse close; failed transport is reported while the window remains open. All
task-owned GUI/Host/provider/bus processes are cleaned after verification.
