# Paseo history as an implementation guide

The user clarified that the implementation guide must start at Paseo's first
commit. Use the [root-to-tip review](paseo-replay.md) and its explicit review
cursor as the primary workflow. This page is the earlier, limited analysis of
changes since the September reference; its recent-first priorities do not define
the order of the chronological replay.

Captured 2026-10-20. This supplements the [workflow inventory](../parity.md).
It uses committed Git objects, not the dirty reference checkout or the installed
Paseo binary. Commit descriptions are discovery hints; the lessons below were
checked against changed implementation and tests. We have not run Paseo's tests.
No Paseo implementation is copied into Fleetpane.

Two lineages matter:

| Source | Captured tip | Commits absent from the original reference |
| --- | --- | --- |
| [getpaseo/paseo](https://github.com/getpaseo/paseo) | `068f272a00ffef3f00ec30ec4cfa3a631be97624` (2026-10-20) | 608 |
| [DonutLabs-ai/paseo](https://github.com/DonutLabs-ai/paseo) | `798b5bb9cabc1ebd309fd08e0cbbe8651541b21e` (2026-10-19) | 551 |

The original reference is fork merge
`a486b0558168c08890890e787d55cfb88c0d7256`. Count reachable commits in each tip
that are not reachable from that reference, rather than using dates or assuming
the fork merge is an upstream ancestor. There are **680 distinct new commits**,
including 16 merges; 72 are exclusive to the fork's delta. These are discovery
counts, not 680 reviewed features, a maturity score, or a replacement claim.
The file-editor origin below predates that reference and was followed separately.

## Lessons and implementation order

| Priority / workflow | Evolution observed in source | Fleetpane action and acceptance |
| --- | --- | --- |
| P0: streaming must not amplify unrelated work | [Subscribe only opened chats](https://github.com/getpaseo/paseo/commit/9696f42264bafef1765f0133436b85dfe24d8e05), then [scope cache-read invalidation to requested rows](https://github.com/getpaseo/paseo/commit/e0de0fdf6f85d70620aa98312a29372f4682db47), [reuse workspace/subagent selectors](https://github.com/getpaseo/paseo/commit/559c0200e098cd257150c2b63c51470bfac7dd32), and [pause hidden status-ring clocks](https://github.com/getpaseo/paseo/commit/69ef741a73559b6fd00fc483aa1a331fac21c229). The cache test explicitly opens another agent/workspace/timeline while a stream remains active. | **Applied one bounded improvement:** card reconciliation now uses indexed identity membership and updates only changed/new cards. Previously each retained card scanned the full new row list and received a cloned row. Verify 1,201 rows over 200 tail updates, pagination/reorder/removal/session replacement, and native expanded-card/copy preservation. Still missing measured W1 UI latency and visibility-aware projection; full row construction remains per update. |
| P0: subscription replacement must retain final events | Fork [subscription handoff](https://github.com/DonutLabs-ai/paseo/commit/83eaadf27a4ea14bc9625a442f90066f01e0b621) keeps the acknowledged observation until its replacement is ready; tests cover a final answer already in flight. | **Contract for later subscription work.** Current Host cursor polling is different; do not transplant React subscription code. Before reducing hidden Host work, test delayed replacement readiness, final-only completion, event expiry, reconnect, controller fencing, and simultaneous approvals. |
| P0: provider history must not leak a process per archived chat | Upstream [release ACP history runtimes](https://github.com/getpaseo/paseo/commit/058375dac8f0f1e1bd3d348a4cfa0793141e819a) terminates the history-only child before returning, retains replayed history, and keeps child ownership on kill timeout. | **Open:** durable ACP transcript replay and complete Host retirement. Test child exit before return, retained transcript after release, interactive reload, kill timeout ownership and reconnect across many archived sessions. Fleetpane currently exposes context resume separately from transcript import; it must not advertise equivalent history yet. |
| P0: measure the UI, including timing boundaries | Fork [GUI bottleneck tracing and sidebar preview caching](https://github.com/DonutLabs-ai/paseo/commit/967119784ab8855498d720e718fed65cf4ec7090) adds bounded metrics, pending/dropped counters and explicit definitions for commit, paint wait and readiness. | **Open:** run PRD W0–W4 with comparable release builds and the full process tree. Separate command acceptance, first visible content, completed rendering and input/switch p95. Keep diagnostics bounded and exclude prompt text, paths and credentials. A Rust port or unit operation count does not establish responsiveness. |
| P1: file editing is a lifecycle, not a Save button | [Editor origin](https://github.com/getpaseo/paseo/commit/4bda2dfea9e32166e85471ed16b38da2f64776a2) spans editor state, observed versions and conflict handling; [responsive large previews](https://github.com/getpaseo/paseo/commit/c763b8082436a9758b419544e8aa5fc8540044cc) separates editing from bounded read-only display. Later fixes preserve [two-dot names](https://github.com/getpaseo/paseo/commit/114389383d6f6c78a9e1580ab245b3edc81feae8) and use a fixed-length temporary basename for [long valid filenames](https://github.com/getpaseo/paseo/commit/a310444c67f9f93150582db6dbaa3df2ee94ce7a). | **Next functional slice; still read-only today.** Shared expected-version save, retained dirty text, explicit reload/discard/conflict choices, close/navigation protection, permissions, exact Unicode/CRLF and bounded input. Test external edit/delete/replacement and long/two-dot names before native acceptance. Rechecking a version then renaming is not atomic compare-and-swap against independent external writers; specify the supported contract rather than promising that guarantee. |
| P1: local image previews belong with file navigation | [Local images inside Markdown file previews](https://github.com/getpaseo/paseo/commit/9eaf7ad0c1be74995b07dc82c81d9a010e932418) extends the file workflow after the editor foundation. | **Open:** bounded workspace-relative image loading, root confinement and visible unsupported/size errors. Existing conversation images do not establish file-preview support. |
| P1: creation retries and first-prompt failure are separate | [Durable creation deduplication](https://github.com/getpaseo/paseo/commit/d63132a37dd49c38af400dbbcd8f8b7ebd519339), then [keep the created agent after a failed first prompt](https://github.com/getpaseo/paseo/commit/29db2b79809fc032860ff4e8ee360fbe0073c3e9). | **Partly covered:** Fleetpane creation intents already bind one UUID to one Host; unknown outcomes remain visible. Add first-prompt-failure acceptance to future automatic initial-prompt flow, without recreating a session or silently changing its Host. |
| P1: provider capability controls the action | [Unsupported plugin steering returns unavailable](https://github.com/getpaseo/paseo/commit/efd6d95f91db3b6f028467e22cdf2652436555b4), with supported/unsupported provider tests. | **Current matching boundary:** Codex exact-turn follow-up is supported; DeepSeek ACP steering is explicitly unsupported. Future provider adapters must not silently convert steering into a fresh turn or clear permissions without the provider's contract. |
| P2: archive and delete need ownership and serialization | Fork [cold worktree storage](https://github.com/DonutLabs-ai/paseo/commit/1e03cb490e1193aa849e7537cdf3daf082d7a93e) preserves files and index before removal; [separate permanent deletion from the archive queue](https://github.com/DonutLabs-ai/paseo/commit/535874297b8c4518176301ea0a4417e46761bdbc) serializes by workspace and uses the shared chunk-store lock only for actual purge. | **Missing:** preview exact owned resources, active root/descendant processes, restorable uncommitted/index state and failure recovery. Ordinary directories must stay separate from managed worktrees. Run concurrent archive/restore/delete and shared-worktree tests before exposing destructive controls. |
| P2: avoid unnecessary process probes | Fork [routine CLI resolution without subprocess probes](https://github.com/DonutLabs-ai/paseo/commit/798b5bb9cabc1ebd309fd08e0cbbe8651541b21e) separates PATH lookup from executing a candidate to check availability. | **Design constraint:** preserve explicit provider argv; don't execute a configured terminal/provider just to resolve its path. Add side-effect and startup-count tests when executable discovery/terminal profiles are implemented. |

Git Diff expansion remains deferred at the user's request. Terminal, layout,
remote transport, packaging and the remaining parity rows remain in scope; this
history review does not remove or claim completion of them. Session Host remains
the accepted architecture, including its existing drain/ownership boundaries.

## Repeatable tracking

The checked-in [checkpoint](paseo-history.json) pins both captured tips.
From a full reference clone with those two remotes, explicitly update only the
remote-tracking refs, then generate the incremental report:

```sh
git -C /path/to/paseo fetch --no-tags upstream main
git -C /path/to/paseo fetch --no-tags origin main
python3 scripts/paseo-history.py /path/to/paseo > /tmp/paseo-new-commits.json
```

The reader never fetches, checks out, stages, writes checkpoints or reads dirty
source contents. `git --no-optional-locks` also avoids optional index refresh
writes. It records HEAD, dirty-state presence, source baseline/target, exact SHA,
changed paths and commit links. Shared commit SHAs appear once with both source
memberships. Merge commits remain explicit and receive no invented file diff.
Path-based topics are discovery hints, **not** inferred feature completion or
priority. Newline/Unicode paths remain lossless JSON strings. Missing objects,
shallow history, ref rewrites in incremental mode, and the per-source commit
limit produce errors instead of a partial success.

Use `--from-reference` to reproduce the initial reachable-set comparison.
Default incremental mode starts at each recorded source tip and requires it to
remain an ancestor of the corresponding remote ref. An unchanged pair returns
zero new commits. Reports use local fetched refs and do not claim remote freshness
unless the fetch was performed and recorded. No background schedule is installed.

For each selected change, inspect its final diff and regression tests, map it to
Fleetpane's engine/Host/UI boundaries, implement one complete workflow, and add
local and native evidence to [status](../status.md). Advance the checkpoint only
as an explicit source-capture update, recording which commits were actually
reviewed. Commit messages can describe intermediate approaches: for example,
`e0de0fdf6`'s final diff uses requested-row invalidation, so its message's broader
cache-overlay claims are not used here as proof.

[Local verification](../verification/2026-10-20-paseo-history.md) records the
actual reader and card reconciliation checks.
