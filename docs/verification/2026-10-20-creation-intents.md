# Durable local creation intents — 2026-10-20

Scope: [ADR 0018](../adr/0018-durable-local-creation-intents.md), local Unix
prototype on Linux. Source is the uncommitted foundation working tree on
`feat/session-host-foundation`, base `973b0ed883c25356a9f0849681333919d4497387`.
Host protocol remains 3 and Host journal schema remains 2; creation-store schema
is independently versioned as 1. No shared runtime or user session was restarted.

## Engine and process evidence

`cargo test --workspace --locked` passes 73 tests; the separate desktop manifest
passes 4. Five new engine tests cover persistent immutable binding, concurrent
attempt-marker exclusivity, reopen, wrong-Host refusal before marking, 32-record
pagination, private-file rejection, missing-schema rejection, mismatched receipts,
historical successful session IDs, provider errors and malformed results.

`python3 scripts/creations-smoke.py` starts private synthetic Codex and ACP Hosts
and calls the production CLI/shared engine. It verifies:

- Planning changes neither Host ownership nor mutation count. Changing group
  priority cannot rebind an existing UUID; different group reuse fails.
- A completed creation is checked using its original endpoint. A duplicate send
  is refused before a new lease claim and causes no second provider side effect.
- The provider fsyncs a synthetic effect and deliberately holds its reply. The
  controller is killed after that observed effect. A new CLI process reads the
  pending receipt, then reads `unknown` after the Host/provider restart. The
  effect count remains one; subsequent sends remain refused.
- Changing a prepared Host's admission epoch refuses dispatch without claiming,
  marking attempted, growing its journal or moving to another ready Host.
- Normal DeepSeek ACP creation recovers its session ID and also rejects repeats.

`python3 scripts/routing-smoke.py` still passes priority, drain/offline/fault,
group boundary, alias identity and no-cross-Host-replay checks.

Both manifests pass strict Clippy (`--all-targets --locked -- -D warnings`),
format checks, and their builds. Ruff checks all scripts, formatting checks the
changed fixture/smoke scripts, and `git diff --check` passes. The Unix CI workflow
now includes creation smoke; remote CI was not run in this turn.

## Native behavior

An isolated `fleetpane-create-qa` X11 workspace ran GPUI Kit 0.7.3 with software
Vulkan and three private fixture Hosts. No external model request was made.
Canonical task-owned evidence is `/tmp/fleetpane-creations-20261020/receipt.json`
with source/binary digests, logs, process identities, state readbacks and cleanup.

1. Old DeepSeek ACP retained a running permission request while drained. A ready
   group recommendation prepared a creation on New DeepSeek. Its private store
   already contained the binding with `attempted=false` before New session.
2. Editing that Host view's workspace and clicking New session displayed the
   saved-workspace mismatch error; no creation was admitted.
3. The desktop was killed before dispatch. Reopening the same configuration
   showed **Prepare saved** for the same UUID. Explicit takeover followed by New
   session created one thread on the original New DeepSeek endpoint and original
   saved workspace. A Chinese prompt completed with synthetic tool/Markdown
   activity; authenticated Host history readback contains that prompt and reply.
4. Reopening the desktop again showed **Check status** and the recovered session
   ID. Snapshots before/after prove unchanged Host/provider identities, leases,
   admission, pending requests and journal totals across the read-only check.
5. The original target Host/provider was killed and restarted at the same
   endpoint. Its creation receipt still recovered the original session ID with
   the new Host incarnation and no controller claim. This proves receipt
   recovery, not recovery of private provider state or the old ACP transcript.
6. With the target socket unavailable, the final build showed a transport error,
   retained the UUID/Host and cleared the previous success display. Restoring
   transport and checking again recovered the same ID without resending.
7. The old Host's Chinese draft survived a visit to the recovery chooser and
   return. Its permission stayed on that Host throughout target failures and
   restarts. **Skip this tool** completed the original turn with unchanged old
   Host/provider PIDs. The unused Codex fixture's journal stayed empty.

Native inspection also found and fixed stale preparation notices appearing
twice on the chooser, and the previous successful result remaining below a failed
check. Final-build unavailable/reconnected checks verify the latter fix.

Selected synthetic screenshots:

- [Prepared record after desktop kill](assets/creation-intents-prepared.png)
- [Recovered ID after target Host restart](assets/creation-intents-recovered.png)
- [Unavailable target without stale success](assets/creation-intents-offline.png)
- [Old Host approval and retained draft](assets/creation-intents-old-draft.png)

## Limits

This is local fixture acceptance, not real model, macOS/Windows, remote transport,
packaging/update or performance acceptance. The native `unknown` display was not
separately exercised; killed-controller pending and restart unknown semantics
were verified through the shared CLI/process fixture. Native pagination at 32
records was not exercised; engine paging across 35 records was.

The initial creation binding is durable. Complete session ownership routing,
provider/tool restoration, retention UI, automatic Host allocation/retirement,
direct per-Host creation persistence and draft persistence remain open. An
attempted intent with no receipt requires investigation; it is never reset or
automatically resent. No commit or push was performed.
