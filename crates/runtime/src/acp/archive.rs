//! Durable observed activity, independent of provider process residency.
use super::{Adapter, Batch, Session, string};
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub kind: String,
    pub session: String,
    pub parent: String,
    pub id: String,
    pub data: Value,
}

impl Adapter {
    /// Save changed metadata, turn metadata and individual items. Never rewrite
    /// every past turn for a streaming chunk. Pending RPCs/permissions stay ephemeral.
    pub(super) fn archive_batch(&mut self, batch: &mut Batch) -> Result<(), String> {
        for id in std::mem::take(&mut self.dirty) {
            let session = self
                .sessions
                .get(&id)
                .ok_or("archive session disappeared")?;
            batch.archive.push(Row {
                kind: "session".into(), session: id.clone(), parent: String::new(), id,
                data: json!({"summary":session.summary,"config":session.config,"modes":session.modes,"turns":[],"active":null}),
            });
        }
        for event in &batch.events {
            let kind = event["method"].as_str();
            let params = &event["params"];
            let (kind, parent, data) = match kind {
                Some("turn/started" | "turn/completed") => ("turn", String::new(), &params["turn"]),
                Some("item/completed") => (
                    "item",
                    string(params, "turnId")?.to_owned(),
                    &params["item"],
                ),
                _ => continue,
            };
            let mut data = data.clone();
            if kind == "turn" {
                data["items"] = json!([]);
            }
            batch.archive.push(Row {
                kind: kind.into(),
                session: string(params, "threadId")?.to_owned(),
                parent,
                id: string(&data, "id")?.to_owned(),
                data,
            });
        }
        Ok(())
    }

    pub(crate) fn restore(rows: Vec<Row>) -> Result<Self, String> {
        let mut adapter = Self::default();
        for row in rows {
            match row.kind.as_str() {
                "session" => {
                    let mut session: Session = serde_json::from_value(row.data)
                        .map_err(|e| format!("invalid ACP archive session: {e}"))?;
                    if row.id != row.session
                        || !row.parent.is_empty()
                        || session.summary.session_id != row.id
                        || row.id.is_empty()
                        || !std::path::Path::new(&session.summary.cwd).is_absolute()
                        || !session.turns.is_empty()
                        || session.active.is_some()
                        || adapter.sessions.contains_key(&row.id)
                    {
                        return Err("invalid ACP archive session identity or metadata".into());
                    }
                    session.resident = false;
                    adapter
                        .summaries
                        .insert(row.id.clone(), session.summary.clone());
                    adapter.sessions.insert(row.id, session);
                }
                "turn" => {
                    let session = adapter
                        .sessions
                        .get_mut(&row.session)
                        .ok_or("orphan ACP archive turn")?;
                    if !row.parent.is_empty()
                        || row.id.is_empty()
                        || string(&row.data, "id")? != row.id
                        || row.data["items"] != json!([])
                        || session.turns.iter().any(|t| t["id"] == row.id)
                    {
                        return Err("invalid ACP archive turn identity or items".into());
                    }
                    let mut turn = row.data;
                    match string(&turn, "status")? {
                        "inProgress" => {
                            turn["status"] = json!("unknown");
                            turn["error"] = json!({"message":"Host stopped before ACP prompt settlement. Inspect provider state before manually retrying; nothing was replayed."});
                        }
                        "completed" | "interrupted" | "failed" | "unknown" => {}
                        _ => return Err("invalid ACP archive turn status".into()),
                    }
                    session.turns.push(turn);
                }
                "item" => {
                    let session = adapter
                        .sessions
                        .get_mut(&row.session)
                        .ok_or("orphan ACP archive item session")?;
                    let turn = session
                        .turns
                        .iter_mut()
                        .find(|t| t["id"] == row.parent)
                        .ok_or("orphan ACP archive item turn")?;
                    let items = turn["items"]
                        .as_array_mut()
                        .ok_or("invalid ACP archive turn items")?;
                    if row.id.is_empty()
                        || string(&row.data, "id")? != row.id
                        || items.iter().any(|item| item["id"] == row.id)
                    {
                        return Err("invalid ACP archive item identity".into());
                    }
                    string(&row.data, "type")?;
                    items.push(row.data);
                }
                _ => return Err("unknown ACP archive row kind".into()),
            }
        }
        adapter.validate()?;
        Ok(adapter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn create(adapter: &mut Adapter, id: &str) -> Batch {
        let request = adapter
            .dispatch("create".into(), "thread/start", json!({"cwd":"/synthetic"}))
            .unwrap();
        adapter.receive(json!({"jsonrpc":"2.0","id":request.wire[0]["id"],"result":{"sessionId":id,"configOptions":[]}})).unwrap()
    }
    fn update(adapter: &mut Adapter, text: &str) -> Batch {
        adapter.receive(json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":text}}}})).unwrap()
    }
    type Rows = BTreeMap<(String, String, String, String), Row>;
    fn save(rows: &mut Rows, batch: Batch) {
        for row in batch.archive {
            rows.insert(
                (
                    row.kind.clone(),
                    row.session.clone(),
                    row.parent.clone(),
                    row.id.clone(),
                ),
                row,
            );
        }
    }
    fn restore(rows: Rows) -> Adapter {
        let mut rows: Vec<_> = rows.into_values().collect();
        rows.sort_by_key(|row| match row.kind.as_str() {
            "session" => 0,
            "turn" => 1,
            _ => 2,
        });
        Adapter::restore(rows).unwrap()
    }
    #[test]
    fn modes_survive_restart_and_resume_replaces_the_catalog() {
        let mut adapter = Adapter::default();
        let mut rows = Rows::new();
        save(&mut rows, create(&mut adapter, "s"));
        let legacy = rows.clone();
        save(&mut rows, adapter.receive(json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"current_mode_update","currentModeId":"provider-mode"}}})).unwrap());
        let mut recovered = restore(rows.clone());
        assert_eq!(
            recovered.sessions["s"].thread(true)["acpModes"]["currentModeId"],
            "provider-mode"
        );
        let request = recovered
            .dispatch("resume".into(), "thread/resume", json!({"threadId":"s"}))
            .unwrap();
        let modes =
            json!({"currentModeId":"fresh","availableModes":[{"id":"fresh","name":"Fresh"}]});
        recovered
            .receive(json!({"jsonrpc":"2.0","id":request.wire[0]["id"],"result":{"modes":modes}}))
            .unwrap();
        assert_eq!(recovered.sessions["s"].thread(true)["acpModes"], modes);
        let mut legacy = legacy;
        for row in legacy.values_mut() {
            row.data.as_object_mut().unwrap().remove("modes");
        }
        assert!(restore(legacy).sessions["s"].modes.is_none());
        for row in rows.values_mut() {
            row.data["modes"]["currentModeId"] = json!("");
        }
        assert!(Adapter::restore(rows.into_values().collect()).is_err());
    }

    #[test]
    fn idle_info_is_archived_without_turns_and_old_rows_remain_readable() {
        let mut adapter = Adapter::default();
        let mut rows = Rows::new();
        save(&mut rows, create(&mut adapter, "s"));
        let legacy = rows.clone();
        let metadata = json!({"title":"Idle 中文","updatedAt":"2026-10-21T01:00:00Z","_meta":{"vendor":[1,null]}});
        let mut update = metadata.clone();
        update["sessionUpdate"] = json!("session_info_update");
        save(
            &mut rows,
            adapter
                .receive(
                    json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":update}}),
                )
                .unwrap(),
        );
        let recovered = restore(rows);
        let thread = recovered.sessions["s"].thread(true);
        assert_eq!(thread["name"], metadata["title"]);
        assert_eq!(thread["acpSessionInfo"]["updatedAt"], metadata["updatedAt"]);
        assert_eq!(thread["acpSessionInfo"]["_meta"], metadata["_meta"]);
        assert_eq!(thread["turns"], json!([]));
        assert!(!recovered.sessions["s"].resident);
        let mut legacy = legacy;
        for row in legacy.values_mut() {
            row.data["summary"]
                .as_object_mut()
                .unwrap()
                .remove("updatedAt");
            row.data["summary"].as_object_mut().unwrap().remove("_meta");
        }
        assert!(
            restore(legacy).sessions["s"]
                .summary
                .info
                .updated_at
                .is_none()
        );
    }
    #[test]
    fn restart_keeps_observed_items_but_never_restores_residency_or_pending_permissions() {
        let mut adapter = Adapter::default();
        let mut rows = Rows::new();
        save(&mut rows, create(&mut adapter, "s"));
        let start = adapter
            .dispatch(
                "send".into(),
                "turn/start",
                json!({"threadId":"s","input":[{"type":"text","text":"user 中文"}]}),
            )
            .unwrap();
        let pending_id = start.wire[0]["id"].clone();
        save(&mut rows, start);
        save(&mut rows, update(&mut adapter, "first "));
        save(&mut rows, update(&mut adapter, "中文"));
        let pending=adapter.receive(json!({"jsonrpc":"2.0","id":42,"method":"session/request_permission","params":{"sessionId":"s","options":[{"optionId":"allow","name":"Allow","kind":"allow_once"}],"toolCall":{"toolCallId":"call"}}})).unwrap();
        assert_eq!(pending.events[0]["id"], 42);
        assert!(pending.archive.is_empty());
        let mut recovered = restore(rows.clone());
        assert!(recovered.pending.is_empty() && recovered.permissions.is_empty());
        let turn = &recovered.sessions["s"].turns[0];
        assert_eq!(turn["status"], "unknown");
        assert!(
            turn["error"]["message"]
                .as_str()
                .unwrap()
                .contains("nothing was replayed")
        );
        let assistant = turn["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["type"] == "acpMessage")
            .unwrap();
        assert_eq!(assistant["content"][0]["text"], "first 中文");
        let denied = recovered
            .dispatch(
                "send2".into(),
                "turn/start",
                json!({"threadId":"s","input":[{"type":"text","text":"must not run"}]}),
            )
            .unwrap();
        assert!(denied.wire.is_empty());
        assert!(
            denied.events[0]["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Resume")
        );
        let resume = recovered
            .dispatch(
                "resume".into(),
                "thread/resume",
                json!({"threadId":"s","excludeTurns":true}),
            )
            .unwrap();
        assert_eq!(resume.wire[0]["method"], "session/resume");
        let ready = recovered
            .receive(
                json!({"jsonrpc":"2.0","id":resume.wire[0]["id"],"result":{"configOptions":[]}}),
            )
            .unwrap();
        assert_eq!(
            ready.events[0]["result"]["thread"]["status"]["type"],
            "idle"
        );
        assert_eq!(ready.events[0]["result"]["thread"]["turns"], json!([]));
        assert_eq!(recovered.sessions["s"].turns.len(), 1);
        assert_eq!(recovered.sessions["s"].turns[0]["status"], "unknown");
        // A settled turn retains its provider-confirmed result across the same restore path.
        let ended = adapter
            .receive(json!({"jsonrpc":"2.0","id":pending_id,"result":{"stopReason":"end_turn"}}))
            .unwrap();
        save(&mut rows, ended);
        assert_eq!(restore(rows).sessions["s"].turns[0]["status"], "completed");
    }

    #[test]
    fn failed_resume_keeps_archive_and_large_resume_uses_the_existing_frame_boundary() {
        let mut adapter = Adapter::default();
        let mut rows = Rows::new();
        save(&mut rows, create(&mut adapter, "s"));
        save(
            &mut rows,
            adapter
                .dispatch(
                    "send".into(),
                    "turn/start",
                    json!({"threadId":"s","input":[{"type":"text","text":"one"}]}),
                )
                .unwrap(),
        );
        save(&mut rows, update(&mut adapter, &"large 中文".repeat(30000)));
        let mut adapter = restore(rows);
        let request = adapter
            .dispatch("resume".into(), "thread/resume", json!({"threadId":"s"}))
            .unwrap();
        let duplicate = adapter
            .dispatch("again".into(), "thread/resume", json!({"threadId":"s"}))
            .unwrap();
        assert!(duplicate.wire.is_empty() && duplicate.events[0].get("error").is_some());
        let failed=adapter.receive(json!({"jsonrpc":"2.0","id":request.wire[0]["id"],"error":{"code":-32603,"message":"synthetic refusal"}})).unwrap();
        assert!(failed.events[0].get("error").is_some());
        assert!(!adapter.sessions["s"].resident);
        let request = adapter
            .dispatch("resume".into(), "thread/resume", json!({"threadId":"s"}))
            .unwrap();
        let ready = adapter
            .receive(
                json!({"jsonrpc":"2.0","id":request.wire[0]["id"],"result":{"configOptions":[]}}),
            )
            .unwrap();
        assert!(adapter.sessions["s"].resident);
        assert!(
            ready.events[0]["error"]["message"]
                .as_str()
                .unwrap()
                .contains("frame limit")
        );
        let metadata = adapter
            .dispatch(
                "read".into(),
                "thread/read",
                json!({"threadId":"s","includeTurns":false}),
            )
            .unwrap();
        assert_eq!(metadata.events[0]["result"]["thread"]["turns"], json!([]));
        assert_eq!(
            adapter.sessions["s"].turns[0]["items"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn chunk_writes_only_the_updated_item_and_archive_identity_damage_is_an_error() {
        let mut adapter = Adapter::default();
        let mut rows = Rows::new();
        save(&mut rows, create(&mut adapter, "s"));
        save(
            &mut rows,
            adapter
                .dispatch(
                    "send".into(),
                    "turn/start",
                    json!({"threadId":"s","input":[{"type":"text","text":"one"}]}),
                )
                .unwrap(),
        );
        let update = update(&mut adapter, "text");
        assert_eq!(update.archive.len(), 1);
        assert_eq!(update.archive[0].kind, "item");
        save(&mut rows, update);
        let mut missing = rows.clone();
        missing.retain(|(kind, _, _, _), _| kind != "session");
        let mut missing: Vec<_> = missing.into_values().collect();
        missing.sort_by_key(|row| if row.kind == "turn" { 0 } else { 1 });
        assert!(Adapter::restore(missing).is_err());
        let mut damaged: Vec<_> = rows.into_values().collect();
        damaged.sort_by_key(|row| match row.kind.as_str() {
            "session" => 0,
            "turn" => 1,
            _ => 2,
        });
        damaged[0].data["summary"]["sessionId"] = json!("other");
        assert!(Adapter::restore(damaged).is_err());
    }
}
