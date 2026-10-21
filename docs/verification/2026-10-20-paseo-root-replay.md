# Paseo root-to-tip review verification

2026-10-20, Linux. Corrects the earlier interpretation that only changes since
the September reference should guide Fleetpane. Product source is unchanged by
this correction. Local base remains
`c91d269247207a08898e264c81083df4fbd4e745` plus the preceding uncommitted slices.
No commit/push or new remote CI run.

## History scope and actual review

Used the already captured full reference clone at upstream
`068f272a00ffef3f00ec30ec4cfa3a631be97624` and fork
`798b5bb9cabc1ebd309fd08e0cbbe8651541b21e`. No checkout of historical trees,
execution of old instructions/application code, reference source edits or
provider/runtime mutation. Captured refs, rather than dates or a moving HEAD,
define this review's scope.

The complete root index contains 6,015 upstream and 6,058 fork commits; their
union has 6,187 SHAs, with 5,886 common, 129 upstream-exclusive and 172
fork-exclusive. Both roots are `fee0f4fd285d2549ddad8d265fd4df69d1740bd0`.
Every indexed parent was checked to precede its child. This includes the root
itself, unlike the previous reference-delta mode. It does not identify 6,187
implemented or reviewed features.

Continuous inspection currently covers ordinals 1–19, through
`dabcf63ebc527289a64345119d58662b272f1065`. Each entry in the
[review ledger](../reference/paseo-replay.json) records source/config inspection
or metadata-only dependency classification, inspected paths, behavior and the
Fleetpane disposition. Dependency package payloads and all historical prompt
text were not audited. No historical application test suite was run.

The next selected batch contains ordinals 20–44, starting at
`40b199e32909412ef128e935c3b130fbaf910828`. Selecting that batch does **not** mark
it reviewed. The [root review](../reference/paseo-replay.md) separates this cursor
from later landmarks that were only located. Earliest missing core workflow:
terminal ownership/control. Terminal behavior is still unimplemented/unaccepted.

## Reader checks

Added `--from-root`, full parent IDs and one unified topological order across
both sources; root counts are labelled `commits_in_scope`, not new commits.
Optional `--after FULL_SHA --take N` returns a labelled review batch while
retaining total source/union counts. The page has `returned`, `has_more` and
`next_after`. The reader never advances the semantic-review cursor itself.

```sh
python3 -m unittest discover -s scripts/tests -p '*_test.py'
python3 scripts/paseo-history.py /path/to/paseo --from-root --limit 7000
python3 scripts/paseo-history.py /path/to/paseo --from-root --limit 7000 \
  --after dabcf63ebc527289a64345119d58662b272f1065 --take 25
uvx --offline ruff check scripts
uvx --offline ruff format --check scripts/paseo-history.py scripts/tests/paseo_history_test.py
git diff --check
```

All **13 Python tests** passed. Added tests cover inclusion of the root and its
changed files, empty parent lists, full history without available delta anchors,
shared-commit source membership, parent ordering through a merge with a deliberately
backdated committer timestamp, explicit batches/cursor advancement, empty last
page, invalid cursor/page size, mutually exclusive modes and CLI JSON root output.
The earlier dirty-index, shared-SHA, nonancestor reference, rewritten/missing ref,
shallow-clone, merge, Unicode/newline path, cap and checkpoint checks also pass.
Ruff and formatting passed. CI already invokes this test directory on Linux,
but no new remote result is claimed.

The real-clone root report was checked against `git rev-list` counts and all
parent dependencies. The first 19 ledger SHAs/ordinals and next 25 selected
SHAs were compared with that report. Existing modes still returned zero
checkpoint changes and 680 commits since the old reference (608 upstream, 551
fork). Reference HEAD, raw status digest and index bytes matched before/after
all these reads.

Canonical task receipt and raw full/first/next reports are in
`/tmp/fleetpane-paseo-replay-20261020/`. The portable
[receipt](assets/paseo-replay/receipt.json) binds source/report hashes and review
limits without checking in the full multi-megabyte historical metadata dump.

No Rust code changed in this correction, so the earlier Rust/native checks were
not rerun. This is history-analysis/tool verification, not terminal, voice,
provider parity, performance or release acceptance. No GUI or model process was
started; shared sessions were left untouched.

Subsequent work advances selected-path review to ordinal 27 and adds the partial
[PTY engine](2026-10-20-host-terminals.md). The receipt and first-19 statements
above remain the historical record of the index/cursor correction.
