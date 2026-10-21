# Metadata and timeline invalidation — 2026-10-21

The uncommitted tree on base `c91d269` separates metadata/directory refresh from
selected conversation row construction. This removes a concrete redundant code
path found while reviewing Paseo 93–101. [ADR0040](../adr/0040-separate-timeline-and-metadata-invalidation.md)
records the decision; the [receipt](assets/timeline-invalidation/receipt.json)
binds the tested sources, binary, logs, native captures and cleanup.

## Reproduction and tests

The old poll changed flag covered directory updates, selected metadata and row
content alike. It always called rebuild_rows, which constructed every loaded
row before retained-card comparison. The retained pre-fix regression applies a
real thread name update, verifies identical Row inputs, and fails because that
metadata still invalidates the timeline. The final test covers all six excluded
metadata methods, actual text deltas, foreign events and conservative handling of
newly supported content methods. Projection errors still rebuild and pause replay.

All 11 desktop tests pass, including existing retained-entity, UTF-8, diff and
terminal tests. Desktop Clippy with warnings denied, locked desktop build,
formatting and diff whitespace checks pass. All 13 history-tool tests pass.
Existing ACP mode and permission-review process smokes pass with the unchanged
Host binary. Root/Windows checks from the preceding mode slice were not rerun:
this change affects only the Unix desktop. Remote CI and real model calls were
not run.

```sh
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
python3 -m unittest discover -s scripts/tests -p '*_test.py' -v
python3 scripts/acp-modes-smoke.py
python3 scripts/acp-review-smoke.py
```

## Native acceptance

One private Host/provider and desktop ran in a hidden 1680×1100 workspace.
The synthetic selected session contains 24 tools with 60,000-character text
results. The initial full-history CLI read hit its existing frame-size boundary;
the session remained healthy and was inspected with bounded pages. Native Load
earlier messages loaded all 26 items, as shown in page-8.png.

The private agent emitted 100 selected title updates, 100 selected mode updates
and 100 background title updates. Native title labels reach 099, Mode reaches
Code, and the history marker remains visible with all 26 items loaded. SQLite
turn/item record hashes are identical before/after the 300 updates; Host ID and
provider PID stay identical with no fault. Native clipboard readback preserves
`Metadata stress draft 中文\n  keep all spaces` exactly. This is behavior and
invalidation-path acceptance, not a wall-clock speed benchmark.

Native Send then starts one permission-review prompt. An initial immediate click
after paste produced no provider prompt. After the saved draft and enabled
button were visible, the native click started exactly one review prompt.
The first request shows Review settings. Native Skip this tool replaces it with
the standalone Review plan request; Native Run this tool replaces that with the
boolean-input request. Turn/item archive hashes are unchanged across both swaps.
The preview therefore refreshes when only the pending request changed. Native
Stop then cancels the final request and settles the turn as interrupted; content
and turn updates still reach the timeline. Provider answer readbacks and native
captures are retained.

## Limits

Content-event polls, history pages and explicit reloads still construct loaded
rows. No incremental item projector, history eviction, latency/CPU/memory budget,
full concurrency acceptance, voice behavior, remote CI or full parity claim.
Only task-owned processes were stopped; no user session was touched. Changes
remain uncommitted and unpushed.

The desktop closed normally with exit 0. All three task Host/provider/desktop
PIDs are gone and the hidden workspace is stopped; cleanup is in the receipt.
