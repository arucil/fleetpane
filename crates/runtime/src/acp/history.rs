//! Paging over Host-observed ACP activity; never reads DSH's private transcript.
use super::{Adapter, Batch, reply, string};
use crate::MAX_PROVIDER_FRAME;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    generation: Uuid,
    session: String,
    turn: Option<String>,
    before: String,
}

fn metadata(turn: &Value) -> Result<Value, String> {
    let fields = turn
        .as_object()
        .ok_or("ACP turn metadata is not an object")?;
    let mut result: serde_json::Map<String, Value> = fields
        .iter()
        .filter(|(key, _)| key.as_str() != "items")
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    result.insert("items".into(), json!([]));
    Ok(Value::Object(result))
}

pub(super) fn flag(params: &Value, key: &str, default: bool) -> Result<bool, String> {
    match params.get(key) {
        Some(value) => value
            .as_bool()
            .ok_or_else(|| format!("ACP {key} must be boolean")),
        None => Ok(default),
    }
}

impl Adapter {
    pub(super) fn read_observed(
        &self,
        host: &str,
        id: &str,
        include_turns: bool,
        batch: &mut Batch,
    ) -> Result<(), String> {
        let session = self
            .sessions
            .get(id)
            .ok_or("Session is inactive; resume it explicitly. ACP does not replay old history.")?;
        let message = reply(host, json!({"thread":session.thread(include_turns)}));
        if serde_json::to_vec(&message)
            .map_err(|e| e.to_string())?
            .len()
            + 1
            > MAX_PROVIDER_FRAME
        {
            return Err("ACP full history exceeds the frame limit; use metadata-only inspect and paged history commands".into());
        }
        batch.events.push(message);
        Ok(())
    }

    pub(super) fn history_page(
        &self,
        host: &str,
        method: &str,
        params: &Value,
        batch: &mut Batch,
    ) -> Result<(), String> {
        let id = string(params, "threadId")?;
        let session = self
            .sessions
            .get(id)
            .ok_or("Resume the ACP session before browsing Host-observed history")?;
        let turn = if method == "thread/items/list" {
            Some(string(params, "turnId")?)
        } else {
            None
        };
        if params
            .get("sortDirection")
            .is_some_and(|value| value != "desc")
        {
            return Err("ACP observed history supports newest-first pages only".into());
        }
        if turn.is_none()
            && params
                .get("itemsView")
                .is_some_and(|value| value != "notLoaded")
        {
            return Err("ACP turn pages contain metadata only".into());
        }
        let maximum = if turn.is_some() { 16 } else { 1 };
        let limit = match params.get("limit") {
            None => maximum,
            Some(value) => {
                let limit = value
                    .as_u64()
                    .ok_or("ACP history page limit must be an integer")?;
                usize::try_from(limit).map_err(|e| e.to_string())?
            }
        };
        if limit == 0 || limit > maximum {
            return Err(format!(
                "ACP history page limit must be between 1 and {maximum}"
            ));
        }
        let cursor = params
            .get("cursor")
            .filter(|value| !value.is_null())
            .map(|_| {
                let text = string(params, "cursor")?;
                if text.len() > 4096 {
                    return Err("ACP history cursor exceeds 4 KiB".into());
                }
                serde_json::from_str::<Cursor>(text)
                    .map_err(|e| format!("Invalid ACP history cursor: {e}"))
            })
            .transpose()?;
        let values = match turn {
            Some(turn) => session
                .turns
                .iter()
                .find(|value| value["id"] == turn)
                .ok_or("ACP history turn is not retained")?["items"]
                .as_array()
                .ok_or("ACP turn items are not an array")?,
            None => &session.turns,
        };
        let end = if let Some(cursor) = cursor {
            if cursor.generation != self.directory_generation
                || cursor.session != id
                || cursor.turn.as_deref() != turn
            {
                return Err(
                    "ACP history cursor belongs to another Host, session or turn scope".into(),
                );
            }
            values
                .iter()
                .position(|value| value["id"] == cursor.before)
                .ok_or("ACP history cursor anchor is not retained")?
        } else {
            values.len()
        };
        let mut data = Vec::new();
        let mut message = reply(host, json!({"data":data,"nextCursor":null}));
        for index in (0..end).rev().take(limit) {
            let value = &values[index];
            let entry = match turn {
                Some(turn) => json!({"turnId":turn,"item":value}),
                None => metadata(value)?,
            };
            data.push(entry);
            let next = if index == 0 {
                None
            } else {
                Some(
                    serde_json::to_string(&Cursor {
                        generation: self.directory_generation,
                        session: id.into(),
                        turn: turn.map(str::to_owned),
                        before: string(value, "id")?.into(),
                    })
                    .map_err(|e| e.to_string())?,
                )
            };
            let candidate = reply(host, json!({"data":data,"nextCursor":next}));
            // Count the exact encoded reply, including its identity and continuation.
            // A byte-limited page may contain fewer than the requested item count.
            if serde_json::to_vec(&candidate)
                .map_err(|e| e.to_string())?
                .len()
                + 1
                > MAX_PROVIDER_FRAME
            {
                if data.len() == 1 {
                    return Err(
                        "ACP history entry exceeds the frame limit; it cannot fit in a paged reply"
                            .into(),
                    );
                }
                break;
            }
            message = candidate;
        }
        batch.events.push(message);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::{Session, Summary};

    fn adapter() -> Adapter {
        let mut adapter = Adapter::default();
        let items: Vec<_> = (0..12).map(|i| json!({"id":format!("item-{i}"),"type":"acpMessage","content":[{"type":"text","text":"汉字\\\"\n".repeat(5000)}]})).collect();
        adapter.sessions.insert(
            "s".into(),
            Session {
                modes: None,
                commands: None,
                mode_update_version: 0,
                summary: Summary {
                    session_id: "s".into(),
                    cwd: "/synthetic".into(),
                    title: None,
                    info: super::super::SessionInfo::default(),
                },
                config: vec![],
                active: None,
                resident: true,
                anonymous_stream: None,
                cancelling: false,
                turns: vec![json!({"id":"t","status":"completed","error":null,"items":items})],
            },
        );
        adapter
    }
    fn command(adapter: &mut Adapter, method: &str, params: Value) -> Value {
        let batch = adapter
            .dispatch("query:test".into(), method, params)
            .unwrap();
        assert!(batch.wire.is_empty());
        assert!(serde_json::to_vec(&batch.events[0]).unwrap().len() < MAX_PROVIDER_FRAME);
        batch.events[0].clone()
    }

    #[test]
    fn encoded_byte_pages_keep_every_entry_and_anchor_across_append() {
        let mut adapter = adapter();
        let metadata = command(
            &mut adapter,
            "thread/read",
            json!({"threadId":"s","includeTurns":false}),
        );
        assert_eq!(metadata["result"]["thread"]["turns"], json!([]));
        let mut result = command(
            &mut adapter,
            "thread/items/list",
            json!({"threadId":"s","turnId":"t","limit":16}),
        )["result"]
            .clone();
        assert!(result["data"].as_array().unwrap().len() < 12);
        let mut ids = Vec::new();
        adapter.sessions.get_mut("s").unwrap().turns[0]["items"].as_array_mut().unwrap().push(json!({"id":"later","type":"acpMessage","content":[{"type":"text","text":"later"}]}));
        loop {
            ids.extend(
                result["data"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| r["item"]["id"].as_str().unwrap().to_owned()),
            );
            if result["nextCursor"].is_null() {
                break;
            }
            result = command(
                &mut adapter,
                "thread/items/list",
                json!({"threadId":"s","turnId":"t","cursor":result["nextCursor"]}),
            )["result"]
                .clone();
        }
        assert_eq!(
            ids,
            (0..12)
                .rev()
                .map(|i| format!("item-{i}"))
                .collect::<Vec<_>>()
        );
        let full = command(
            &mut adapter,
            "thread/read",
            json!({"threadId":"s","includeTurns":true}),
        );
        assert!(full["error"]["message"].as_str().unwrap().contains("paged"));
    }

    #[test]
    fn invalid_scopes_flags_and_limits_are_rejected_without_provider_io() {
        let mut adapter = adapter();
        let page = command(
            &mut adapter,
            "thread/items/list",
            json!({"threadId":"s","turnId":"t"}),
        );
        let cursor = page["result"]["nextCursor"].clone();
        for (method, params) in [
            ("thread/turns/list", json!({"threadId":"s","cursor":cursor})),
            (
                "thread/items/list",
                json!({"threadId":"s","turnId":"t","limit":17}),
            ),
            (
                "thread/items/list",
                json!({"threadId":"s","turnId":"t","limit":0}),
            ),
            (
                "thread/items/list",
                json!({"threadId":"s","turnId":"t","sortDirection":"asc"}),
            ),
            (
                "thread/read",
                json!({"threadId":"s","includeTurns":"false"}),
            ),
            ("thread/resume", json!({"threadId":"s","excludeTurns":null})),
        ] {
            assert!(command(&mut adapter, method, params).get("error").is_some());
        }
        let stale = command(
            &mut self::adapter(),
            "thread/items/list",
            json!({"threadId":"s","turnId":"t","cursor":cursor}),
        );
        assert!(stale.get("error").is_some());
        assert!(
            command(
                &mut adapter,
                "thread/read",
                json!({"threadId":"s","includeTurns":false})
            )
            .get("result")
            .is_some()
        );
    }
}
