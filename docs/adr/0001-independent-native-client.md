# 0001: Start with an independent native client

Status: accepted for the initial foundation, 2026-10-18.

## Context

The replacement targets responsiveness with many coding-agent sessions. Rust
alone does not establish a performance improvement: rendering, subscriptions,
serialization, history loading, memory retention and daemon work all matter.

Replacing the presentation and provider lifecycle simultaneously would make
compatibility failures and performance changes harder to attribute. Existing
Paseo sessions are valuable reference behavior and should remain available while
the replacement is developed.

## Decision

Start in an independent repository with a narrow, read-only protocol adapter.
Keep the wire projection, transport and presentation in separate packages.
The first runnable slice lists a bounded page of active session metadata without
subscribing to timelines. A CLI makes this slice testable before a desktop shell.

The planned desktop toolkit is GPUI. Validate its current API and Linux text,
input, clipboard, accessibility and terminal requirements before treating that
choice as settled for a complete replacement. No desktop toolkit is needed by the
initial protocol or client crates.

Adopt ArtCraft's publicly documented thin-UI and behavior-verification ideas,
using a workflow matrix and reproducible performance evidence. Do not copy its
proprietary-product-specific restrictions or assume access to private standards.

## Consequences

- An existing compatible daemon is required for live reads.
- Initial progress improves observability and protocol isolation; it does not
  remove the current daemon's memory or latency costs.
- Native daemon work remains a later, independently evaluated migration. Related
  Rust implementations can be assessed module by module with license provenance.
- Compare release builds on identical workloads before claiming better speed or
  lower total memory. Include every process still required by the application.
