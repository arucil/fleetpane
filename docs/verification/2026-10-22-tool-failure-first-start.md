# ACP failure presentation and first-launch local state

Verified locally on 2026-10-22 against base `94bc0a2`, with uncommitted changes.
The [receipt](assets/tool-failure-first-start/receipt.json) pins source/binary
hashes, before/after screenshots and regression logs. The historical review
continues through ordinal 268; this is neither tip coverage nor replacement
acceptance.

ACP `failed` was absent from the native card's error predicate. The same source
item displayed a muted title/status although Codex and MCP failures were red.
The new regression failed before the one-arm correction; all six failed,
completed, pending, in-progress and unknown/falsy cases now pass. Classification
is on the pure Row used by the actual renderer; no output-content heuristics.

An unrelated first-launch issue appeared in the same real UI: the default nested
preference path failed with ENOENT because Store created only its final leaf.
A new test reproduced the missing ancestors. Store now creates missing parents
with 0700 before its existing private, non-symlink leaf/database checks. Existing
ancestor permissions are unchanged. Tests also cover a public leaf, a symlink
leaf and a regular-file ancestor; macOS/system ancestor aliases remain supported.
No error is replaced with default preference data.

Verification commands passed:

```sh
cargo test --workspace --locked -j2
cargo clippy --workspace --all-targets --locked -j2 -- -D warnings
cargo test --manifest-path apps/desktop/Cargo.toml --locked -j2
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -j2 -- -D warnings
cargo fmt --all --check
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
python3 scripts/acp-tools-smoke.py
python3 scripts/acp-info-smoke.py
python3 scripts/acp-archive-smoke.py
python3 scripts/drafts-smoke.py
python3 scripts/appearance-smoke.py
cargo clippy --workspace --all-targets --locked --target x86_64-pc-windows-gnu -j2 -- -D warnings
python3 -m unittest discover -s scripts/tests -p '*_test.py'
```

Root unit tests: 250 (4 + 4 + 63 + 179); desktop: 28; Python: 15.
Both debug applications built. Earlier failed regression logs are retained.
The four jobs at pushed base `94bc0a2` passed remotely; that CI does not test
these uncommitted changes.

Hidden Linux workspace `fleetpane-tool-status-20261022` used one owned ACP Host
and fixture, with one `[tool-progress]` prompt. The previous desktop binary
reproduced muted failure; the new binary rendered red title/status and expandable
`false` output. Exact failed-item Copy read back 224 bytes. A 51-byte Chinese and
multiline unsent draft survived normal close and replacement, then matched native
Ctrl+A/Ctrl+C readback. The same Host/provider stayed running until teardown.

A wholly absent nested `--preferences-dir` opened with System appearance and no
error. Filesystem readback confirmed all three created directories 0700 and the
database 0600. Selecting Dark saved revision 1; a replacement window read Dark.
The 900×560 window retained the failed card and editable draft. All four desktop
normal closes exited 0; the hidden workspace and its Host were stopped afterward.

No external model, remote transport, non-Linux native behavior, full ACP diff or
performance comparison is established. Historical source inspection is limited
to the paths/sections recorded in the replay ledger; historical tests were not run.
