# Device appearance and persistence

Verified locally on 2026-10-21 (Asia/Shanghai), with uncommitted work on
`feat/session-host-foundation`. This is the appearance part of PRD UX-02, not
complete settings parity. See [ADR 0032](../adr/0032-device-appearance-preferences.md)
and the [evidence receipt](assets/appearance/receipt.json) for exact source and
binary hashes, logs, screenshots and private fixture identities.

## Shared behavior

`fleetpane-host appearance STORE get` and
`fleetpane-host appearance STORE set EXPECTED_REVISION system|light|dark`
use the same typed Store methods as the native window. An empty Store starts at
System/revision 0. Saving commits a new revision before changing native theme.
Stale writers fail explicitly; reload is a user action. Damaged or unavailable
storage is not replaced with default data.

The final root suite passed **118 tests**, the desktop suite **9**. Root and
desktop Clippy passed with warnings denied; both builds passed. Root Windows GNU
cross-target Clippy also passed; it excludes Unix appearance and does not verify
Windows appearance support. `cargo fmt --all --check` and the separate desktop
formatter check passed.

Commands:

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build -p fleetpane-host --locked
cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu --locked -- -D warnings
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
python3 scripts/appearance-smoke.py
```

Three new domain tests cover persistence/reopen, concurrent revision conflicts,
failed commits, schema-5 upgrade with an exact Chinese multiline draft, missing
current tables, invalid/oversize data and revision exhaustion. Migration fixtures
for older draft/pin/catalog schemas now remove the new table when reconstructing
those older versions; current damage is still refused. The shared CLI smoke
also verifies private permissions and symlink refusal without starting a Host.
Workspace pins, files, session, ACP archive and terminal process smokes passed
against the final Host build. No remote CI run is claimed.

## Native acceptance

An owned hidden X11 workspace ran the final native binary against a private ACP
fixture Host. A separate private D-Bus, installed GTK desktop portal and GSettings
keyfile backend supplied real OS appearance notifications. Config/data/cache/
runtime paths belonged to `/tmp/fleetpane-appearance-qa`; the user's desktop bus,
settings, Paseo and Codex processes were not used.

- System started dark from the private portal. Explicit Light stayed light when
  that portal changed light then dark. System followed a later light event
  without changing saved revision 2. Screenshots 01–06 capture these states.
- A CLI writer saved Dark/revision 3. The stale native System/revision 2 window
  tried Light: a conflict appeared, with its previous light/System display intact.
  Reload applied saved Dark. Screenshots 07–08.
- A SQLite trigger rejected an appearance write. The native error was visible,
  saved Dark/revision 3 remained unchanged, and the window stayed dark. Removing
  the trigger and explicitly reloading restored the normal flow. Screenshot 09.
- A brief SQLite writer lock delayed the next save. Clicking the titlebar close
  showed “Saving appearance before closing…”; releasing the lock committed
  Light/revision 4 and the app exited **0**. Reopening restored Light, the exact
  Chinese multiline draft, and the pending permission. Screenshots 10–11.
- A separate lock delayed a newly edited draft. Titlebar close showed “Saving
  drafts before closing…” and disabled appearance controls; releasing the lock
  saved the exact appended line and exited **0**. Screenshot 12 and shared CLI
  draft readback prove that the appearance guard preserves the existing guard.
- Injecting an unknown stored theme produced “Appearance unavailable” and a
  visible parsing error, with theme writes disabled. Repair plus explicit Reload
  recovered; the third native process exited **0**. Screenshot 13.

Before/after Host snapshots retain the same Host UUID, Host/provider PIDs and
exact pending permission. No prompt or permission answer was sent during theme
testing. The existing pending turn and saved draft were retained throughout.
All three native processes, the private Host/provider, portal services and the
hidden workspace were stopped after verification; their owned PIDs are absent.

## Limits

The private provider is a deterministic ACP fixture, not a new real-DSH or
OpenCodex run. Earlier real-provider evidence remains in its separate receipts.
No macOS/Windows native acceptance, packaged updater, accessibility/IME
acceptance, measured performance improvement, automatic cross-process theme
sync, forced-kill save guarantee, or remaining font/language/shortcut/layout/
notification settings is claimed. History inspection here ends at scoped
ordinal 51; indexing all 6,187 commits is a separate fact.
