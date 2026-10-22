# Portable export fixture and current CLI smoke

The exact `9c8d790122ef75eded59e08dc30cfc38013a0897` [CI run](https://github.com/arucil/fleetpane/actions/runs/39756470651)
passed Windows workspace and Linux desktop jobs, but failed macOS workspace
compilation and Linux appearance migration smoke. The preceding
[`193e8e2` run](https://github.com/arucil/fleetpane/actions/runs/39747654282)
failed the same macOS compilation and the Linux model smoke.

The alert-policy schema migration updated Rust old-schema fixtures but missed
their process-smoke counterparts. The appearance smoke retained the schema-15
notification table while pretending to be schema 5; the next real migration
correctly rejected the duplicate table. Reproduced locally. The host-choice and
last-session downgrade fixtures have the same omission. Remove that new table
when constructing these older schemas, and update all five process-smoke current
schema assertions to 15. Production migration checks remain strict.

The export rejection test called `rustix::fs::mknodat`, which rustix excludes on
Apple targets. Use the existing repository's portable `mkfifo` fixture approach
and verify that it actually created a FIFO. Keep all special-file rejection,
no-output and source identity assertions. This changes test setup only.

The model smoke expected the old `require a value` fragment after the CLI prompt
parser switched to `--model requires a value`. Reproduced locally before the
change. Match the current specific error while retaining the nonzero status and
unchanged provider turn-count assertions. Production CLI behavior is unchanged.

Linux targeted checks passed:

```sh
cargo fmt --all --check
cargo test -p fleetpane-session files::export::tests --locked
cargo clippy -p fleetpane-session --all-targets --locked -- -D warnings
python3 scripts/models-smoke.py
python3 scripts/file-export-smoke.py
python3 scripts/prompt-input-smoke.py
python3 scripts/appearance-smoke.py
python3 scripts/host-choice-smoke.py
python3 scripts/last-session-smoke.py
python3 scripts/recent-directories-smoke.py
python3 scripts/paseo-import-smoke.py
git diff --check
```

[Failed exact-head excerpts and local regression logs](assets/ci-portability/failed-9c8d790.log)
retain the before/after evidence. No application runtime code or CI checks are
disabled. macOS compilation/runtime and the full remote matrix require the next
exact-head CI run; the earlier failures are not a passing result.
