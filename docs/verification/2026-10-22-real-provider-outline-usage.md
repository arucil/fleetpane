# Real OpenCodex and DeepSeek prompt navigation and context usage

2026-10-22, Linux X11 isolated workspace. Source `eab1e7b`, installed Codex
0.163.0 through the unchanged existing OpenCodex 2.89.0 loopback proxy;
DSH 0.2.0-rc.2 with deepseek-v4-flash. Each provider used an independent private
home and synthetic workspace. Two real model turns per provider completed with
the requested exact text tokens. [Artifact receipt](assets/prompt-outline-real/receipt.json).

- Codex initially displays [one loaded prompt](assets/prompt-outline-real/codex-menu-one.png).
  Explicit earlier-history loading reveals [both prompts](assets/prompt-outline-real/codex-menu-two.png).
  Selecting the first [reveals its actual Chinese message](assets/prompt-outline-real/codex-jump.png).
- DSH likewise loads its observed history and [shows both prompts](assets/prompt-outline-real/deepseek-menu-two.png);
  [selection reveals the first](assets/prompt-outline-real/deepseek-jump.png).
- Both fit at 900×560: [Codex](assets/prompt-outline-real/codex-small.png),
  [DeepSeek](assets/prompt-outline-real/deepseek-small.png), with composer and Send reachable.
- Real context telemetry is **13,684 / 258,400 tokens (5.3%)** for Codex's last
  usage, distinct from cumulative 27,321; DSH is **7,692 / 1,000,000 (0.8%)**.
  Both complete copied JSON observations equal their Host query results,
  including untouched raw provider notifications, exact session/turn and Host
  receipt time. Counts describe these two fixtures, not model-wide guarantees.
- Host/provider identities, pending requests, active work and mutation journal
  totals remain unchanged across native inspection. History/query sequence
  numbers may advance with explicit reads. No GUI Send was used in this check.

An initial test script read the wrong activity field and requested full history
before Codex's first turn settled. The provider returned `list_turns is not
supported yet`. Correcting the predicate to `activity.turns` let the same
completed turn read successfully. No product fallback/retry was introduced.

This adds installed-provider acceptance to the separate synthetic
[prompt-navigation](2026-10-22-prompt-outline.md) and
[context-usage](2026-10-22-context-usage.md) checks. It does not accept general
running-turn history, all provider versions, media/tools, performance, IME or
other platforms. Only owned private providers are stopped for cleanup; no shared
Paseo daemon, existing user session or OpenCodex proxy is restarted.

Cleanup readback confirms both owned real Host/provider PIDs and sockets are
gone, the desktop exited normally with code 0, and the private DSH credential
copy was removed. [Cleanup receipt](assets/prompt-outline-real/cleanup.json).
