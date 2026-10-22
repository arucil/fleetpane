# Workspace alert policy acceptance

2026-10-22, Linux x86_64, GPUI Kit 0.7.3, isolated X11 and Mesa software rendering,
debug builds. Source base `df39a36b7c3acc03f228d40c2333be03604eb702` plus this
change. [Decision](../adr/0113-workspace-alert-scopes.md),
[fixture readback](assets/workspace-alert-policy/final-readback.json),
[real-provider receipt](assets/workspace-alert-policy/real-provider-receipt.json),
[artifact hashes and build provenance](assets/workspace-alert-policy/manifest.json).

## Covered behavior

- Device-local workspace scopes match an exact Host endpoint and observed
  directory. A same-path session on another Host, child directory, neighboring
  prefix and unknown directory are not muted by that scope. All six existing
  event types share the rule. Global, event, Host and exact-session rules retain
  their prior semantics. Muting does not answer a request or review a result.
- The Host passively caches at most 64 session-directory records within 128 KiB.
  Tests cover event-ring expiry, record/byte eviction, replacement incarnation,
  metadata validation, old snapshots without support and untrusted request cwd.
  Unknown/evicted associations remain included; the native row identifies this
  condition when workspace rules exist on its endpoint. No native legacy-Host
  or eviction acceptance is claimed; those boundaries have shared tests.
- Old schema-15 policies still read with no workspace scopes. Shared store and
  CLI probes cover persistence, reopening, CAS conflicts, invalid paths,
  duplicates, capacity and rejection before storage creation. No directory is
  created or inspected by policy matching.

## Native synthetic workflow

Three task-owned fixture Hosts expose four workspace tabs: Codex alpha/beta
and two ACP Hosts sharing alpha's Chinese path. Four original pending requests
are a two-page question, Codex approval and two ACP permissions. Two independent
completed/failed records make the original alert count six.

Muting Codex alpha gives five alerts. Creating a new alpha approval and a new
alpha session with completed/failed turns after the save retains five alerts:
future sessions inherit the rule. Muting the separate results directory then
gives three alerts while all **five pending requests and four unreviewed results**
remain present. Removing the two workspace rules in Alerts restores counts
five and nine. Later beta's header toggle changes three to two, and unmute
restores three. See [retained pending queue](assets/workspace-alert-policy/pending-queue.png)
and [workspace toggle](assets/workspace-alert-policy/workspace-toggle.png).

The final label build repeats the owned SQLite trigger refusal of a workspace unmute. Revision **9**, both saved
scopes and all three complete Host snapshots remain identical. Alt+F4 refuses
normal closure and shows the actual failure. Removing the trigger and using
Reload alerts clears save/close errors without changing the policy.
[Final-build failure](assets/workspace-alert-policy/failed-save.png),
[recovery](assets/workspace-alert-policy/recovery.png).

Native QA reproduced an in-flow settings panel pushing the composer/actions
outside a **900×560** window. Settings now use Kit's bounded scrolling popover.
The [final small window](assets/workspace-alert-policy/small-popover.png) retains
the composer and approval/Stop buttons. Escape restores the session; Next request
and tab shortcuts do not navigate behind the open popover. A 17-scope scroll
probe reaches lower entries without resizing the session. The full directory
and endpoint are available in each toggle's tooltip. Explicit Next request still
selects the muted question source without answering it.

Normal closure/replacement retains saved scopes and all four exact draft records:
each is **48 UTF-8 bytes** with Chinese, a newline and two trailing spaces. The
final build's native beta clipboard is exactly
`workspace mute 1 中文\nretain trailing spaces  ` (a real newline).
All original pending payloads, terminal results, Host/provider identities,
mutation IDs and journal totals remain. Explicit QA takeover and normal
conversation metadata reads account for owner/sequence/cache-order changes;
the policy-only fault probe compares complete snapshots exactly. No decision,
Stop or user draft Send is issued.

## Installed-provider acceptance

Task-owned OpenCodex and DeepSeek Harness instances each complete a no-tool turn
in the same exact Chinese directory. The replies contain the requested synthetic
tokens; `wait` returns the same completed turn and Host/provider identity is
unchanged. Both immediate creation snapshots contain the exact observed thread
directory. Native read-only result inspection keeps two results unreviewed.

On the final desktop binary, muting only OpenCodex's workspace yields
[one alert](assets/workspace-alert-policy/real-one-muted.png); adding DeepSeek's
scope yields [zero](assets/workspace-alert-policy/real-both-muted.png); removing
them restores [two](assets/workspace-alert-policy/real-unmuted.png). Both
full Host snapshots remain exactly unchanged through these policy changes and
window closure. This proves real directory-metadata/result compatibility; actual
questions/permissions for this slice use the separate synthetic workflow above.
The earlier real approval/question evidence remains independent.

The first exploratory real harness read `wait.status` instead of its documented
`outcome`, after OpenCodex had already returned the expected token. The corrected
harness used fresh private instances; this was a probe error, not a product fix
or provider retry. Final real acceptance uses new task-owned instances and the
final binary. Copied private DSH credentials are removed; no provider-home files
or credentials enter the public artifacts. All owned desktops, Hosts, providers
and sockets are stopped; shared Paseo and other sessions are untouched.

## Checks and limits

**345 workspace tests and 44 desktop tests pass**, along with strict Clippy,
formatting and builds. Process checks cover raw attention directory projection,
terminal-result cwd, policy persistence, runtime/controller replacement, read
queries and drain admission. Compressed logs accompany the artifact manifest.

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build -p fleetpane-host --locked
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
python3 scripts/notification-policy-smoke.py
python3 scripts/attention-smoke.py --hold-seconds 1
python3 scripts/completion-smoke.py
python3 scripts/runtime-smoke.py
python3 scripts/query-smoke.py
python3 scripts/drain-smoke.py
git diff --check
```

Final Host SHA-256:
`620c426ef2b1e3fcd1fe3611d4988e03a81a4a40cd8da5ada8ef44a28e89f36d`.
Final desktop SHA-256:
`581b101b597ba60e7c627f17ca5fb177e7d94c0ade8f15e45650b54609513a98`.
The pending-queue/toggle/scroll exploration used the immediately preceding
desktop `e54b1edc…`; only directory-first labels/tooltips changed afterward.
Final small-window, fault/reload, exact clipboard and installed-provider counts
were repeated on `581b101b…`; screenshot provenance is explicit in the manifest.

The previous exact-head [tab-drag CI](https://github.com/arucil/fleetpane/actions/runs/39761641148)
passes four jobs. This new change requires its own exact-head CI result.
OS delivery/sound/tray/dock, delivery deduplication, cross-device settings, real
IME, macOS/Windows native GUI and release/performance acceptance remain open.
This is not complete ATT-04/05 or replacement-release readiness.
