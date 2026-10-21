# Portable FIFO rejection fixtures — 2026-10-21

The [macOS job](https://github.com/arucil/fleetpane/actions/runs/39585727859/job/119174009452)
for `c5d10f4af7418f7372f3eda195a51ff6aee12b94` failed during test compilation.
Four fixtures called `rustix::fs::mkfifoat` or `mknodat`, which rustix 1.1.5
explicitly excludes on Apple targets. Linux, Windows and desktop Linux jobs
passed for that revision; this was not a runtime rejection failure.

The fixtures now create their private named pipes with the POSIX `mkfifo -m 600`
utility and assert its successful exit. Existing rejection assertions still run
for attachment import, workspace file reads, worktree locks and migration writer
locks. No production I/O behavior, warning policy or test platform is disabled.
These Unix tests require `mkfifo` on PATH, as other fixtures already require Git
and Python. Arguments are passed directly without shell interpolation.

Local Linux validation:

```
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

All 223 Rust tests pass, including the four rejection paths. The original macOS
compile log establishes the regression; local Linux success does not establish
macOS acceptance. The pushed candidate requires a fresh hosted CI run.
