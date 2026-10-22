# Provider context observations

2026-10-22, Linux X11 hidden workspace `fp-model-search`, debug Rust builds,
GPUI Kit 0.7.3. Synthetic Codex/ACP providers; no model calls or live sessions.
[ADR 0103](../adr/0103-provider-context-observations.md) defines exact semantics.

Core/source checks passed: `cargo fmt --all --check`,
`cargo test --workspace --locked -j2` (311 tests), and
`cargo clippy --workspace --all-targets --locked -j2 -- -D warnings`.
Desktop formatting, test (39), strict clippy and build passed using the separate
`apps/desktop/Cargo.toml` manifest and `--locked -j2`. Owned source hashes and
original logs are retained in [the receipt](assets/context-usage/verification-receipt.json).
No Rust file changed after those checks. The fixture's `thread/list` initially
returned turns; native QA reproduced its violation of metadata-only paging.
The fixture now strips turns, and the corrected real-Host process smoke passed:
`python3 scripts/context-usage-smoke.py --evidence-dir /tmp/fp-context-usage/process-fixture-fixed`.
The [process receipt](assets/context-usage/process-receipt.json) includes cleanup.

| Workflow | Native evidence and result |
| --- | --- |
| Codex live context | [125/100 tokens and 125%](assets/context-usage/codex-observed.png), using last usage rather than cumulative 999999 |
| Small window | [900×560 composer/Send/Stop](assets/context-usage/codex-small.png), Chinese multiline draft remains; pending choice can be scrolled into reach |
| Zero capacity | [0/0, no percentage](assets/context-usage/codex-zero-settled.png) |
| Unknown capacity | [7/unavailable](assets/context-usage/codex-unknown.png), no percentage or invented default |
| Other thread | [Explicit unavailable](assets/context-usage/codex-unavailable.png), no leakage from observed thread |
| ACP live context | [42/100 and 42%](assets/context-usage/acp-observed.png), opaque cost and metadata retained |
| ACP question controls | [Allow/Reject and retained draft at 900×560](assets/context-usage/acp-choice-actions.png) |
| Exact Copy | Both copied JSON objects equal their complete expected envelopes, including thread, receipt time and untouched raw provider telemetry; JSON formatting/key order are not wire-byte claims |
| Independent activity | [Snapshot comparison](assets/context-usage/nonmutation-receipt.json): host/provider identities, active turns, pending requests, operation IDs and journal counts unchanged |

Installed Codex schema generation matches the selected fields; this is schema
evidence, not installed-provider context runtime acceptance. ACP restoration is
covered by archive and process tests. Codex observations remain memory-only and
are unavailable after Host restart until new telemetry. Fixture Stop settlement,
IME, other platforms and installed-provider runtime remain unaccepted.

Installed OpenCodex and DSH runtime observations and exact native JSON Copy now
have [separate two-turn acceptance](2026-10-22-real-provider-outline-usage.md).
The synthetic edge cases above and real-provider scope remain distinct.
