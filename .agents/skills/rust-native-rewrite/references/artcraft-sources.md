# Sources and adaptations

Checked 2026-10-18. These are original notes based on public documentation, not a
copy of ArtCraft's internal standards. The pinned links describe the inspected
versions; refresh before making claims about current ArtCraft capabilities.

## Primary sources

- [PhotoCraft agent guide, 8d2e615](https://github.com/storytold/photocraft/blob/8d2e615ab93d424c4ec25ab15b4edb5368d1f9a6/AGENTS.md):
  command engine shared by GUI/CLI/control/MCP; thin UI; dependency-layer checks;
  synthetic regressions and pinned corpora; visual verification; performance
  scorecards; parallel-agent ownership. Its introduction explicitly says the
  shared `craftrules` repository is not public.
- [PhotoCraft target-app parity, same revision](https://github.com/storytold/photocraft/blob/8d2e615ab93d424c4ec25ab15b4edb5368d1f9a6/docs/target-app-parity.md):
  separates feature breadth from readiness for professional use; distinguishes
  measurements from estimates; records unsupported workflows and performance
  gaps. Its dated headline is an author estimate, not an independent benchmark.
- [WordCraft architecture, 4a251d4](https://github.com/storytold/wordcraft/blob/4a251d4b9c724a5bd3dd75f7c91c6aae77c6650e/docs/architecture.md)
  and [agent guide](https://github.com/storytold/wordcraft/blob/4a251d4b9c724a5bd3dd75f7c91c6aae77c6650e/AGENTS.md):
  UI-independent layers, shared command dispatch, headless operation and visual
  comparison. Some original planning documents are local and gitignored.
- [FigCraft agent guide, 1c3e175](https://github.com/storytold/figcraft/blob/1c3e175f66225d5e5219ecb4ddd888d69df0d84e/AGENTS.md):
  replaceable UI layer; behavioral rather than menu-only parity; programmatic
  calls do not unexpectedly open dialogs; inspect screenshots after UI work.

## What was found but not installed

[midasmatterhorn/artcraft-skills](https://github.com/midasmatterhorn/artcraft-skills)
is a community collection of thirteen app-operation skills, covering commands,
formats, automation and known limits of individual Crafting Apps. It is not an
official general-purpose migration methodology. Installing these app skills
would not supply the native rewrite workflow requested here.

The official guides reference `storytold/craftrules`; its public GitHub URL
returned 404 on the inspection date. This skill makes no claim to reproduce its
unavailable contents.

## Deliberate adaptations

- Reuse architecture and verification ideas, not a requirement to use egui, a
  specific number of layers, WASM, or a particular proprietary-product policy.
- Inspect source only where its license and the task permit it. A permitted
  open-source port does not require the black-box-only restrictions used for
  proprietary Adobe/Microsoft compatibility projects. Track provenance for any
  reused code and assets.
- Avoid importing broad panic-catching, default-returning, unconditional test
  expansion or automatic commit rules. Follow the current project's root-cause,
  error-handling, verification and authorization rules.
- Whole-application release comparisons, bounded subscriptions and workload
  scaling checks are adaptations for concurrent-session applications. They are
  not attributed as specific ArtCraft prescriptions.
