# Paseo history tracking and retained-card verification

2026-10-20, Linux. Base `c91d269247207a08898e264c81083df4fbd4e745` plus local
changes, including the earlier steering, workspace-pin and file-browser slices.
This slice adds a read-only history reader and reduces conversation-card
reconciliation work. File editing is still pending. No commit/push or remote CI
run was performed for this slice.

## Reference capture

Fetched only `upstream main` and `origin main` with `--no-tags` in the reference
clone; no checkout/reset or dirty-source edit. Captured upstream
`068f272a00ffef3f00ec30ec4cfa3a631be97624` and Donut fork
`798b5bb9cabc1ebd309fd08e0cbbe8651541b21e`. The
[history guide](../reference/paseo-history.md) links the selected implementation
and regression-test changes actually reviewed, and lists their Fleetpane
acceptance implications. It does not claim exhaustive review of every new commit
or runtime acceptance of Paseo's implementation.

Against original fork reference `a486b0558168c08890890e787d55cfb88c0d7256`, the
reader returned 608 upstream and 551 fork commits, 680 distinct SHAs, 16 merges
and 72 fork-exclusive commits. Against the two captured checkpoints it returned
zero new commits. Source HEAD, raw status digest and index byte digest matched
before/after both reader runs. The reader does not include dirty file contents.
Raw reports and the canonical task receipt are in
`/tmp/fleetpane-upstream-20261020/`; the portable selected
[receipt](assets/paseo-history/receipt.json) binds source and artifact hashes.

## Automated checks

```sh
python3 -m unittest discover -s scripts/tests -p '*_test.py'
python3 scripts/paseo-history.py /path/to/paseo --from-reference
python3 scripts/paseo-history.py /path/to/paseo
python3 scripts/history-smoke.py
cargo test --workspace --locked
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
cargo fmt --all --check
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
uvx --offline ruff check scripts
uvx --offline ruff format --check scripts/paseo-history.py scripts/tests/paseo_history_test.py
git diff --check
```

Results: 99 Rust workspace tests, 7 desktop tests and 9 Python history-reader
tests passed. The 5,211,650-byte / 1,200-command process history smoke passed
bounded recent/older items, turn boundaries/end and metadata-only resume.
Strict Clippy, desktop build, formatting and Ruff passed.

Reader tests use local Git DAGs: shared-SHA deduplication, separate source
checkpoints, dirty-source/index preservation, Unicode/newline paths, empty delta,
missing/ref-rewritten history, explicit commit cap, merge metadata, root commit's
empty parents field, nonancestor original references, shallow-clone refusal,
CLI JSON output and malformed/duplicate checkpoint rejection. No network in CI;
the Python test step is added to Linux CI, but has only run locally so far.

Card tests keep 1,200 cards plus one changing tail through 200 updates and select
exactly one card update per tick; an unchanged snapshot selects none. They cover
insert/reorder/remove, label/content/copy/error changes, thread-qualified IDs,
empty history and session replacement after clearing previous rows. This checks
the reconciliation operation set, not frame time, retained memory or real-model
throughput. Full row construction remains per update.

## Native acceptance

Used the `agent-workspace-linux` skill and a hidden `fleetpane-history-qa`
workspace with software Vulkan. Launched a new native desktop and one new
private Host/provider fixture with two synthetic paginated sessions. No existing
Paseo, OpenCodex or DeepSeek process was acquired, restarted or stopped.

- Opened `row-reuse`, expanded the completed command, then sent a Chinese
  follow-up through the native composer. The reply completed and rendered as
  `Echo: Retained card update · 中文 exact`.
- The previous command remained expanded after that update, including
  `OLD-CARD-KEEP · 中文` and its exact trailing spaces/newlines. Its JSON clipboard
  payload parsed to the expected exact `aggregatedOutput`.
- Switched to `other-session`: its command clipboard contained only the distinct
  `OTHER-SESSION-ONLY` output. Switched back, loaded the older turn and confirmed
  the original command clipboard was byte-for-byte identical.
- Reloaded the conversation, loaded its older turn and checked the original
  command again. Recent/older pagination and Markdown remained visible.
- Closed the desktop normally, then stopped only the task-owned Host/provider
  and hidden workspace. Receipt records resource identities and cleanup.

See [retained card after reply](assets/paseo-history/retained.png),
[new completed reply](assets/paseo-history/new-reply.png),
[separate session](assets/paseo-history/other-session.png) and
[reloaded older card](assets/paseo-history/reloaded.png).

No W0–W4 release comparison, UI p95, long soak, real inference or multi-Host
native performance acceptance was run. These checks do not establish overall
responsiveness under the user's full workload. The history tracker runs when
invoked; no automatic fetch/schedule or unattended implementation is installed.
