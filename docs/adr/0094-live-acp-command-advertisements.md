# Live ACP command advertisements

Accepted 2026-10-22.

Paseo history 626/627/677 exposed command discovery as a useful workflow.
Fleetpane uses the current provider protocol rather than parsing CLI help or
recreating Claude stdout tags. ACP v1 advertises commands through
[`available_commands_update`](https://agentclientprotocol.com/protocol/v1/slash-commands).
An update replaces the complete list and can arrive while idle or waiting for a
permission; it must not create a conversation item or imply a new prompt.

The Host validates typed names, descriptions, optional input hints and opaque
extensions before replacing session metadata. Limits are 128 commands and
64 KiB per list, included in existing session/Host memory bounds. Unknown and
explicitly empty lists differ. Availability belongs to the resident provider
session: archives omit it and resume must receive a new advertisement. Control
window replacement reads the current list from the unchanged Host.

The native menu resolves an exact command against the selected session's current
advertisement at click time. It fills an empty draft or replaces a lone slash
prefix; existing prose or arguments require explicit editing. Images remain on
the existing draft. Selection does not send; ordinary Send owns prompt dispatch,
permissions and receipts. Full command JSON is copyable without normalizing
Unicode, extension fields or hints. Metadata updates do not reconstruct content
rows. The menu shares the existing scrollable conversation options region.

Codex commands, command execution semantics, fuzzy completion and persistent
command caches are separate work. A protocol-capable synthetic provider proves
this contract; it does not prove that every installed DeepSeek version advertises
commands. [Local evidence](../verification/2026-10-22-acp-commands.md).
