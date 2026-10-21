# 0076: Recover observational reads without replaying actions

Date: 2026-10-22. Status: accepted.

The native Host view previously used `poll_paused` for both transient Unix
transport loss and invalid history. One failed snapshot stopped event polling
permanently, even when the same Host became reachable again. A synthetic proxy
reproduced the failure in the existing binary while its provider continued.

Keep history pauses separate from failed reads. Retry a transport-only failure
after two seconds using the original event cursor and lease. Window activation
wakes the next observational read. Never resend actions, answers or a provider
resume; operation uncertainty retains its existing explicit receipt workflow.
Keep connection errors separate from action errors so a successful read does not
erase an unsuccessful or uncertain action.

While reads are unavailable, retain content with an explicit status, disable
Host actions and allow an already loaded local draft to be edited and saved.
Successful replay clears only the connection failure. Framing/JSON/protocol and
expired-cursor failures require Reconnect to reconcile the selected history;
observational snapshots still refresh pending requests and ownership. Invalid
JSON and malformed framing are protocol failures, rather than transient transport
failures for observational requests. Mutating admissions and permission answers
retain their conservative uncertain-delivery classification. The wire format and protocol version do not change.

Adopting a snapshot that revokes the current lease discards the local connection.
Seeing the previous Host or controller again cannot restore that discarded
authority. Retained history stays paused until explicit takeover and reload.
This is read recovery, not a Host update or provider-process migration solution.

Verification and limits: [read recovery](../verification/2026-10-22-read-recovery.md).
