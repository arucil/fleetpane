---
name: rust-native-rewrite
description: Plan and implement a native Rust desktop rewrite with observable behavior parity, a testable engine, and reproducible performance comparisons. Use for replacing an existing desktop application or migrating its runtime, rather than routine Rust edits.
---

# Rust native rewrite

This is a locally authored workflow informed by ArtCraft's public PhotoCraft,
WordCraft and FigCraft engineering documents. It is not an official ArtCraft skill
and does not contain their private `craftrules` standards. Read
[sources and adaptations](references/artcraft-sources.md) when checking provenance
or deciding whether an upstream convention applies.

## Establish the replacement contract

Inspect the incumbent's current behavior, architecture and bottlenecks before
choosing the migration seam. Pin the source revision and record the reference
configuration. Separate observed results from suspected causes.

Build a behavior matrix around workflows people actually use. Each row needs a
trigger, observable result, fixture or reproduction, verification evidence, and
status: missing, partial, implemented, or verified. Keep the presence of a UI
control separate from correctness of its behavior. Use a small prioritized matrix;
do not invent an overall readiness percentage from item counts.

Prefer an independently verifiable vertical slice before expanding coverage.
Choose desktop-first, engine-first, or an incremental adapter from evidence and
the user's chosen scope; this skill does not decide that architecture in advance.

## Separate the engine from its presentation

Keep protocol/domain types and state transitions independent of the UI toolkit.
Use explicit crate dependency boundaries that fit the application; ArtCraft's
exact layer count is not required. Add an executable dependency check when the
workspace becomes large enough that review alone is insufficient.

Route domain actions through the same typed command or use-case implementation
from GUI, CLI and any automation surface. Selection, scrolling and window layout
can remain presentation state. Provide inspectable results and errors. Build only
the automation surface currently needed; a command engine does not require
shipping an MCP server immediately.

Keep rendering free of filesystem scans, network waits, full-history parsing and
unbounded background queues. For large collections, evaluate virtualization,
incremental updates, bounded caches, pagination and demand-driven subscriptions
against the measured workload. Make omissions or truncation visible.

## Verify behavior through the same entry points

Use synthetic fixtures, pinned public corpora, protocol recordings with sensitive
data removed, or deterministic event replays as appropriate. Test the observable
contract, including failure behavior, cancellation and reconnect semantics when
those are part of the current slice. Do not silently treat a missing external
fixture or unavailable integration as a passing check.

For compatibility work, use round trips or differential comparisons where the
reference behavior is a trustworthy oracle. Preserve unknown data when required
by the format/protocol contract, and reject unsupported operations explicitly.
Input and I/O errors should carry useful context; do not hide an internal bug
behind defaults, blanket panic recovery or retries.

Inspect rendered UI output as well as tests. Use offscreen rendering or an
isolated desktop where available, without taking over the user's active window.
Keyboard input, IME, clipboard, text selection and terminal behavior need their
own acceptance evidence when the replacement depends on them.

## Measure the actual replacement workload

Compare release builds under the same machine, workload, data volume and warm/cold
conditions. Record process boundaries: a faster worker does not establish a
faster whole application. Capture input/action latency distributions, idle and
loaded memory, startup/recovery, and the hot operations relevant to the user.

For each scenario record the command, revisions, build profile, environment,
workload size, raw result location, baseline, candidate, and measurement limits.
Set budgets from user constraints or measured needs; label proposed targets and
unmeasured scenarios. Do not claim gains from development-versus-release builds,
small synthetic inputs, or feature counts alone.

## Keep progress reviewable

Run targeted tests and the relevant formatter/compiler/lint checks. Rerun affected
checks after changing the implementation. Keep architecture decisions in an ADR,
task progress in the issue/PR or project status document, and stable routing rules
in AGENTS.md. Update the behavior matrix and known gaps with the code change.

When working in parallel, use clear file ownership and appropriate build resource
limits; choose separate target directories only when lock contention warrants the
extra build and disk cost. Follow the user's commit/publish authorization rather
than copying an upstream project's automatic commit policy.

Report what works, its evidence, what remains partial, and which runtime claims
are still unverified. The skill itself grants no permission to publish a repo,
restart an existing runtime, migrate live sessions, or change an account.
