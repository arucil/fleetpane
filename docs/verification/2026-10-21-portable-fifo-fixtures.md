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

The [next macOS run](https://github.com/arucil/fleetpane/actions/runs/39587447852/job/119179771313)
compiled successfully and reached all session tests. It exposed additional
fixture assumptions: seven socket tests exceeded Darwin's path limit under its
long per-user temporary directory; two exact path comparisons used `/var` while
stored identities use `/private/var`; and APFS rejected an invalid UTF-8 filename
with `EILSEQ` before enumeration could run.

Socket fixtures now use unique short `/tmp` directories. File fixtures use their
canonical directory, and the DeepSeek source assertion compares canonical paths.
The invalid-name fixture verifies application rejection when creation succeeds;
on macOS a failed creation must specifically be `EILSEQ`. Other filesystem errors
fail the test. None of these changes alters production file access or weakens
symlink, traversal, identity or special-file refusal.

The subsequent macOS run passed 151 of 152 session tests; the remaining assertion
was a second use of the same DeepSeek path after deleting its source. The fixture
now keeps the canonical path from creation, so both original and source-absent
readbacks compare the same saved identity. The equivalent Python DeepSeek smoke
resolves its fixture root as well. The filesystem process smoke recognizes only
Darwin `EILSEQ` at invalid-name creation; otherwise it still tests the ordinary
application rejection path. Both affected process commands pass locally on Linux.
