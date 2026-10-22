# Functional parity with donut-paseo

Conversation image dialogs now provide Fit/100%, two-axis scrolling, original
source Copy and Kit modal close/focus restoration, with
[Linux fixture evidence](verification/2026-10-22-image-viewer.md).

Native Markdown file Source/Preview, confined explicit links and no automatic
image fetching now have [Linux fixture evidence](verification/2026-10-22-markdown-preview.md).
Provider context observations show exact Codex/ACP counts, capacity/unavailable
states and original parsed telemetry Copy, with independent pending choices and
900×560 controls [verified locally](verification/2026-10-22-context-usage.md).
Installed-provider telemetry, large-document Copy and other platforms remain open.


Provider choices now use native Kit search by name, ID and description. Duplicate
labels retain separate IDs, stale leases are refused and exact drafts survive
window replacement. [Linux fixture evidence](verification/2026-10-22-model-search.md)
covers Codex and ACP, including 900×560. Live catalog refresh, favorites, creation
defaults and installed-provider acceptance remain open.

Native session sidebar widths now support Kit divider dragging and explicit
Narrow/Widen/Reset controls, bounded beside the conversation. Per-Host hide/show,
small windows, appearance switching and exact drafts have [Linux fixture
acceptance](verification/2026-10-22-sidebar-resize.md). Width is window-local;
persisted layout and workspace tabs remain open.

CLI Send and Codex live follow-up now accept prompt files or stdin with exact
UTF-8 preservation and validation before Host control. [Local CLI evidence](verification/2026-10-22-prompt-input.md)
covers the 64 KiB boundary, FIFO refusal and unchanged Host on input errors;
this is separate from GUI and external-provider acceptance.

Last inspected session recovery is locally verified for shared/CLI/Linux native
fixtures: exact Host/session/directory/archive identity, explicit opening after
takeover, retained Chinese draft and conflicting write refusal.
[Evidence](verification/2026-10-22-last-session.md). Complete tabs/scroll/layout
restoration remains open. Chronological scoped review now reaches the pinned ordinal 6215 plus upstream
documentation delta `1198cddf` (remote tips checked 2026-10-22);
this is a source-review cursor, not a parity completion count.

[Composer focus](verification/2026-10-22-composer-focus.md) now uses Ctrl/Cmd+L
with native fixture evidence for exact drafts and ownership/palette guards.

The bounded cross-Host session switcher now uses Ctrl/Cmd+K and Kit search,
with exact source identity, explicit omissions and read-only takeover boundaries.
[Local fixture evidence](verification/2026-10-22-session-switcher.md) covers drafts,
window replacement and 900×560; full-index search and IME remain open.

ACP advertised commands (2026-10-22): live exact-session command menus prepare
ordinary drafts without sending. Idle/pending replacement, empty lists,
archive/resume expiry, exact Copy, Chinese drafts, window replacement and
900×560 have [local fixture evidence](verification/2026-10-22-acp-commands.md).
Installed-provider advertisement, Codex command discovery and IME remain open.

Updated 2026-10-21. The user's target is donut-paseo's useful workflow completeness,
with selective adoption rather than an identical port. OpenCodex and DeepSeek
Harness are the current provider scope; Kimi and GLM are excluded. Other providers
need an actual use case before inclusion. Milestones sequence delivery within
this scope.
There is no overall readiness percentage. A visible control, protocol schema or
passing synthetic test does not establish a complete production workflow.

Reference: local Paseo fork at `a486b0558168c08890890e787d55cfb88c0d7256`.
Its Codex provider and tests contain uncommitted changes. This inventory describes
the inspected source tree, not an attestation of the installed application.
Sources below are paths relative to that reference tree. No source/assets are
copied into Fleetpane. Fleetpane evidence is in [behavior status](status.md).

The [chronological review](reference/paseo-replay.md) follows upstream and the
Donut fork from the first commit, using captured 2026-10-20/19 tips. Its explicit
review cursor separates complete indexing from inspected behavior. It guides
missing-workflow implementation; recent deltas are supplemental. It does not
attest the installed application.

CLI batch migration also has [cooperative signal Stop](verification/2026-10-21-paseo-signals.md):
Ctrl+C/SIGTERM finishes the current unit and saves a stopped report for explicit
retry. This extends offline migration reliability; it does not transfer running
Paseo providers.

| Workflow / observable result | Reference source or scenario | Fleetpane status and next acceptance |
| --- | --- | --- |
| Find and revisit pending requests across Hosts | PRD ATT-01–05; per-agent request/status and registry evolution in history 145–150 | **Partial, locally verified** read-only global inventory, approval/question/other filters, exact raw copy and checked source routing. Explicit takeover, external-resolution refresh, original native answers and exact draft preservation covered ([evidence](verification/2026-10-21-attention-queue.md)). Host filters and queue-local keyboard selection/open/copy now have [separate acceptance](verification/2026-10-21-attention-navigation.md). Explicit completion/failure/interruption observations and exact local review now have [separate acceptance](verification/2026-10-22-turn-results.md). Request creation times, a global next-request shortcut, OS notification deduplication and configurable mute/tray remain open. |
| Add a project, open an existing directory, create an isolated or shared workspace | `screens/new-workspace-screen.tsx`, `e2e/browser/new-workspace.spec.ts` | **Partial, locally verified**: durable per-Host catalog, existing folders, main/linked Git status, saved-cwd sessions and pinned-base independent worktree/new-branch creation. Selected subdirectory, window reopen, one-attempt dispatch, retained failure, source identity and independent edits covered. Existing local-branch checkout now verifies tip/occupancy, cross-store guard, cancelled awaiter and native reopen/apply/session cwd ([evidence](verification/2026-10-21-existing-worktrees.md)). Missing worktree removal/archive, setup scripts, external Git fencing and supervised recovery of interrupted Git; [catalog](verification/2026-10-20-workspaces.md), [creation](verification/2026-10-20-worktree-creation.md). |
| Navigate many workspaces and tabs; rename, pin and preserve layout | `screens/workspace/workspace-tab-model.ts`, `e2e/browser/sidebar-workspace-pin-shortcut.spec.ts` | **Partial, locally verified** durable local workspace pins, independent of catalog pagination; native same-path Host separation, reopen, unpin preservation and checked quick-use verified ([evidence](verification/2026-10-20-workspace-pins.md)). Missing workspace tabs/layout, rename, pin shortcuts, drag ordering and restored selection; the flat session directory is still not equivalent. |
| Keep unsent text across session switches and desktop replacement | `stores/draft-store/persistence.ts`, `stores/draft-store/state.ts`, `stores/draft-store/draft-keys.ts` | **Partial, locally verified** private revisioned session drafts, exact multiline/Unicode restoration, concurrent-window conflict handling, successful-send clearing and conservative pending-send recovery. Normal close waits for coalesced writes. Missing options/media/question drafts, pre-creation drafts and restored selection; earlier Host incarnations use CLI recovery. [Evidence](verification/2026-10-20-session-drafts.md). |
| Create, reopen and continue sessions with the installed provider configuration | `docs/providers.md`, `messages.ts` provider/session schemas | **Partial, locally verified** for covered OpenCodex and DeepSeek workflows. Missing account setup and remaining workflows for these providers. Other reference providers are not a current acceptance prerequisite; Kimi/GLM are out of scope. |
| Choose model/reasoning and provider-specific options | `e2e/browser/agent-profile-modeless-preferences.spec.ts` | **Partial, locally verified** model/effort controls for Codex and ACP. ACP v1 advertised session modes are also locally verified, including active changes, provider updates and archive/resume ([evidence](verification/2026-10-21-acp-modes.md)). Missing profiles, permission presets, v2 negotiation and workspace defaults. |
| Send, stop, steer an active turn, queue input and recover ambiguous outcomes | `docs/providers.md` active-turn steering contract, `e2e/browser/permission-steer.real.spec.ts` | **Partial, locally verified** send/stop with explicit ACP stop reasons plus explicit Codex same-turn follow-up, durable draft/operation identity, exact acknowledgement and no new-turn fallback ([evidence](verification/2026-10-20-active-turn-follow-ups.md)). Real OpenCodex receipt, same-turn user item and final response verified. [Host-owned Stop progress](verification/2026-10-21-stop-progress.md) now separately gates acknowledgement and exact-turn settlement, with native explicit retry and other-session isolation. DeepSeek ACP follow-up is explicitly unsupported; cross-provider queues, cancellation of unread steers and permission-clear semantics remain open. |
| Inspect live and saved Markdown, tools, images, supplied diffs and unknown blocks | `e2e/browser/paseo-tool-call-details.codex.real.spec.ts`, `agent-tab-image-stability.spec.ts` | **Partial, locally verified** for covered content; ACP tool defaults, exact patch semantics, name/kind/location details and retained expansion/copy now have [separate acceptance](verification/2026-10-21-acp-tools.md); local image selection/send has [separate evidence](verification/2026-10-21-image-attachments.md); native clipboard image drafts/reopen/send and refusal have [separate acceptance](verification/2026-10-21-clipboard-images.md); native file drops, ordered batch refusal and exact send/reopen have [separate acceptance](verification/2026-10-22-image-drop.md); explicit conversation file/line navigation now has [separate acceptance](verification/2026-10-21-file-navigation.md). Conversation Fit/100% image dialogs now have [separate acceptance](verification/2026-10-22-image-viewer.md). Remote upload, Mermaid and complete content/sandbox limits remain open. |
| Read old history incrementally; retain scroll position and jump to a prompt | `e2e/browser/agent-timeline-pagination.spec.ts`, `chat-outline.spec.ts` | **Partial, locally verified** bounded Codex/observed-ACP pages and live reconciliation. Observed ACP messages/tools now persist across Host restarts with unknown unfinished turns and explicit context resume ([evidence](verification/2026-10-21-acp-archive.md)). Provider session information now updates titles without prompt activity and survives archive restart ([evidence](verification/2026-10-21-acp-info.md)). Loaded prompt navigation now has [separate native acceptance](verification/2026-10-22-prompt-outline.md). Missing history window eviction, full-index prompt lookup, full-text search and provider transcript import. |
| Inspect supplied resource contents and references | Tool-output evolution in replay 151–156; explicit ACP/MCP content contracts | **Partial, locally verified** embedded text/Markdown, binary placeholder, resource-link metadata, exact text/URI/base64 copy, malformed/future content and ACP archive reload ([evidence](verification/2026-10-21-resources.md)). Fetching/browsing, binary preview, audio and outgoing embedded context remain open. |
| Answer exact permission choices and agent questions; see task/plan progress | `e2e/browser/acp-permission-choices.real.spec.ts`, `question-prompt-pagination.spec.ts`, `provider-task-list.real.spec.ts` | **Partial, locally verified** synthetic exact ACP permission/Codex question interactions, standard ACP/Codex task notifications and native task panel. Real DSH successful `todo_write` updates now project task lists; replacement, clear, copy, cache recovery and unknown status covered ([evidence](verification/2026-10-20-agent-tasks.md)). Real OpenCodex emitted no plan tool call; full real permission/question acceptance remains open. DSH ACP does not expose Codex elicitation. |
| Search/import saved provider sessions and preserve their existing history | `e2e/browser/import-session-flow.spec.ts`, `docs/providers.md` import contract; explicit Paseo migration request | **Partial, locally verified** offline Paseo workspace/session preview and identity mapping, separate private Codex/DeepSeek transcript snapshots, deduplication, explicit failures and native paged browsing. Two real histories copied in temporary stores. Batch selection, progress, cooperative Stop, repeat execution and per-workspace diagnostics have [separate acceptance](verification/2026-10-21-paseo-batch.md). Independent continuation from complete legacy Codex snapshots has [separate acceptance](verification/2026-10-21-paseo-fork.md), including [explicit unsent reprepare](verification/2026-10-21-paseo-reprepare.md). DeepSeek v3/v4 generation selection, native tool-result retention and real archive readback have [separate acceptance](verification/2026-10-21-paseo-dsh-v4.md). Durable reports, process interruption and explicit saved-selection retry have [separate acceptance](verification/2026-10-21-paseo-receipts.md). Explicit versioned scratch inventory/collection with active-writer fencing and legacy preservation has [separate acceptance](verification/2026-10-21-paseo-scratch.md). DeepSeek/paginated Codex continuation, running-process transfer, other providers, legacy-file/automatic collection, archive quotas and later synchronization remain open. [Evidence](verification/2026-10-21-paseo-import.md). |
| Fork, rewind and inspect parent/subagent relationships | `messages.ts` `agent.fork_context.request` / `agent.rewind.request`, `e2e/browser/subagent-detach.spec.ts` | **Partial** read-only session ancestry and loaded-context inspection have [separate evidence](verification/2026-10-21-session-tree.md); explicit checked history opening and return to the source have [native acceptance](verification/2026-10-22-tree-navigation.md); independent forks from imported legacy Codex snapshots have [separate acceptance](verification/2026-10-21-paseo-fork.md). General live-session fork/rewind, subagent mutation and provider-specific closure remain open. |
| Archive/restore sessions or workspaces without losing active work | `messages.ts` archive requests, `e2e/browser/auto-archive-after-merge.real.spec.ts` | **Partial** provider archive browse/read/restore, exact history/draft retention and separate context resume verified with native fixtures and installed Codex process checks ([evidence](verification/2026-10-21-archive-restoration.md)). Read-only ancestry/loaded-context inspection is now locally verified ([evidence](verification/2026-10-21-session-tree.md)); Codex archive still requires explicit-stop/resource closure and transaction fences before mutation UI. Workspace/worktree archive and deletion remain open. |
| Run terminal commands, resize, scroll, paste and reconnect to the same PTY | `screens/workspace/terminals/use-workspace-terminals.ts`, `e2e/browser/terminal-keystroke-stress.spec.ts` | **Partial** Linux Host-owned PTY and shared CLI verified with real shells and fixture providers: workspace/Host isolation, input sequencing, resize, bounded raw output, controller detach/transfer, drain and exit. [Evidence](verification/2026-10-20-host-terminals.md). Native ANSI/Unicode renderer, alternate screen, scrollback/copy, explicit-input/control buttons, resize/rename/close, draft isolation and window replacement have [separate acceptance](verification/2026-10-21-native-terminals.md). Direct keys, cursor/paste modes, ordered bounded input and waiting for accepted input on close have [native evidence](verification/2026-10-21-terminal-keyboard.md). Selection/IME, conversation attachment and Host update recovery remain open. Provider-fault independence and storage fencing now have [separate evidence](verification/2026-10-21-terminal-fault-domains.md). |
| Explore, read and edit workspace files with safe save/conflict behavior | `e2e/browser/file-explorer-collapse.spec.ts`, `file-editing.spec.ts`, `file-change-conflicts.spec.ts` | **Partial, locally verified** shared local listing/read APIs and native single-directory browser, read-only text preview, exact full copy, explicit refresh, binary/empty/size notices and bounded workers. Chinese/CRLF/long-copy, directory limits, symlink/FIFO/traversal refusal and replaced-root recovery verified without changing running Hosts ([evidence](verification/2026-10-20-workspace-files.md)). PNG/JPEG/WebP single-file previews now have [separate acceptance](verification/2026-10-21-file-images.md). Bounded native Back/Forward and directory row recovery have [separate acceptance](verification/2026-10-21-file-history.md). Sequential image navigation with fresh directory/read, native PNG/JPEG/WebP, explicit errors and small-window interaction now has [separate acceptance](verification/2026-10-21-image-navigation.md). Raw local new-copy export now has [separate acceptance](verification/2026-10-22-file-export.md). Complete numbered/highlighted read-only text, wrap and original source anchors now have [separate acceptance](verification/2026-10-22-text-preview.md). Source Find and Markdown Preview now have [separate acceptance](verification/2026-10-22-file-find.md). Missing editing/conflict-aware save, recursive tree, watchers/workspace-wide search, thumbnail grid/zoom, embedded Markdown-file images, other media and large-file streaming. |
| Review working-tree/commit changes, switch branches and use Git/PR checks | `messages.ts` checkout requests, `e2e/browser/branch-switcher.spec.ts`, `pr-pane.spec.ts` | **Partial, locally verified** read-only per-file staged/unstaged diffs in CLI/native, add/delete/rename/mode/untracked, binary/conflict/size errors and raw patch copy ([evidence](verification/2026-10-20-workspace-diffs.md)). Missing commit/base comparisons, untracked-directory expansion, branch switching, Git writes, PR/CI operations and conflict resolution. |
| Run workspace scripts and scheduled/recurring tasks with per-run history | `screens/workspace/workspace-scripts-button.tsx`, `screens/schedules-screen.tsx` | **Missing** script/automation service, run identities, cancellation and deduplication across control replacement. |
| Connect another host/device, authenticate/pair and recover reconnects | `docs/product.md`, `e2e/browser/pair-device-relay.spec.ts`, `relay-deployment-reconnect.real.spec.ts` | **Partial** local named-Host workbench with independently retained views verified, including real OpenCodex/DeepSeek. **Missing** authenticated remote transport, SSH/relay, pairing and durable routing. Local endpoint selection is not remote access; [evidence](verification/2026-10-20-multi-host.md). |
| Configure appearance, shortcuts, notifications, plugins, voice and browser tools | `screens/settings-screen.tsx`, `e2e/browser/plugin-host-ui.spec.ts`, `docs/product.md` | **Partial** persistent System/Light/Dark and Linux native OS-event/reopen/conflict/failure/close-guard acceptance ([evidence](verification/2026-10-21-appearance.md)). Device preferences do not change Host/provider settings. Shortcuts, fonts/language/layout, notifications, plugins, voice and browser workflows remain missing or unaccepted; keep each extension's resource ownership explicit. |
| Install/update the app and daemon while sessions keep running | Reference daemon-independent clients; Fleetpane [Host ADR](adr/0002-preserve-runtimes-across-control-daemon-upgrades.md) | **Partial** same Host/provider across control-window replacement, durable RPC receipts and conservative crash reconciliation verified locally. Persistent drain preserves old work/answers/Stop; explicit groups now recommend a ready Host and prepare its normal session creation flow, with stale target rejection and no cross-group retry. Durable initial creation bindings and at most one dispatch per UUID now survive window replacement; receipt checks stay on the original Host. Missing complete session routing, active-provider/tool restoration, resource-complete retirement and packaging/updater; [drain evidence](verification/2026-10-20-host-drain.md), [selection evidence](verification/2026-10-20-host-selection.md), [creation evidence](verification/2026-10-20-creation-intents.md). |
| Many active sessions remain correct and the UI stays responsive | `e2e/browser/replica-cache-performance.spec.ts`, `terminal-keystroke-stress.spec.ts`; Fleetpane PRD W0–W4 | **Partial** 20 synthetic ACP streams with reads/cancel/controller replacement verified. No same-machine incumbent comparison, UI latency measurement, real 20-agent resource baseline or long soak. |

`screens/` and `e2e/` above are under `packages/app/src/` and `packages/app/`
respectively; bare spec filenames use the same `packages/app/e2e/browser/`
directory. `messages.ts` is `packages/protocol/src/messages.ts`.
Reference tests describe expected workflows; they were not run as Fleetpane QA.

Follow the user's root-to-tip history review when choosing the next missing
workflow. The first reviewed phase exposes terminal ownership and interaction
as an early gap; conversation isolation and lifecycle follow in the next review
batch. Existing verified features are not rebuilt. Complete provider recovery,
durable ownership routing, workspace lifecycle, remote access, automation and
remaining provider/extension workflows remain required. Git Diff expansion is
deferred; voice retains its PRD priority. Each implemented slice must include
shared command behavior and real native acceptance. Release performance budgets
must be measured with the full required process tree. The Host journal now persists UUIDs beyond its resident cache; its 256 MiB disk
budget has no retention policy yet. Provider recovery gaps and the 64-resident-session
ACP cap still preclude a replacement claim.

ACP cancellation follow-up (2026-10-21): pending and in-flight permission replies
are cancelled when Stop is requested; provider settlement is still required.
Private process tests cover other-session isolation, stale answers, duplicate Stop
and restart during cancellation. Linux native window replacement preserves the
waiting state and draft. See [evidence](verification/2026-10-21-acp-cancel.md).
Paseo root replay now covers scoped paths through ordinal 75 (`30bf4d1b6`), with
76 (`c6cdc09a0`) next; metadata indexing is still separate from source review.

ACP permission review (2026-10-21): preview the request's ToolCallUpdate even when
no initial tool event exists; request fields replace older observed values without
rewriting conversation history. Complete raw request copy, long content scrolling,
small-window controls and exact Host-owned choices have separate local evidence.
See [verification](verification/2026-10-21-acp-review.md). Root replay now covers
scoped paths through ordinal 77 (`0893a9dc3`); 78 (`a930c4156`) is next.


Recent directories (2026-10-21): three endpoint-scoped suggestions after successful
native session creation, exact path prefill, transactional MRU/limits, shared CLI,
forget/reopen and independent save-failure reporting now have [local evidence](verification/2026-10-21-recent-directories.md).
This complements explicit saved workspaces and pins; home shorthand and automatic
cross-window refresh remain open. The offline paint suspicion was a preview false
positive; [saved frames and the unchanged binary were rechecked](verification/2026-10-21-render-inspection.md).
Scoped-path Paseo review reaches ordinal 201 (`7d8979e12`); 202 (`4ab79b1a5`) is next.
Tool details and request previews in 87–91 were compared with existing contracts;
92 exposed the optional ACP v1 mode gap now covered by local implementation and acceptance.
The ledger records partial hunk scope for 91; no mobile or voice runtime claim.

Review 93–101 covers the shared footer evolution, message memoization, confirmed
permission removal and voice gating. The native poll path now separates metadata
refresh from timeline invalidation ([evidence](verification/2026-10-21-timeline-invalidation.md));
content-event row construction and end-to-end performance budgets remain open.

Review 102–113 covers provider-specific resume identities, lazy initialization,
shutdown deadlines and input height. Fleetpane keeps observational history reads
separate from explicit context resume. The native composer now grows with content,
with responsive row limits and independent interaction scrolling
([evidence](verification/2026-10-21-bounded-composer.md)). Last-activity sorting,
relative ages, broader media and full resource-complete Host retirement remain open. Local image attachments from review 114 now have separate native/provider acceptance; installed DSH does not advertise image support.


Review 115–124 separates preserved session identity from a fresh-session fallback,
mode confirmation from printed logs and native metadata updates from history
rebuilds. Existing archive/mode contracts remain in use; archive deletion and full
Host retirement remain open. Local controller operations now share a canonical
store queue, including independent handles and cancelled blocking work
([before/after evidence](verification/2026-10-21-controller-store-queue.md)); this is
separate from cross-process fairness and performance acceptance.


Review 125–127 compares worktree transport and manual-test limits with the existing
planned worktree engine. Native **Jump to latest** now resumes tail follow without
fetching, sending or clearing drafts; [native evidence](verification/2026-10-21-history-navigation.md)
includes a growing final row and an independent pending permission. Unread counts,
scroll restoration remain open. The minimum-window reading gap recorded here has
subsequent [layout acceptance](verification/2026-10-21-conversation-space.md). Review scope for
126/127 is deliberately limited to the ledger's listed paths.

Review 128–131 separates initial scroll behavior, successive mobile keyboard fixes
and duplicate-initialization UI state from Host admission. Desktop Kit tail
intent and explicit resume contracts remain in use. No frame-by-frame no-flash or
mobile keyboard acceptance is claimed. Existing-branch worktree creation closes
the earlier review 125 gap with separate real Git and native evidence.

Review 132 replaces historical ScrollView with inverted FlatList. Fleetpane already
has a virtual list; the reproduced gap was allocation of the remaining height.
Bounded scrollable details now share that height with messages, preserving the
fixed editor. Native before/after, exact text/image send, history, approvals/Stop
and two-question submission are verified separately. No virtualization speedup,
universal pane sizing or historical mobile runtime claim is made.

Review 133–138 maps multi-agent definitions to existing independent Hosts and
runtime-advertised modes. Static permission fallback, unsupported-resume new
sessions and new-message implicit permission cancellation are not adopted.
Pre-creation mode preferences, provider setup UI and orchestration MCP remain
separate gaps. Mobile modal/menu/input changes do not replace Kit; Git Diff
expansion remains user-deferred. Scoped paths and exclusions are in the replay
ledger, with no historical runtime or quantitative contrast acceptance.

Review 139–141 preserves existing Markdown quote/Copy behavior and separates
dispatch from waiting. Shared exact-turn CLI observation is now locally verified
for completion/failure/refusal, pending requests, timeout/SIGINT, controller
replacement, lookup/completion ordering and restart-unknown, including installed
Codex against one gated loopback response. [Evidence](verification/2026-10-21-wait-turn.md).
Public orchestration MCP, curated activity export, push subscriptions and external
model/performance acceptance remain open. Historical prompts are reference data;
the Expo dependency update does not change native clipboard implementation.


Conversation identity follow-up (replay 157–161): native turn/status and item keys
now use tagged tuples. Old-binary pagination reproduced status/message aliasing
and one command replacing another; native independent expansion and five exact
Copy checks, including reload, pass after correction. See
[acceptance](verification/2026-10-21-row-identity.md). This does not implement fuzzy
missing-ID reconciliation or conflicting provider-history repair.

Agent progress follow-up (2026-10-21): selected-session activity now has a shared
Host-snapshot projection and a fixed native indicator, separate from client action
completion. Quiet work, pending Stop, independent approval, 38-byte draft and
window replacement have [local fixture evidence](verification/2026-10-21-agent-progress.md).
The concurrency checker now drains startup events and matches exact turn IDs;
20 synthetic streams pass. Real model capacity, UI performance and native fault
recovery remain unaccepted. Normal titlebar close now has [separate Linux
acceptance](verification/2026-10-21-native-close.md); the automation's direct X
window destruction remains non-graceful.

Reasoning deltas (2026-10-21): indexed Codex summary/raw-text live updates and
per-item history boundaries now have [local native and regression evidence](verification/2026-10-21-streamed-reasoning.md).
Empty items stay hidden, Copy preserves fields, and Stop retains its exact-turn
contract. External model and cross-platform native reasoning acceptance remain open.
Scoped-path chronological review reaches 195 (`47a99f566`); 196 (`6aa9b1c72`) is next.


Review 202 adds explicit per-daemon directory accessors and grouped sessions.
Fleetpane now has an observational cross-Host session directory with bounded
metadata pages, native title/Host filtering, pinned source opening and explicit
partial errors ([evidence](verification/2026-10-21-fleet-directory.md)).
Provider ordering remains in effect; remote connections, registry editing,
subscriptions and global activity sorting remain open. Seven new source paths
were inspected; the other 26 changed paths remain outside this review scope.

Preferred Host (2026-10-21): exact configuration-local endpoint selection now survives
window replacement and Host rename/reorder. Offline/missing endpoints, explicit
conflict/save-failure handling and normal close have [local evidence](verification/2026-10-21-preferred-host.md).
Session/scroll restoration, editable registry and remote connections remain open.
Scoped chronological review reaches 203 (`59967aad8`); 204 (`04d51fbe1`) is next.


Cross-Host directory filters (2026-10-21): provider Archived/Sessions membership,
exact workspace and title searches now share bounded provider queries in the CLI
and native All sessions view. Source opening preserves archive membership even
when runtime status is idle, with explicit restore and exact draft preservation.
ACP archive listing remains unsupported and visible; see [acceptance](verification/2026-10-21-directory-filters.md).
Scoped review reaches 207 (`5807f62f1`); next is 208 (`db87f331f`).


Per-Host diagnostics (2026-10-22): native Hosts cards and shared
`host-diagnostics CONFIG [HOST_NAME]` report count-only snapshot observations,
protocol compatibility, drain/fault, controller/process identity and check timing.
Single/all checks, exact JSON copy, 8 endpoints, stalled response isolation,
900×560 scrolling, offline recovery and retained drafts/requests have
[local acceptance](verification/2026-10-22-host-diagnostics.md). Remote auth,
registry editing and safe Host retirement/update remain open.
Scoped chronological review reaches 214 (`881f0e7a8`); next is 215 (`393aa44e6`).

Observational read recovery (2026-10-22): native same-Host event replay keeps the
original cursor, local draft and explicit mutation receipts; window activation
wakes reads. Transport failures recover automatically, while expired history and
protocol errors require Reconnect, and revoked leases require explicit takeover.
Owned Linux Codex/ACP fixtures retain process identities and pending requests;
no OS suspend, remote or external-model acceptance. See
[verification](verification/2026-10-22-read-recovery.md).

### Cross-Host loaded activity order

Implemented and verified locally for covered engine/Linux native fixture paths.
Activity order places fresh active rows first, then provider update time; missing
or malformed time is explicit, and stale observations sort after fresh rows.
By Host retains provider page order. Exact endpoint/Host UUID/session identity,
archive membership and pagination remain independent. Native history/draft
readback, 237-row paging, offline labels and minimum size have
[separate evidence](verification/2026-10-22-activity-order.md).
This is ordering over loaded rows; full-index activity and realtime subscriptions
remain open, together with directory keyboard navigation and accessibility.

Loaded-session cwd suggestions (2026-10-22): selected-Host, current-filter/page
scope, exact path deduplication and bounded native menu now complement persistent
recents. Non-Git creation, exact path/draft copy, another Host and window replacement
have [separate acceptance](verification/2026-10-22-loaded-directories.md).
Scoped historical inspection reaches ordinal 402 (`9318048f5`); MCP automation
and injection remain open. Native Markdown selection already exists in Kit and
was observed without modifying the renderer.


Explicit MCP discovery (2026-10-22): shared fenced/journaled command and Linux
native catalog now have [local evidence](verification/2026-10-22-mcp-discovery.md).
Installed Codex 0.163.0 discovers a synthetic stdio tool without a model call.
Unknown auth/runtime values and schemas are retained; discovery is explicit and
may start configured servers. Config editing/OAuth, injected orchestration and
per-session connection status remain open.

Codex supplied web-search cards (2026-10-22) now have [local projection/native evidence](verification/2026-10-22-web-search.md): exact queries, actions, opaque results and future/invalid metadata. This does not implement external search or resolve the minimum-size conversation layout gap.

Supplied command actions and compact conversations (2026-10-22): ordered typed actions, separate read links, complete Copy and retained drafts/window replacement have [local native evidence](verification/2026-10-22-command-actions.md). At 900×560 optional settings use explicit Session options while composer and approval/Stop stay reachable; arbitrary-content and cross-platform acceptance remain open.

Creation directory validation (2026-10-22): the Host rejects invalid paths before fresh admission for both current providers, retains exact cwd and original duplicate receipts. [Local native/process evidence](verification/2026-10-22-creation-directory.md) separates preflight observation from provider use.

Loaded turn answer copy (2026-10-22, replay 633) has [separate native evidence](verification/2026-10-22-turn-answer-copy.md). The action exports assistant text from loaded items only, preserves exact whitespace and excludes other roles/tools/media. Explicit paging, refusal, draft retention and window replacement are covered.
