# Provider model capabilities and next-message options

Accepted 2026-10-19 for the local Codex Host.

## Problem

Every prompt previously inherited provider settings. There was no model catalog,
model picker or reasoning control. Hardcoding model names or an effort enum would
drift from the installed provider and could advertise unsupported combinations.

## Decision

`fleetpane-session::models` owns catalog decoding, bounded pagination and
`TurnOptions`. The contracts were checked against the installed Codex 0.163.0
generated TypeScript schema. `Command::Models` requests `model/list` with
`includeHidden: false`, 64 models per page and the provider's opaque cursor.
The CLI exposes individual pages; the desktop automatically continues them.

Retained catalogs allow at most 256 models, 1 MiB of encoded model metadata and
4 KiB per cursor. Reject oversize/empty continuing pages, repeated cursors,
duplicate picker IDs or wire model names, duplicate/empty effort identities and
unsupported default efforts before applying a page. Page failures retain the
previous state. A failed first refresh leaves the old catalog available; a failed
later page leaves a visibly incomplete catalog that cannot admit model overrides.
Reload models starts a fresh traversal. Session defaults still work without a
catalog. Existing transport/frame/operation limits also apply.

Picker IDs and `displayName` are presentation identities. The actual `model`
string is sent to `turn/start`; effort strings come from that model's
`supportedReasoningEfforts`. Future effort strings are supported without an enum
update. `TurnOptions::new` requires a complete catalog and validates membership;
its fields are private. GUI and CLI use this validation and the same typed Send
command. CLI sends with overrides first read the complete current catalog.

The desktop loads capabilities after connecting and permits explicit reload.
The model and reasoning menus live above the composer and apply on the next
message. They neither restart a provider nor change an executing turn. Choices
belong to the selected session and survive switching/reconnecting while the
window stays open. Selecting another model resets effort to that model's declared
default. Models without effort options show a disabled No reasoning control.
Menus support keyboard navigation and do not expose hidden models. Opening a menu
captures the session identity; its handler cannot change a different session if
selection changes before activation.

Use session settings clears overrides: both fields are omitted from the next
request. It means the provider's *current session configuration*, which can have
been changed by a previous turn; it does not restore an original global default.
CLI `--model` without `--effort` likewise leaves effort to the provider. Explicit
effort requires an explicit model. Refreshing capabilities does not silently
replace a removed choice: keep its identity visible, show the validation reason,
and disable Send until the user chooses an available model or session settings.

The provider remains authoritative for actual model access and turn acceptance.
Catalog membership is not authentication or proof of entitlement. Rejected sends
retain text and choices. Catalog failures do not invalidate conversation replay;
they use the existing Host command identity and ownership fencing without retries.
All provider I/O occurs outside rendering. Catalogs and choices are not persisted
across desktop exits; configuration/authentication/mode selection remain separate.

## Archive boundary found during this slice

Codex 0.163.0's [thread processor](https://github.com/openai/codex/blob/rust-v0.163.0/codex-rs/app-server/src/request_processors/thread_processor.rs)
archives the root and spawned descendants and prepares loaded threads for shutdown.
Simply wiring that RPC would not satisfy WS-05's explicit-stop requirement.
Archive/restore remains unimplemented until session-tree impact and activity
checks, explicit stop and provider lifecycle races have their own acceptance.
This slice does not change visibility or stop sessions as an organizing action.

## Verification

See [the native and protocol record](../verification/2026-10-19-model-options.md).
Synthetic process checks cover 71 visible models over two pages, distinct picker
and wire identities, future efforts, rejected options and provider failures.
Isolated real Codex emits the chosen model/effort into a loopback HTTP request.
Native checks cover keyboard menus, state retention, removed capabilities,
failure recovery, parameter readback and a 900 × 640 window. Authenticated model
access, billing, external LLM output and non-Linux GUI behavior are not accepted.
