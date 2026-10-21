//! ACP v1 adapter. Host projections are independent of DSH's private storage.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

#[cfg(unix)]
pub(crate) mod archive;
mod history;
pub mod modes;
pub mod tool_calls;

#[derive(Default)]
pub struct Batch {
    pub wire: Vec<Value>,
    pub events: Vec<Value>,
    #[cfg(unix)]
    pub(crate) archive: Vec<archive::Row>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    session_id: String,
    cwd: String,
    title: Option<String>,
    #[serde(flatten)]
    info: SessionInfo,
}

/// Provider-owned ACP metadata. Timestamp strings and extension values remain opaque.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub updated_at: Option<String>,
    #[serde(rename = "_meta")]
    pub meta: Option<serde_json::Map<String, Value>>,
}

fn patch_field<T: serde::de::DeserializeOwned>(
    update: &Value,
    key: &str,
    old: &mut T,
) -> Result<(), String> {
    if let Some(value) = update.get(key) {
        *old = serde_json::from_value(value.clone())
            .map_err(|error| format!("invalid ACP session info {key}: {error}"))?;
    }
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Session {
    summary: Summary,
    config: Vec<Value>,
    #[serde(default)]
    modes: Option<modes::State>,
    #[serde(skip)]
    mode_update_version: u64,
    turns: Vec<Value>,
    active: Option<String>,
    #[serde(skip, default = "resident")]
    resident: bool,
    /// Only adjacent unidentified chunks belong to the same display segment.
    /// Ephemeral: archived turns never accept further chunks after restoration.
    #[serde(skip)]
    anonymous_stream: Option<(String, usize)>,
    /// Cancellation is an in-process request, never evidence of prompt settlement.
    #[serde(skip)]
    cancelling: bool,
}

fn resident() -> bool {
    true
}

impl Session {
    fn thread(&self, include_turns: bool) -> Value {
        json!({"id":self.summary.session_id,"cwd":self.summary.cwd,"name":self.summary.title,
            "acpSessionInfo":self.summary.info,
            "preview":"DeepSeek Harness", "historyMode":"acp-observed", "historyPaging":true, "configOptions":self.config, "acpModes":self.modes,
            "status":{"type":if !self.resident {"archived"} else if self.active.is_some() {"active"} else {"idle"},"cancelRequested":self.cancelling},
            "turns":if include_turns {self.turns.clone()} else {vec![]}})
    }
}

enum Pending {
    New {
        host: String,
        cwd: String,
    },
    List {
        host: String,
        cwd: Option<String>,
        first: bool,
    },
    Resume {
        host: String,
        id: String,
        include_turns: bool,
    },
    Prompt {
        id: String,
        turn: String,
    },
    Config {
        host: String,
        id: String,
    },
    Mode {
        host: String,
        id: String,
        requested: String,
        revision: u64,
    },
}

pub struct Adapter {
    sessions: BTreeMap<String, Session>,
    summaries: BTreeMap<String, Summary>,
    pending: BTreeMap<String, Pending>,
    permissions: BTreeMap<String, Value>,
    directory_generation: Uuid,
    #[cfg(unix)]
    dirty: std::collections::BTreeSet<String>,
}

impl Default for Adapter {
    fn default() -> Self {
        Self {
            sessions: BTreeMap::new(),
            summaries: BTreeMap::new(),
            pending: BTreeMap::new(),
            permissions: BTreeMap::new(),
            directory_generation: Uuid::new_v4(),
            #[cfg(unix)]
            dirty: Default::default(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct DirectoryCursor {
    generation: Uuid,
    cwd: Option<String>,
    remainder: Vec<String>,
    provider: Option<String>,
}

fn notification(method: &str, params: Value) -> Value {
    json!({"method":method,"params":params})
}
fn reply(id: &str, result: Value) -> Value {
    json!({"id":id,"result":result})
}
fn cancelled_permission(id: &Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":{"outcome":{"outcome":"cancelled"}}})
}
fn rejected(id: &str, message: impl ToString) -> Value {
    json!({"id":id,"error":{"code":-32602,"message":message.to_string()}})
}
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("ACP {key} must contain a string"))
}
fn config(result: &Value) -> Result<Vec<Value>, String> {
    serde_json::from_value(
        result
            .get("configOptions")
            .cloned()
            .unwrap_or_else(|| json!([])),
    )
    .map_err(|e| format!("invalid ACP config options: {e}"))
}

impl Adapter {
    fn rpc(&mut self, method: &str, params: Value, pending: Pending, batch: &mut Batch) {
        let id = format!("acp:{}", Uuid::new_v4());
        self.pending.insert(id.clone(), pending);
        batch
            .wire
            .push(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
    }

    /// Input already has a fenced Host identity. Unsupported commands are explicit errors.
    pub fn dispatch(&mut self, host: String, method: &str, params: Value) -> Result<Batch, String> {
        self.dispatch_inner(host, method, params, None)
    }

    #[cfg(unix)]
    pub(crate) fn dispatch_images(
        &mut self,
        host: String,
        params: Value,
        prompt: &crate::attachments::Prompt,
    ) -> Result<Batch, String> {
        self.dispatch_inner(host, "turn/start", params, Some(prompt))
    }

    fn dispatch_inner(
        &mut self,
        host: String,
        method: &str,
        params: Value,
        prompt: Option<&crate::attachments::Prompt>,
    ) -> Result<Batch, String> {
        let mut batch = Batch::default();
        if let Err(error) = self.command(&host, method, &params, &mut batch, prompt) {
            batch.events.push(rejected(&host, error));
        }
        self.validate()?;
        #[cfg(unix)]
        self.archive_batch(&mut batch)?;
        Ok(batch)
    }

    fn command(
        &mut self,
        host: &str,
        method: &str,
        params: &Value,
        batch: &mut Batch,
        prompt: Option<&crate::attachments::Prompt>,
    ) -> Result<(), String> {
        match method {
            "thread/start" => {
                let cwd = string(params, "cwd")?.to_owned();
                if !std::path::Path::new(&cwd).is_absolute() {
                    return Err("ACP workspace must be absolute".into());
                }
                self.rpc(
                    "session/new",
                    json!({"cwd":cwd,"mcpServers":[]}),
                    Pending::New {
                        host: host.into(),
                        cwd,
                    },
                    batch,
                );
            }
            "thread/list" => {
                if history::flag(params, "archived", false)? {
                    return Err("DeepSeek ACP does not advertise provider archive listing".into());
                }

                if params
                    .get("searchTerm")
                    .is_some_and(|v| !v.is_null() && v != "")
                {
                    return Err("DeepSeek ACP does not support server-side title filtering".into());
                }
                let cwd = params
                    .get("cwd")
                    .filter(|v| !v.is_null())
                    .map(|_| string(params, "cwd").map(str::to_owned))
                    .transpose()?;
                let cursor = params
                    .get("cursor")
                    .filter(|v| !v.is_null())
                    .map(|_| {
                        serde_json::from_str::<DirectoryCursor>(string(params, "cursor")?)
                            .map_err(|e| format!("Invalid ACP directory cursor: {e}"))
                    })
                    .transpose()?;
                let first = cursor.is_none();
                let mut arguments = json!({"cwd":cwd});
                if let Some(cursor) = cursor {
                    if cursor.generation != self.directory_generation
                        || cursor.cwd != cwd
                        || cursor.remainder.len() > 100
                    {
                        return Err(
                            "ACP directory cursor belongs to another Host or workspace query"
                                .into(),
                        );
                    }
                    if !cursor.remainder.is_empty() {
                        let rows = cursor
                            .remainder
                            .iter()
                            .map(|id| {
                                self.summaries.get(id).cloned().ok_or_else(|| {
                                    "ACP cursor session metadata is no longer retained".to_owned()
                                })
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        self.list_reply(host, cwd, rows, cursor.provider, batch)?;
                        return Ok(());
                    }
                    arguments["cursor"] = json!(
                        cursor
                            .provider
                            .ok_or("ACP directory cursor has no continuation")?
                    );
                }
                self.rpc(
                    "session/list",
                    arguments,
                    Pending::List {
                        host: host.into(),
                        cwd,
                        first,
                    },
                    batch,
                );
            }
            "thread/read" => {
                let id = string(params, "threadId")?;
                self.read_observed(
                    host,
                    id,
                    history::flag(params, "includeTurns", false)?,
                    batch,
                )?;
            }
            "thread/turns/list" | "thread/items/list" => {
                self.history_page(host, method, params, batch)?;
            }
            "thread/resume" => {
                let id = string(params, "threadId")?;
                let include_turns = !history::flag(params, "excludeTurns", false)?;
                if self
                    .pending
                    .values()
                    .any(|pending| matches!(pending,Pending::Resume{id:current,..} if current==id))
                {
                    return Err("Wait for the pending ACP context resume to settle".into());
                }
                if self
                    .sessions
                    .get(id)
                    .is_some_and(|session| session.resident)
                {
                    self.read_observed(host, id, include_turns, batch)?;
                } else {
                    let summary = self
                        .summaries
                        .get(id)
                        .ok_or("List sessions before resuming this ACP session")?;
                    self.rpc(
                        "session/resume",
                        json!({"sessionId":id,"cwd":summary.cwd,"mcpServers":[]}),
                        Pending::Resume {
                            host: host.into(),
                            id: id.into(),
                            include_turns,
                        },
                        batch,
                    );
                }
            }
            "turn/start" => {
                let id = string(params, "threadId")?;
                self.configuration_settled(id)?;
                if params.get("model").is_some() || params.get("effort").is_some() {
                    return Err("Use ACP session configuration before sending; per-prompt Codex model overrides are unsupported".into());
                }
                #[derive(Deserialize)]
                struct Text {
                    #[serde(rename = "type")]
                    kind: String,
                    text: String,
                }
                let (wire, content) = if let Some(prompt) = prompt {
                    (prompt.wire.clone(), prompt.display.clone())
                } else {
                    let input: Vec<Text> = serde_json::from_value(
                        params.get("input").cloned().ok_or("prompt has no input")?,
                    )
                    .map_err(|e| e.to_string())?;
                    if input.is_empty()
                        || input
                            .iter()
                            .any(|t| t.kind != "text" || t.text.trim().is_empty())
                    {
                        return Err(
                            "Use text or verified Fleetpane image attachments for this ACP prompt"
                                .into(),
                        );
                    }
                    let content: Vec<Value> = input
                        .iter()
                        .map(|t| json!({"type":"text","text":t.text}))
                        .collect();
                    (content.clone(), content)
                };
                let session = self
                    .sessions
                    .get_mut(id)
                    .ok_or("Resume the ACP session before sending")?;
                if !session.resident {
                    return Err("Resume the ACP session before sending".into());
                }
                if session.active.is_some() {
                    return Err("ACP session already has an active prompt".into());
                }
                let turn_id = Uuid::new_v4().to_string();
                let turn = json!({"id":turn_id,"status":"inProgress","items":[],"error":null});
                session.turns.push(turn.clone());
                session.active = Some(turn_id.clone());
                session.cancelling = false;
                session.anonymous_stream = None;
                batch.events.push(notification(
                    "thread/status/changed",
                    json!({"threadId":id,"status":{"type":"active"}}),
                ));
                batch.events.push(notification(
                    "turn/started",
                    json!({"threadId":id,"turn":turn}),
                ));
                let item =
                    json!({"id":format!("user:{turn_id}"),"type":"userMessage","content":content});
                session.turns.last_mut().expect("created turn")["items"]
                    .as_array_mut()
                    .expect("items array")
                    .push(item.clone());
                batch.events.push(notification(
                    "item/completed",
                    json!({"threadId":id,"turnId":turn_id,"item":item}),
                ));
                self.rpc(
                    "session/prompt",
                    json!({"sessionId":id,"prompt":wire}),
                    Pending::Prompt {
                        id: id.into(),
                        turn: turn_id,
                    },
                    batch,
                );
                // This is a Host dispatch receipt; ACP confirms prompt outcome only at settlement.
                batch.events.push(reply(
                    host,
                    json!({"turn":turn,"admission":"host-dispatched"}),
                ));
            }
            "turn/interrupt" => {
                let id = string(params, "threadId")?;
                let session = self
                    .sessions
                    .get_mut(id)
                    .ok_or("ACP session is not resident")?;
                if !session.resident {
                    return Err("Resume the ACP session before cancellation".into());
                }
                if session.active.as_deref() != Some(string(params, "turnId")?) {
                    return Err("Stop targets a different or finished ACP prompt".into());
                }
                if !session.cancelling {
                    session.cancelling = true;
                    batch.wire.push(
                        json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":id}}),
                    );
                    // ACP requires answers now; waiting for prompt settlement can deadlock
                    // a provider that is itself waiting for these permission responses.
                    let requests: Vec<_> = self
                        .permissions
                        .iter()
                        .filter(|(_, p)| p["params"]["threadId"] == id)
                        .map(|(key, p)| (key.clone(), p["id"].clone()))
                        .collect();
                    for (key, request_id) in requests {
                        batch.wire.push(cancelled_permission(&request_id));
                        self.permissions.remove(&key);
                        batch.events.push(notification(
                            "serverRequest/resolved",
                            json!({"threadId":id,"requestId":request_id}),
                        ));
                    }
                    batch.events.push(notification(
                        "thread/status/changed",
                        json!({"threadId":id,"status":{"type":"active","cancelRequested":true}}),
                    ));
                }
                batch.events.push(reply(host, json!({})));
            }
            "acp/mode/set" => self.set_mode(host, params, batch)?,
            "acp/config/set" => {
                let id = string(params, "threadId")?;
                self.configuration_settled(id)?;
                let config_id = string(params, "configId")?;
                let value = string(params, "value")?;
                let session = self.sessions.get(id).ok_or("ACP session is not resident")?;
                if !session.resident {
                    return Err("Resume the ACP session before configuration changes".into());
                }
                if session.active.is_some() {
                    return Err("Change ACP configuration between prompts".into());
                }
                let option = session
                    .config
                    .iter()
                    .find(|o| o["id"] == config_id)
                    .ok_or("Unknown ACP configuration option")?;
                if !choice_exists(
                    option.get("options").ok_or("ACP option has no choices")?,
                    value,
                ) {
                    return Err("ACP configuration choice is no longer available".into());
                }
                self.rpc(
                    "session/set_config_option",
                    json!({"sessionId":id,"configId":config_id,"value":value}),
                    Pending::Config {
                        host: host.into(),
                        id: id.into(),
                    },
                    batch,
                );
            }
            _ => {
                return Err(format!(
                    "{method} is unsupported by the DeepSeek ACP adapter"
                ));
            }
        }
        Ok(())
    }

    fn configuration_settled(&self, id: &str) -> Result<(), String> {
        if self
            .pending
            .values()
            .any(|pending| matches!(pending, Pending::Config { id: current, .. } | Pending::Mode { id: current, .. } if current == id))
        {
            return Err("Wait for the pending ACP configuration change to settle".into());
        }
        Ok(())
    }

    /// Translate provider frames; malformed known data faults the runtime, never looks idle.
    pub fn receive(&mut self, message: Value) -> Result<Batch, String> {
        if message.get("jsonrpc") != Some(&json!("2.0")) {
            return Err("ACP frame lacks jsonrpc 2.0".into());
        }
        let mut batch = Batch::default();
        if let Some(method) = message.get("method") {
            let method = method.as_str().ok_or("ACP method must be a string")?;
            let params = message.get("params").ok_or("ACP method lacks params")?;
            match method {
                "session/update" => self.update(params, &mut batch)?,
                "session/request_permission" => {
                    let id = string(params, "sessionId")?;
                    let session = self
                        .sessions
                        .get(id)
                        .ok_or("Permission targets an unowned ACP session")?;
                    if session.active.is_none() {
                        return Err("ACP permission has no active prompt".into());
                    }
                    let request_id = message
                        .get("id")
                        .filter(|v| v.is_string() || v.is_i64())
                        .ok_or("Invalid ACP permission ID")?;
                    let options = params
                        .get("options")
                        .and_then(Value::as_array)
                        .ok_or("ACP permission has no options")?;
                    if options.is_empty() || options.len() > 16 {
                        return Err("ACP permission must offer 1 to 16 options".into());
                    }
                    let mut seen = std::collections::BTreeSet::new();
                    for option in options {
                        if !seen.insert(string(option, "optionId")?) {
                            return Err("ACP permission repeats an option ID".into());
                        }
                        string(option, "name")?;
                        string(option, "kind")?;
                    }
                    if self.permissions.contains_key(&request_id.to_string()) {
                        return Err("ACP reused a pending permission ID".into());
                    }
                    if session.cancelling {
                        batch.wire.push(cancelled_permission(request_id));
                    } else {
                        let mut pending = params.clone();
                        pending["threadId"] = json!(id);
                        let request = json!({"id":request_id,"method":method,"params":pending});
                        self.permissions
                            .insert(request_id.to_string(), request.clone());
                        batch.events.push(request);
                    }
                }
                _ if message.get("id").is_some() => {
                    batch.wire.push(json!({"jsonrpc":"2.0","id":message["id"],"error":{"code":-32601,"message":"Fleetpane does not provide this ACP client capability"}}));
                }
                _ => {} // Unadvertised extension notifications carry no known state transition.
            }
        } else {
            let wire = string(&message, "id")?;
            let pending = self
                .pending
                .remove(wire)
                .ok_or("ACP replied to an unrecorded request")?;
            if message.get("result").is_some() == message.get("error").is_some() {
                return Err("ACP response must contain result or error".into());
            }
            if let Pending::Prompt { id, turn } = pending {
                let error = message.get("error").cloned();
                let status = if error.is_some() {
                    "failed"
                } else {
                    match string(&message["result"], "stopReason")? {
                        "cancelled" => "interrupted",
                        "end_turn" | "max_tokens" | "max_turn_requests" | "refusal" => "completed",
                        other => return Err(format!("Unknown ACP stop reason: {other}")),
                    }
                };
                let session = self
                    .sessions
                    .get_mut(&id)
                    .ok_or("Prompt session disappeared")?;
                if session.active.as_deref() != Some(&turn) {
                    return Err("ACP prompt settlement identity changed".into());
                }
                let ended = session.turns.last_mut().ok_or("ACP prompt has no turn")?;
                ended["status"] = json!(status);
                ended["error"] = json!(error);
                ended["stopReason"] = message["result"]["stopReason"].clone();
                // Items have already been emitted individually. Repeating the entire turn
                // can exceed the frame limit even when each original update was bounded.
                let mut completion = ended.clone();
                completion["items"] = json!([]);
                batch.events.push(notification(
                    "turn/completed",
                    json!({"threadId":id,"turn":completion}),
                ));
                session.active = None;
                session.cancelling = false;
                batch.events.push(notification(
                    "thread/status/changed",
                    json!({"threadId":id,"status":{"type":"idle"}}),
                ));
                let resolved: Vec<_> = self
                    .permissions
                    .iter()
                    .filter(|(_, p)| p["params"]["threadId"] == id)
                    .map(|(k, p)| (k.clone(), p["id"].clone()))
                    .collect();
                for (key, request_id) in resolved {
                    self.permissions.remove(&key);
                    batch.events.push(notification(
                        "serverRequest/resolved",
                        json!({"threadId":id,"requestId":request_id}),
                    ));
                }
            } else {
                let host = match &pending {
                    Pending::New { host, .. }
                    | Pending::List { host, .. }
                    | Pending::Resume { host, .. }
                    | Pending::Config { host, .. }
                    | Pending::Mode { host, .. } => host,
                    Pending::Prompt { .. } => unreachable!(),
                }
                .clone();
                if let Some(error) = message.get("error") {
                    batch.events.push(json!({"id":host,"error":error}));
                } else {
                    self.response(pending, &message["result"], &mut batch)?;
                }
            }
        }
        self.validate()?;
        #[cfg(unix)]
        self.archive_batch(&mut batch)?;
        Ok(batch)
    }

    fn response(
        &mut self,
        pending: Pending,
        result: &Value,
        batch: &mut Batch,
    ) -> Result<(), String> {
        match pending {
            Pending::New { host, cwd } => {
                let id = string(result, "sessionId")?.to_owned();
                if self.sessions.contains_key(&id) {
                    return Err("ACP reused an existing session identity".into());
                }
                let summary = Summary {
                    session_id: id.clone(),
                    cwd,
                    title: None,
                    info: SessionInfo::default(),
                };
                let session = Session {
                    summary: summary.clone(),
                    config: config(result)?,
                    modes: modes::parse(result)?,
                    mode_update_version: 0,
                    turns: vec![],
                    active: None,
                    resident: true,
                    anonymous_stream: None,
                    cancelling: false,
                };
                batch
                    .events
                    .push(reply(&host, json!({"thread":session.thread(true)})));
                self.summaries.insert(id.clone(), summary);
                #[cfg(unix)]
                self.dirty.insert(id.clone());
                self.sessions.insert(id, session);
            }
            Pending::Resume {
                host,
                id,
                include_turns,
            } => {
                let summary = self
                    .summaries
                    .get(&id)
                    .ok_or("ACP resume summary disappeared")?
                    .clone();
                let turns = self
                    .sessions
                    .get(&id)
                    .map(|s| s.turns.clone())
                    .unwrap_or_default();
                let session = Session {
                    summary,
                    config: config(result)?,
                    modes: modes::parse(result)?,
                    mode_update_version: 0,
                    turns,
                    active: None,
                    resident: true,
                    anonymous_stream: None,
                    cancelling: false,
                };
                #[cfg(unix)]
                self.dirty.insert(id.clone());
                self.sessions.insert(id.clone(), session);
                if let Err(error) = self.read_observed(&host, &id, include_turns, batch) {
                    batch.events.push(rejected(&host, error));
                }
            }
            Pending::List { host, cwd, first } => {
                let persisted: Vec<Summary> = serde_json::from_value(
                    result
                        .get("sessions")
                        .cloned()
                        .ok_or("ACP list lacks sessions")?,
                )
                .map_err(|e| e.to_string())?;
                if persisted.len() > 100 {
                    return Err("ACP directory page exceeds 100 sessions".into());
                }
                let mut seen = std::collections::BTreeSet::new();
                // DSH lists inactive sessions only. Resident sessions must remain
                // discoverable after controller replacement, including before persistence.
                let mut rows: Vec<Summary> = if first {
                    self.sessions
                        .values()
                        .filter(|s| cwd.as_ref().is_none_or(|cwd| s.summary.cwd == *cwd))
                        .map(|s| s.summary.clone())
                        .collect()
                } else {
                    vec![]
                };
                for row in &rows {
                    seen.insert(row.session_id.clone());
                }
                for mut row in persisted {
                    if row.session_id.is_empty() {
                        return Err("ACP session identity is empty".into());
                    }
                    if !std::path::Path::new(&row.cwd).is_absolute() {
                        return Err("ACP session workspace is not absolute".into());
                    }
                    if let Some(session) = self.sessions.get_mut(&row.session_id) {
                        if session.summary.cwd != row.cwd {
                            return Err(
                                "ACP listed a different workspace for an observed session".into()
                            );
                        }
                        if session.resident {
                            // A list response may have been captured before a live update.
                            row = session.summary.clone();
                        } else {
                            session.summary = row.clone();
                            #[cfg(unix)]
                            self.dirty.insert(row.session_id.clone());
                        }
                    }
                    self.summaries.insert(row.session_id.clone(), row.clone());
                    if seen.insert(row.session_id.clone()) {
                        rows.push(row);
                    }
                }
                let next = result
                    .get("nextCursor")
                    .filter(|v| !v.is_null())
                    .map(|_| string(result, "nextCursor").map(str::to_owned))
                    .transpose()?;
                self.list_reply(&host, cwd, rows, next, batch)?;
            }
            Pending::Mode {
                host,
                id,
                requested,
                revision,
            } => {
                if !result.is_object() {
                    return Err("ACP mode response must be an object".into());
                }
                self.mode_response(&host, &id, requested, revision, batch)?;
            }
            Pending::Config { host, id } => {
                let options = config(result)?;
                self.sessions
                    .get_mut(&id)
                    .ok_or("ACP config session disappeared")?
                    .config = options.clone();
                #[cfg(unix)]
                self.dirty.insert(id.clone());
                batch.events.push(notification(
                    "thread/config/updated",
                    json!({"threadId":id,"configOptions":options}),
                ));
                batch
                    .events
                    .push(reply(&host, json!({"configOptions":options})));
            }
            Pending::Prompt { .. } => unreachable!(),
        }
        Ok(())
    }

    fn list_reply(
        &self,
        host: &str,
        cwd: Option<String>,
        rows: Vec<Summary>,
        provider: Option<String>,
        batch: &mut Batch,
    ) -> Result<(), String> {
        let remainder: Vec<_> = rows
            .iter()
            .skip(100)
            .map(|r| r.session_id.clone())
            .collect();
        let next = if remainder.is_empty() && provider.is_none() {
            None
        } else {
            Some(
                serde_json::to_string(&DirectoryCursor {
                    generation: self.directory_generation,
                    cwd,
                    remainder,
                    provider,
                })
                .map_err(|e| e.to_string())?,
            )
        };
        let data: Vec<_> = rows.iter().take(100).map(|row| {
            if let Some(session) = self.sessions.get(&row.session_id) {session.thread(false)} else {
                json!({"id":row.session_id,"cwd":row.cwd,"name":row.title,"acpSessionInfo":row.info,"preview":"DeepSeek Harness","historyMode":"acp-observed","status":{"type":"notLoaded"},"turns":[]})
            }
        }).collect();
        batch
            .events
            .push(reply(host, json!({"data":data,"nextCursor":next})));
        Ok(())
    }

    fn update(&mut self, params: &Value, batch: &mut Batch) -> Result<(), String> {
        let id = string(params, "sessionId")?;
        let session = self
            .sessions
            .get_mut(id)
            .ok_or("ACP update targets an unowned session")?;
        if !session.resident {
            return Err("ACP update targets an inactive archived session".into());
        }
        let update = params.get("update").ok_or("ACP update lacks payload")?;
        let kind = string(update, "sessionUpdate")?;
        if kind == "session_info_update" {
            // Validate the whole patch before changing either metadata index.
            let mut summary = session.summary.clone();
            patch_field(update, "title", &mut summary.title)?;
            patch_field(update, "updatedAt", &mut summary.info.updated_at)?;
            patch_field(update, "_meta", &mut summary.info.meta)?;
            batch.events.push(notification(
                "thread/acpSessionInfo/updated",
                json!({"threadId":id,"threadName":summary.title,"sessionInfo":summary.info}),
            ));
            session.summary = summary.clone();
            self.summaries.insert(id.into(), summary);
            #[cfg(unix)]
            self.dirty.insert(id.into());
            return Ok(());
        }
        if kind == "current_mode_update" {
            let current = string(update, "currentModeId")?.to_owned();
            let modes = session.modes.get_or_insert_with(|| modes::State {
                current_mode_id: current.clone(),
                available_modes: vec![],
                meta: None,
            });
            modes.current_mode_id = current;
            session.mode_update_version += 1;
            #[cfg(unix)]
            self.dirty.insert(id.into());
            batch.events.push(modes::updated(id, modes));
            return Ok(());
        }
        if kind == "config_option_update" {
            session.config = config(update)?;
            #[cfg(unix)]
            self.dirty.insert(id.into());
            batch.events.push(notification(
                "thread/config/updated",
                json!({"threadId":id,"configOptions":session.config}),
            ));
            return Ok(());
        }
        if kind == "usage_update" {
            return Ok(());
        } // Not displayed by the current common projection.
        let turn=session.active.as_ref().ok_or("ACP emitted conversation activity without a Host prompt; autonomous turns need a separate boundary contract")?.clone();
        if !matches!(kind, "agent_message_chunk" | "agent_thought_chunk") {
            session.anonymous_stream = None;
        }
        if kind == "plan" {
            let plan = crate::tasks::Plan::from_acp(id, &turn, update)?;
            batch.events.push(plan.notification());
            return Ok(());
        }
        let items = session.turns.last_mut().ok_or("ACP active turn missing")?["items"]
            .as_array_mut()
            .ok_or("ACP items is not an array")?;
        let item = match kind {
            "agent_message_chunk" | "agent_thought_chunk" => {
                let content = update.get("content").ok_or("ACP message has no content")?;
                let message_id = update.get("messageId").and_then(Value::as_str);
                let identity = message_id.map_or_else(
                    || format!("{turn}:{kind}:segment:{}", items.len()),
                    |id| format!("{turn}:{kind}:id:{id}"),
                );
                let existing = if message_id.is_some() {
                    items.iter().position(|item| item["id"] == identity)
                } else {
                    session
                        .anonymous_stream
                        .as_ref()
                        .filter(|(previous_kind, _)| previous_kind == kind)
                        .map(|(_, index)| *index)
                };
                let index = existing.unwrap_or_else(|| {
                    items.push(if kind == "agent_thought_chunk" {
                        json!({"id":identity,"type":"reasoning","summary":[],"content":[]})
                    } else {
                        json!({"id":identity,"type":"acpMessage","content":[]})
                    });
                    items.len() - 1
                });
                session.anonymous_stream = message_id.is_none().then(|| (kind.to_owned(), index));
                let entry = &mut items[index];
                if kind == "agent_thought_chunk" {
                    if string(content, "type")? != "text" {
                        return Err("ACP thought content must be text".into());
                    }
                    entry["content"]
                        .as_array_mut()
                        .ok_or("thought content is not an array")?
                        .push(json!(string(content, "text")?));
                } else {
                    let parts = entry["content"]
                        .as_array_mut()
                        .ok_or("message content is not an array")?;
                    if content["type"] == "text" {
                        let text = content
                            .get("text")
                            .and_then(Value::as_str)
                            .ok_or("ACP text chunk lacks text")?;
                        if let Some(last) = parts.last_mut().filter(|p| p["type"] == "text") {
                            let previous =
                                last["text"].as_str().ok_or("ACP text part is not text")?;
                            last["text"] = json!(format!("{previous}{text}"));
                        } else {
                            parts.push(content.clone());
                        }
                    } else {
                        parts.push(content.clone());
                    }
                }
                entry.clone()
            }
            "tool_call" | "tool_call_update" => {
                let call = string(update, "toolCallId")?;
                let identity = format!("{turn}:tool:{call}");
                let existing = items.iter().position(|i| i["id"] == identity);
                if existing.is_none() && kind != "tool_call" {
                    return Err("ACP tool update precedes tool call".into());
                }
                let merged =
                    tool_calls::merge(existing.map(|index| &items[index]["call"]), update)?;
                let index = existing.unwrap_or_else(|| {
                    items.push(json!({"id":identity,"type":"acpToolCall","call":{}}));
                    items.len() - 1
                });
                let entry = &mut items[index];
                let was_completed = entry["call"]["status"] == "completed";
                entry["call"] = merged;
                if !was_completed
                    && entry["call"]["status"] == "completed"
                    && entry["call"]["title"] == "todo_write"
                {
                    let plan = crate::tasks::Plan::from_deepseek_todos(
                        id,
                        &turn,
                        &entry["call"]["rawInput"],
                    )?;
                    batch.events.push(plan.notification());
                }
                entry.clone()
            }
            _ => {
                let item = json!({"id":format!("{turn}:unknown:{}",items.len()),"type":"acpUpdate","update":update});
                items.push(item.clone());
                item
            }
        };
        batch.events.push(notification(
            "item/completed",
            json!({"threadId":id,"turnId":turn,"item":item}),
        ));
        Ok(())
    }

    pub fn permission_response(&mut self, id: &Value, result: &Value) -> Result<Value, String> {
        let pending = self
            .permissions
            .get(&id.to_string())
            .ok_or("ACP permission is no longer pending")?;
        let outcome = result
            .get("outcome")
            .ok_or("ACP permission result lacks outcome")?;
        match string(outcome, "outcome")? {
            "selected" => {
                let choice = string(outcome, "optionId")?;
                if !pending["params"]["options"]
                    .as_array()
                    .ok_or("permission options malformed")?
                    .iter()
                    .any(|v| v["optionId"] == choice)
                {
                    return Err("ACP permission option is no longer offered".into());
                }
            }
            "cancelled" => {}
            _ => return Err("Invalid ACP permission outcome".into()),
        }
        Ok(json!({"jsonrpc":"2.0","id":id,"result":result}))
    }
    pub fn answered(&mut self, id: &Value) {
        self.permissions.remove(&id.to_string());
    }

    fn validate(&self) -> Result<(), String> {
        if self.sessions.len() > 64
            || self.summaries.len() > 5000
            || self.pending.len() > 160
            || self.permissions.len() > 64
        {
            return Err("ACP adapter capacity exceeded".into());
        }
        let mut bytes = 0;
        for session in self.sessions.values() {
            if let Some(modes) = &session.modes {
                modes.validate()?;
            }
            let size = serde_json::to_vec(session)
                .map_err(|e| e.to_string())?
                .len();
            bytes += size;
            if size > 2 * 1024 * 1024
                || session.turns.len() > 500
                || session
                    .turns
                    .iter()
                    .filter_map(|t| t["items"].as_array())
                    .map(Vec::len)
                    .sum::<usize>()
                    > 500
            {
                return Err("ACP observed conversation exceeds 500 entries/turns or 2 MiB; no history was silently discarded".into());
            }
        }
        bytes += serde_json::to_vec(&self.summaries)
            .map_err(|e| e.to_string())?
            .len();
        if bytes > 8 * 1024 * 1024 {
            return Err("ACP adapter retained projection exceeds 8 MiB".into());
        }
        Ok(())
    }
}

fn choice_exists(options: &Value, value: &str) -> bool {
    options.as_array().is_some_and(|rows| {
        rows.iter().any(|row| {
            row["value"] == value
                || row
                    .get("options")
                    .is_some_and(|children| choice_exists(children, value))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertised_modes_are_projected_and_switchable() {
        let mut adapter = Adapter::default();
        let created = adapter
            .dispatch("create".into(), "thread/start", json!({"cwd":cwd()}))
            .unwrap();
        let modes = json!({"currentModeId":"ask", "availableModes":[{"id":"ask","name":"Ask"},{"id":"code","name":"Code","description":"Change files"}]});
        let result = adapter.receive(json!({"jsonrpc":"2.0","id":created.wire[0]["id"],"result":{"sessionId":"s","modes":modes}})).unwrap();
        assert_eq!(result.events[0]["result"]["thread"]["acpModes"], modes);
        let change = adapter
            .dispatch(
                "mode".into(),
                "acp/mode/set",
                json!({"threadId":"s","modeId":"code"}),
            )
            .unwrap();
        assert_eq!(change.wire[0]["method"], "session/set_mode");
        assert_eq!(
            change.wire[0]["params"],
            json!({"sessionId":"s","modeId":"code"})
        );
    }

    fn cwd() -> std::path::PathBuf {
        std::env::temp_dir().join("fleetpane-synthetic")
    }

    fn setup() -> Adapter {
        let mut adapter = Adapter::default();
        let batch = adapter
            .dispatch("create".into(), "thread/start", json!({"cwd":cwd()}))
            .unwrap();
        adapter.receive(json!({"jsonrpc":"2.0","id":batch.wire[0]["id"],"result":{"sessionId":"s","configOptions":[{"id":"model","options":[{"group":"models","options":[{"value":"a"},{"value":"b"}]}]}]}})).unwrap();
        adapter
    }
    fn start(adapter: &mut Adapter) -> Value {
        let batch = adapter
            .dispatch(
                "send".into(),
                "turn/start",
                json!({"threadId":"s","input":[{"type":"text","text":"hello"}]}),
            )
            .unwrap();
        assert_eq!(
            batch.events.last().unwrap()["result"]["admission"],
            "host-dispatched"
        );
        batch.wire[0]["id"].clone()
    }
    fn update(adapter: &mut Adapter, value: Value) -> Batch {
        adapter.receive(json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":value}})).unwrap()
    }

    #[test]
    fn minimal_tool_call_defaults_and_interleaved_updates_keep_one_card() {
        let mut adapter = setup();
        start(&mut adapter);
        update(
            &mut adapter,
            json!({"sessionUpdate":"tool_call","toolCallId":"a","title":"First","name":"read_file","locations":[{"path":cwd().join("file.rs"),"line":0}],"rawInput":false}),
        );
        assert_eq!(
            adapter.sessions["s"].turns[0]["items"][1]["call"]["status"],
            "pending"
        );
        update(
            &mut adapter,
            json!({"sessionUpdate":"tool_call","toolCallId":"b","title":"Second","status":"in_progress","rawOutput":{"old":true}}),
        );
        update(
            &mut adapter,
            json!({"sessionUpdate":"tool_call_update","toolCallId":"a","title":"","status":"completed","rawOutput":0}),
        );
        update(
            &mut adapter,
            json!({"sessionUpdate":"tool_call_update","toolCallId":"b","rawOutput":{"new":true},"status":"failed"}),
        );
        update(
            &mut adapter,
            json!({"sessionUpdate":"tool_call_update","toolCallId":"a","title":null,"name":null,"rawInput":null,"rawOutput":null,"locations":[],"content":[]}),
        );
        let items = adapter.sessions["s"].turns[0]["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        let a = &items[1]["call"];
        assert_eq!(a["title"], "");
        assert_eq!(a["name"], "read_file");
        assert_eq!(a["kind"], "other");
        assert_eq!(a["rawInput"], false);
        assert_eq!(a["rawOutput"], 0);
        assert_eq!(a["locations"], json!([]));
        assert_eq!(items[2]["call"]["rawOutput"], json!({"new":true}));
        assert_eq!(items[2]["call"]["status"], "failed");
    }

    #[test]
    fn invalid_tool_patch_is_rejected_before_changing_a_card() {
        let mut adapter = setup();
        start(&mut adapter);
        update(
            &mut adapter,
            json!({"sessionUpdate":"tool_call","toolCallId":"a","title":"Original","status":"pending"}),
        );
        for patch in [
            json!({"title":42}),
            json!({"status":false}),
            json!({"name":[]}),
            json!({"kind":0}),
            json!({"locations":[{"path":false}]}),
            json!({"locations":[{"path":cwd(),"line":-1}]}),
            json!({"content":{}}),
        ] {
            let before = adapter.sessions["s"].thread(true);
            let mut patch = patch;
            patch["sessionUpdate"] = json!("tool_call_update");
            patch["toolCallId"] = json!("a");
            assert!(adapter.receive(json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":patch}})).is_err());
            assert_eq!(adapter.sessions["s"].thread(true), before);
        }
    }

    #[test]
    fn session_info_is_idle_metadata_and_preserves_omitted_fields() {
        let mut adapter = setup();
        let batch = update(
            &mut adapter,
            json!({"sessionUpdate":"session_info_update",
            "title":"  中文 title  ","updatedAt":"2026-10-21T09:08:07+08:00",
            "_meta":{"vendor":{"opaque":[1,true,null]}}}),
        );
        assert!(batch.wire.is_empty());
        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.events[0]["method"], "thread/acpSessionInfo/updated");
        let before = adapter.sessions["s"].thread(true);
        assert_eq!(before["name"], "  中文 title  ");
        assert_eq!(
            before["acpSessionInfo"]["updatedAt"],
            "2026-10-21T09:08:07+08:00"
        );
        assert_eq!(before["turns"], json!([]));
        update(
            &mut adapter,
            json!({"sessionUpdate":"session_info_update","updatedAt":null}),
        );
        let after = adapter.sessions["s"].thread(true);
        assert_eq!(after["name"], before["name"]);
        assert!(after["acpSessionInfo"]["updatedAt"].is_null());
        assert_eq!(
            after["acpSessionInfo"]["_meta"],
            before["acpSessionInfo"]["_meta"]
        );
        update(
            &mut adapter,
            json!({"sessionUpdate":"session_info_update","title":null,"_meta":null}),
        );
        assert!(adapter.sessions["s"].thread(true)["name"].is_null());
        assert!(adapter.summaries["s"].title.is_none());
    }

    #[test]
    fn session_info_during_prompt_does_not_create_or_split_history() {
        let mut adapter = setup();
        start(&mut adapter);
        update(
            &mut adapter,
            json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"before "}}),
        );
        update(
            &mut adapter,
            json!({"sessionUpdate":"session_info_update","title":"During prompt"}),
        );
        update(
            &mut adapter,
            json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"after"}}),
        );
        let items = adapter.sessions["s"].turns[0]["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[1]["content"][0]["text"], "before after");
    }

    #[test]
    fn malformed_session_info_never_partially_changes_metadata() {
        for patch in [
            json!({"title":42}),
            json!({"title":"new","updatedAt":42}),
            json!({"title":"new","_meta":[]}),
        ] {
            let mut adapter = setup();
            let before = adapter.sessions["s"].thread(true);
            let mut update = patch;
            update["sessionUpdate"] = json!("session_info_update");
            let error = adapter.receive(json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":update}})).err().unwrap();
            assert!(error.contains("invalid ACP session info"), "{error}");
            assert_eq!(adapter.sessions["s"].thread(true), before);
            assert!(adapter.summaries["s"].title.is_none());
        }
    }

    #[test]
    fn resident_info_is_not_replaced_by_a_stale_list_response() {
        let mut adapter = setup();
        let list = adapter
            .dispatch("list".into(), "thread/list", json!({}))
            .unwrap();
        update(
            &mut adapter,
            json!({"sessionUpdate":"session_info_update","title":"Live 中文"}),
        );
        let result = adapter.receive(json!({"jsonrpc":"2.0","id":list.wire[0]["id"],"result":{"sessions":[{"sessionId":"s","cwd":cwd(),"title":"old"}]}})).unwrap();
        assert_eq!(
            result.events.last().unwrap()["result"]["data"][0]["name"],
            "Live 中文"
        );
        assert_eq!(adapter.summaries["s"].title.as_deref(), Some("Live 中文"));
    }

    #[test]
    fn anonymous_chunks_keep_tool_and_role_boundaries_without_trimming() {
        let mut adapter = setup();
        start(&mut adapter);
        for value in [
            json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":" Before 中文\n"}}),
            json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"  second line "}}),
            json!({"sessionUpdate":"tool_call","toolCallId":"call","title":"Read","status":"in_progress"}),
            json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":" After "}}),
            json!({"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":" thought "}}),
            json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":" Tail "}}),
            json!({"sessionUpdate":"tool_call_update","toolCallId":"call","status":"completed","rawOutput":"exact result"}),
            json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":" New "}}),
        ] {
            update(&mut adapter, value);
        }
        let items = adapter.sessions["s"].turns[0]["items"].as_array().unwrap();
        assert_eq!(
            items
                .iter()
                .map(|item| item["type"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "userMessage",
                "acpMessage",
                "acpToolCall",
                "acpMessage",
                "reasoning",
                "acpMessage",
                "acpMessage"
            ]
        );
        assert_eq!(
            items[1]["content"][0]["text"],
            " Before 中文\n  second line "
        );
        assert_eq!(items[2]["call"]["status"], "completed");
        assert_eq!(items[3]["content"][0]["text"], " After ");
        assert_eq!(items[4]["content"][0], " thought ");
        assert_eq!(items[5]["content"][0]["text"], " Tail ");
        assert_eq!(items[6]["content"][0]["text"], " New ");
    }

    #[test]
    fn explicit_message_identity_survives_interleaving_and_cannot_alias_anonymous() {
        let mut adapter = setup();
        start(&mut adapter);
        for value in [
            json!({"sessionUpdate":"agent_message_chunk","messageId":"stream","content":{"type":"text","text":"one"}}),
            json!({"sessionUpdate":"tool_call","toolCallId":"call","title":"Read","status":"completed"}),
            json!({"sessionUpdate":"agent_message_chunk","messageId":"stream","content":{"type":"text","text":"two"}}),
            json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"anonymous"}}),
        ] {
            update(&mut adapter, value);
        }
        let items = adapter.sessions["s"].turns[0]["items"].as_array().unwrap();
        assert_eq!(items.len(), 4);
        assert_eq!(items[1]["content"][0]["text"], "onetwo");
        assert_eq!(items[3]["content"][0]["text"], "anonymous");
        let ids: std::collections::BTreeSet<_> = items
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids.len(), items.len());
    }

    #[test]
    fn acp_plans_are_full_task_updates_instead_of_unknown_history_items() {
        let mut adapter = setup();
        start(&mut adapter);
        let count = adapter.sessions["s"].turns[0]["items"]
            .as_array()
            .unwrap()
            .len();
        let batch = update(
            &mut adapter,
            json!({"sessionUpdate":"plan","entries":[{"content":"检验","status":"in_progress","priority":"high"}]}),
        );
        assert!(batch.wire.is_empty());
        let plan = crate::tasks::Plan::from_event(&batch.events[0])
            .unwrap()
            .unwrap();
        assert_eq!(plan.entries[0].priority.as_deref(), Some("high"));
        assert_eq!(
            adapter.sessions["s"].turns[0]["items"]
                .as_array()
                .unwrap()
                .len(),
            count
        );
        let batch = update(&mut adapter, json!({"sessionUpdate":"plan","entries":[]}));
        assert!(
            crate::tasks::Plan::from_event(&batch.events[0])
                .unwrap()
                .unwrap()
                .entries
                .is_empty()
        );
    }

    #[test]
    fn deepseek_completed_todos_report_plans_without_inventing_priority() {
        let mut adapter = setup();
        start(&mut adapter);
        let call = json!({"sessionUpdate":"tool_call","toolCallId":"todo-1","title":"todo_write","status":"pending","rawInput":{"todos":[{"content":"真实任务","status":"in_progress"}]}});
        let batch = update(&mut adapter, call);
        assert!(
            batch
                .events
                .iter()
                .all(|e| e["method"] != "turn/plan/updated")
        );
        let batch = update(
            &mut adapter,
            json!({"sessionUpdate":"tool_call_update","toolCallId":"todo-1","status":"completed"}),
        );
        let plans: Vec<_> = batch
            .events
            .iter()
            .filter_map(|e| crate::tasks::Plan::from_event(e).unwrap())
            .collect();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].entries[0].content, "真实任务");
        assert_eq!(plans[0].entries[0].status, "in_progress");
        assert_eq!(plans[0].entries[0].priority, None);
        // A late content update for this old tool must not overwrite a newer plan.
        let batch = update(
            &mut adapter,
            json!({"sessionUpdate":"tool_call","toolCallId":"todo-2","title":"todo_write","status":"completed","rawInput":{"todos":[]}}),
        );
        assert!(
            batch
                .events
                .iter()
                .filter_map(|e| crate::tasks::Plan::from_event(e).unwrap())
                .next()
                .unwrap()
                .entries
                .is_empty()
        );
        let batch = update(
            &mut adapter,
            json!({"sessionUpdate":"tool_call_update","toolCallId":"todo-1","content":[]}),
        );
        assert!(
            batch
                .events
                .iter()
                .all(|e| e["method"] != "turn/plan/updated")
        );
        let batch = update(
            &mut adapter,
            json!({"sessionUpdate":"tool_call","toolCallId":"failed","title":"todo_write","status":"failed","rawInput":{"todos":false}}),
        );
        assert!(
            batch
                .events
                .iter()
                .all(|e| e["method"] != "turn/plan/updated")
        );
        let malformed = adapter.receive(json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call","toolCallId":"bad","title":"todo_write","status":"completed","rawInput":{"todos":[{"content":"Missing status"}]}}}}));
        assert!(malformed.is_err());
    }

    #[test]
    fn chunks_merge_by_message_and_partial_tool_updates_keep_input() {
        let mut adapter = setup();
        let wire = start(&mut adapter);
        for text in ["hello ", "world"] {
            update(
                &mut adapter,
                json!({"sessionUpdate":"agent_message_chunk","messageId":"m1","content":{"type":"text","text":text}}),
            );
        }
        update(
            &mut adapter,
            json!({"sessionUpdate":"tool_call","toolCallId":"tool","title":"read","status":"in_progress","rawInput":{"path":"sample"}}),
        );
        update(
            &mut adapter,
            json!({"sessionUpdate":"tool_call_update","toolCallId":"tool","status":"completed","title":null,"content":[{"type":"content","content":{"type":"text","text":"done"}}]}),
        );
        update(
            &mut adapter,
            json!({"sessionUpdate":"agent_message_chunk","messageId":"m2","content":{"type":"image","mimeType":"image/png","data":"synthetic"}}),
        );
        let batch = adapter
            .receive(json!({"jsonrpc":"2.0","id":wire,"result":{"stopReason":"end_turn"}}))
            .unwrap();
        assert_eq!(batch.events[0]["params"]["turn"]["items"], json!([]));
        let items = adapter.sessions["s"].turns[0]["items"].as_array().unwrap();
        assert_eq!(items.len(), 4);
        assert_eq!(items[1]["content"][0]["text"], "hello world");
        assert_eq!(items[2]["call"]["title"], "read");
        assert_eq!(items[2]["call"]["rawInput"]["path"], "sample");
        assert_eq!(items[2]["call"]["status"], "completed");
        assert_eq!(items[3]["content"][0]["type"], "image");
    }

    #[test]
    fn prompt_admission_and_activity_are_not_settlement() {
        for (reason, status) in [
            ("end_turn", "completed"),
            ("max_tokens", "completed"),
            ("max_turn_requests", "completed"),
            ("refusal", "completed"),
            ("cancelled", "interrupted"),
        ] {
            let mut adapter = setup();
            let wire = start(&mut adapter);
            let activity = update(
                &mut adapter,
                json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Observed output is not a completion signal"}}),
            );
            assert!(
                activity
                    .events
                    .iter()
                    .all(|event| event["method"] != "turn/completed")
            );
            assert!(adapter.sessions["s"].active.is_some());
            assert_eq!(adapter.sessions["s"].turns[0]["status"], "inProgress");
            let batch = adapter
                .receive(json!({"jsonrpc":"2.0","id":wire,"result":{"stopReason":reason}}))
                .unwrap();
            let ended = batch
                .events
                .iter()
                .find(|event| event["method"] == "turn/completed")
                .unwrap();
            assert_eq!(ended["params"]["turn"]["stopReason"], reason);
            assert_eq!(ended["params"]["turn"]["status"], status);
            assert!(adapter.sessions["s"].active.is_none());
        }
        let mut adapter = setup();
        let wire = start(&mut adapter);
        assert!(
            adapter
                .receive(
                    json!({"jsonrpc":"2.0","id":wire,"result":{"stopReason":"future_success"}})
                )
                .is_err()
        );
    }

    #[test]
    fn settlement_does_not_reemit_large_turn() {
        let mut adapter = setup();
        let wire = start(&mut adapter);
        for index in 0..4 {
            update(
                &mut adapter,
                json!({"sessionUpdate":"tool_call","toolCallId":index.to_string(),"title":"large output","status":"completed","rawOutput":"x".repeat(70000)}),
            );
        }
        let batch = adapter
            .receive(json!({"jsonrpc":"2.0","id":wire,"result":{"stopReason":"end_turn"}}))
            .unwrap();
        assert!(serde_json::to_vec(&batch.events[0]).unwrap().len() < 1024);
        assert!(
            serde_json::to_vec(&adapter.sessions["s"]).unwrap().len() > crate::MAX_PROVIDER_FRAME
        );
    }

    #[test]
    fn configuration_is_serialized_across_controllers() {
        let mut adapter = setup();
        let config = adapter
            .dispatch(
                "config".into(),
                "acp/config/set",
                json!({"threadId":"s","configId":"model","value":"b"}),
            )
            .unwrap();
        let blocked = adapter
            .dispatch(
                "send".into(),
                "turn/start",
                json!({"threadId":"s","input":[{"type":"text","text":"hello"}]}),
            )
            .unwrap();
        assert!(blocked.wire.is_empty());
        assert!(blocked.events[0].get("error").is_some());
        assert!(adapter.sessions["s"].turns.is_empty());
        adapter
            .receive(
                json!({"jsonrpc":"2.0","id":config.wire[0]["id"],"result":{"configOptions":[]}}),
            )
            .unwrap();
        start(&mut adapter);
    }

    #[test]
    fn cancellation_answers_permissions_before_settlement_and_isolates_sessions() {
        let mut adapter = setup();
        let wire = start(&mut adapter);
        let create = adapter
            .dispatch("other".into(), "thread/start", json!({"cwd":cwd()}))
            .unwrap();
        adapter
            .receive(
                json!({"jsonrpc":"2.0","id":create.wire[0]["id"],"result":{"sessionId":"other"}}),
            )
            .unwrap();
        adapter
            .dispatch(
                "other-send".into(),
                "turn/start",
                json!({"threadId":"other","input":[{"type":"text","text":"hello"}]}),
            )
            .unwrap();
        adapter.receive(json!({"jsonrpc":"2.0","id":17,"method":"session/request_permission","params":{"sessionId":"other","options":[{"optionId":"skip","name":"Skip","kind":"reject_once"}]}})).unwrap();
        let request = json!({"jsonrpc":"2.0","id":"permission-1","method":"session/request_permission","params":{"sessionId":"s","options":[{"optionId":"skip","name":"Skip","kind":"reject_once"}]}});
        adapter.receive(request.clone()).unwrap();
        let mut second = request.clone();
        second["id"] = json!(16);
        adapter.receive(second).unwrap();
        assert!(
            adapter
                .permission_response(
                    &json!("permission-1"),
                    &json!({"outcome":{"outcome":"selected","optionId":"invented"}})
                )
                .is_err()
        );
        let turn = adapter.sessions["s"].active.clone().unwrap();
        let stop = adapter
            .dispatch(
                "stop".into(),
                "turn/interrupt",
                json!({"threadId":"s","turnId":turn}),
            )
            .unwrap();
        assert!(stop.wire[0].get("id").is_none());
        assert_eq!(
            stop.wire.len(),
            3,
            "Stop must answer both pending permissions"
        );
        for reply in &stop.wire[1..] {
            assert_eq!(reply["result"], json!({"outcome":{"outcome":"cancelled"}}));
            assert!(reply["id"] == 16 || reply["id"] == "permission-1");
        }
        assert_eq!(
            stop.events
                .iter()
                .filter(|e| e["method"] == "serverRequest/resolved")
                .count(),
            2
        );
        assert_eq!(
            adapter.sessions["s"].thread(true)["status"]["cancelRequested"],
            true
        );
        assert_eq!(adapter.permissions.len(), 1);
        assert!(adapter.permissions.contains_key("17"));
        assert!(
            adapter
                .permission_response(
                    &json!("permission-1"),
                    &json!({"outcome":{"outcome":"selected","optionId":"skip"}})
                )
                .is_err()
        );
        let repeat = adapter
            .dispatch(
                "repeat-stop".into(),
                "turn/interrupt",
                json!({"threadId":"s","turnId":turn}),
            )
            .unwrap();
        assert!(repeat.wire.is_empty(), "Do not send cancellation twice");
        let mut late = request;
        late["id"] = json!("late-permission");
        let late = adapter.receive(late).unwrap();
        assert!(
            late.events.is_empty(),
            "No actionable approval during cancellation"
        );
        assert_eq!(
            late.wire[0]["result"],
            json!({"outcome":{"outcome":"cancelled"}})
        );
        assert!(adapter.sessions["s"].active.is_some());
        let ended = adapter
            .receive(json!({"jsonrpc":"2.0","id":wire,"result":{"stopReason":"cancelled"}}))
            .unwrap();
        assert_eq!(ended.events[0]["params"]["turn"]["status"], "interrupted");
        assert_eq!(
            ended.events.last().unwrap()["params"]["status"]["type"],
            "idle"
        );
        assert_ne!(
            adapter.sessions["s"].thread(true)["status"]["cancelRequested"],
            true
        );
        assert_eq!(adapter.permissions.len(), 1);
    }

    #[test]
    fn inactive_only_provider_directory_keeps_residents_and_paginates_without_loss() {
        let mut adapter = setup();
        start(&mut adapter);
        let listing = adapter
            .dispatch("list".into(), "thread/list", json!({}))
            .unwrap();
        let rows: Vec<_> = (0..100)
            .map(|i| json!({"sessionId":format!("saved-{i}"),"cwd":cwd(),"title":null}))
            .collect();
        let page=adapter.receive(json!({"jsonrpc":"2.0","id":listing.wire[0]["id"],"result":{"sessions":rows,"nextCursor":"provider-more"}})).unwrap();
        let result = &page.events[0]["result"];
        assert_eq!(result["data"].as_array().unwrap().len(), 100);
        assert_eq!(result["data"][0]["id"], "s");
        assert_eq!(result["data"][0]["status"]["type"], "active");
        let next = adapter
            .dispatch(
                "next".into(),
                "thread/list",
                json!({"cursor":result["nextCursor"]}),
            )
            .unwrap();
        assert!(next.wire.is_empty());
        assert_eq!(next.events[0]["result"]["data"][0]["id"], "saved-99");
        let continuation = &next.events[0]["result"]["nextCursor"];
        let upstream = adapter
            .dispatch("last".into(), "thread/list", json!({"cursor":continuation}))
            .unwrap();
        assert_eq!(upstream.wire[0]["params"]["cursor"], "provider-more");
        let wrong = adapter
            .dispatch(
                "wrong".into(),
                "thread/list",
                json!({"cwd":"/other","cursor":continuation}),
            )
            .unwrap();
        assert!(wrong.wire.is_empty() && wrong.events[0].get("error").is_some());
        let stale = setup()
            .dispatch(
                "stale".into(),
                "thread/list",
                json!({"cursor":continuation}),
            )
            .unwrap();
        assert!(stale.wire.is_empty() && stale.events[0].get("error").is_some());
    }

    #[test]
    fn malformed_activity_and_dispatch_capacity_fail_explicitly() {
        let mut adapter = setup();
        assert!(adapter.receive(json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"unowned activity"}}}})).is_err());
        for index in 0..160 {
            adapter
                .dispatch(index.to_string(), "thread/list", json!({}))
                .unwrap();
        }
        assert!(
            adapter
                .dispatch("overflow".into(), "thread/list", json!({}))
                .is_err()
        );
    }
}
