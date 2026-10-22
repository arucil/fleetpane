//! Versioned Session Host protocol. No UI or provider SDK dependencies.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashSet, VecDeque};
use uuid::Uuid;

pub mod acp;
pub mod approvals;
pub mod attachments;
pub mod attention;
#[cfg(unix)]
mod creation_directory;
#[cfg(unix)]
pub mod journal;
pub mod lifecycle;
pub mod session_directories;
pub mod stopping;
pub mod tasks;
pub mod terminal;
#[cfg(unix)]
pub mod unix;
pub mod usage;

pub const PROTOCOL_VERSION: u32 = 3;
pub const MAX_FRAME: usize = 1024 * 1024;
pub const MAX_PROVIDER_FRAME: usize = MAX_FRAME / 4;
const MAX_OPERATIONS: usize = 128;
const MAX_OPERATION_BYTES: usize = 8 * 1024 * 1024;
const MAX_QUERIES: usize = 32;
const MAX_EVENTS: usize = 128;
const MAX_EVENT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Lease {
    pub host_id: Uuid,
    pub controller_id: Uuid,
    pub generation: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Operation {
    pub id: Uuid,
    pub method: String,
    pub params: Value,
    pub outcome: Outcome,
    /// Ordered provider-response boundary for rebuilding a conversation projection.
    #[serde(default)]
    pub completed_sequence: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "state", content = "response", rename_all = "snake_case")]
pub enum Outcome {
    Accepted,
    Finished(Value),
    Unknown(String),
}

/// A provider failure fences that provider's work; a Host failure fences every
/// mutation, including local PTYs that share its durable journal.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FaultScope {
    Provider,
    Host,
}

impl FaultScope {
    fn includes(self, method: &str) -> bool {
        self == Self::Host || OperationTarget::for_method(method) == OperationTarget::Provider
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OperationTarget {
    Provider,
    Terminal,
}

impl OperationTarget {
    fn for_method(method: &str) -> Self {
        if terminal::is_mutation(method) {
            Self::Terminal
        } else {
            Self::Provider
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub sequence: u64,
    pub message: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub protocol: u32,
    pub host_id: Uuid,
    pub host_pid: u32,
    pub provider_pid: u32,
    pub provider_info: Value,
    pub owner: Option<Lease>,
    pub sequence: u64,
    pub operation_ids: Vec<Uuid>,
    /// Total durable receipts. `operation_ids` is only the bounded resident set.
    pub journal_total: u64,
    pub admission: lifecycle::Admission,
    pub activity: lifecycle::Activity,
    pub pending_requests: Vec<Value>,
    /// None means an older Host cannot provide completion review records.
    #[serde(default)]
    pub attention: Option<Box<attention::Feed>>,
    /// None means an older Host has no workspace association observations.
    #[serde(default)]
    pub session_directories: Option<Box<session_directories::Feed>>,
    pub fault: Option<String>,
    #[serde(default)]
    pub fault_scope: Option<FaultScope>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Request {
    Snapshot,
    SetAdmission {
        lease: Lease,
        expected_generation: u64,
        mode: lifecycle::Mode,
    },
    Claim {
        protocol: u32,
        host_id: Uuid,
        expected_generation: u64,
        controller_id: Uuid,
    },
    Execute {
        lease: Lease,
        operation_id: Uuid,
        method: String,
        params: Value,
    },
    /// Observational RPCs have no mutation identity or retained result.
    Query {
        lease: Lease,
        method: String,
        params: Value,
    },
    Operation {
        operation_id: Uuid,
    },
    Operations {
        cursor: Option<String>,
    },
    Events {
        host_id: Uuid,
        after: u64,
    },
    Respond {
        lease: Lease,
        request_id: Value,
        result: Value,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "response", content = "data", rename_all = "snake_case")]
pub enum Response {
    Snapshot(Snapshot),
    Admission(lifecycle::Admission),
    Lease(Lease),
    Operation(Operation),
    Operations(OperationPage),
    Query { message: Value, sequence: u64 },
    Events(Vec<Event>),
    Answered,
    Error { code: String, message: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperationEntry {
    pub position: u64,
    pub id: Uuid,
    pub host_id: Uuid,
    pub method: String,
    pub state: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperationPage {
    pub entries: Vec<OperationEntry>,
    pub next_cursor: Option<String>,
}

impl Response {
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self::Error {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{code}: {message}")]
pub struct HostError {
    code: &'static str,
    message: String,
}

impl HostError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<HostError> for Response {
    fn from(error: HostError) -> Self {
        Response::error(error.code, error.message)
    }
}

pub struct State {
    #[cfg(unix)]
    journal: Option<journal::Journal>,
    snapshot: Snapshot,
    operations: Vec<Operation>,
    queries: HashSet<Uuid>,
    events: VecDeque<(Event, usize)>,
    event_bytes: usize,
    activity: lifecycle::Tracker,
    stopping: stopping::Tracker,
    tasks: tasks::Cache,
    usage: usage::Cache,
}

/// Only these Codex RPCs are observational. Resume and unknown methods are writes.
pub fn is_query_method(method: &str) -> bool {
    matches!(
        method,
        "thread/list"
            | "thread/loaded/list"
            | "model/list"
            | "thread/read"
            | "thread/turns/list"
            | "thread/items/list"
            | tasks::READ_METHOD
            | usage::READ_METHOD
            | terminal::READ_METHOD
    )
}

pub struct QueryReply {
    pub id: Uuid,
    pub message: Value,
    pub sequence: u64,
}

impl State {
    pub fn new(provider_pid: u32, provider_info: Value) -> Self {
        Self {
            #[cfg(unix)]
            journal: None,
            snapshot: Snapshot {
                protocol: PROTOCOL_VERSION,
                host_id: Uuid::new_v4(),
                host_pid: std::process::id(),
                provider_pid,
                provider_info,
                owner: None,
                sequence: 0,
                operation_ids: Vec::new(),
                journal_total: 0,
                admission: lifecycle::Admission::default(),
                activity: lifecycle::Activity {
                    stopping: Some(Vec::new()),
                    terminal_ids: Vec::new(),
                    turns: Vec::new(),
                    in_flight_operations: 0,
                    unknown_operations: 0,
                },
                pending_requests: Vec::new(),
                attention: Some(Box::default()),
                session_directories: Some(Box::default()),
                fault: None,
                fault_scope: None,
            },
            operations: Vec::new(),
            queries: HashSet::new(),
            events: VecDeque::new(),
            event_bytes: 0,
            activity: lifecycle::Tracker::default(),
            stopping: stopping::Tracker::default(),
            tasks: tasks::Cache::default(),
            usage: usage::Cache::default(),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = self.snapshot.clone();
        snapshot.activity.turns = self.activity.snapshot();
        snapshot.activity.stopping = Some(self.stopping.snapshot());
        snapshot.activity.in_flight_operations = self
            .operations
            .iter()
            .filter(|op| matches!(op.outcome, Outcome::Accepted))
            .count();
        snapshot
    }

    pub fn fault(&self) -> Option<&str> {
        self.snapshot.fault.as_deref()
    }

    pub fn claim(
        &mut self,
        protocol: u32,
        host_id: Uuid,
        expected: u64,
        controller_id: Uuid,
    ) -> Response {
        if protocol != PROTOCOL_VERSION || host_id != self.snapshot.host_id {
            return Response::error(
                "incompatible_host",
                "protocol or host incarnation changed; read snapshot",
            );
        }
        let generation = self
            .snapshot
            .owner
            .as_ref()
            .map_or(0, |owner| owner.generation);
        if generation != expected {
            return Response::error(
                "ownership_conflict",
                "controller generation changed; read snapshot",
            );
        }
        let Some(next) = generation.checked_add(1) else {
            return Response::error("generation_exhausted", "cannot transfer ownership");
        };
        let lease = Lease {
            host_id,
            controller_id,
            generation: next,
        };
        self.snapshot.owner = Some(lease.clone());
        Response::Lease(lease)
    }

    pub fn authorize(&self, lease: &Lease) -> Result<(), HostError> {
        self.authorize_host(lease)?;
        if let Some(fault) = &self.snapshot.fault {
            return Err(HostError::new("provider_unavailable", fault));
        }
        Ok(())
    }

    fn authorize_host(&self, lease: &Lease) -> Result<(), HostError> {
        self.authorize_controller(lease)?;
        if self.snapshot.fault_scope == Some(FaultScope::Host) {
            return Err(HostError::new(
                "host_unavailable",
                self.snapshot
                    .fault
                    .as_deref()
                    .expect("Host fault has a reason"),
            ));
        }
        Ok(())
    }

    fn authorize_mutation(&self, lease: &Lease, method: &str) -> Result<(), HostError> {
        match OperationTarget::for_method(method) {
            OperationTarget::Terminal => self.authorize_host(lease),
            OperationTarget::Provider => self.authorize(lease),
        }
    }

    pub fn authorize_controller(&self, lease: &Lease) -> Result<(), HostError> {
        if self.snapshot.owner.as_ref() != Some(lease) {
            return Err(HostError::new(
                "stale_controller",
                "this controller no longer owns the host",
            ));
        }
        Ok(())
    }

    /// Reserve the bounded resident set. Transport must first consult and then
    /// commit the durable identity before dispatch; this cache alone cannot deduplicate.
    pub fn admit(
        &mut self,
        lease: &Lease,
        id: Uuid,
        method: String,
        params: Value,
    ) -> Result<bool, HostError> {
        self.authorize_mutation(lease, &method)?;
        if method.is_empty() || matches!(method.as_str(), "initialize" | "initialized") {
            return Err(HostError::new(
                "reserved_method",
                "provider initialization belongs to Session Host",
            ));
        }
        if method.len() > 256 {
            return Err(HostError::new("invalid_method", "method exceeds 256 bytes"));
        }
        if let Some(existing) = self.operations.iter().find(|op| op.id == id) {
            return if existing.method == method && existing.params == params {
                Ok(false)
            } else {
                Err(HostError::new(
                    "operation_conflict",
                    "operation ID already identifies another command",
                ))
            };
        }
        if !self.snapshot.admission.permits(&method) {
            return Err(HostError::new(
                "host_draining",
                "Host is draining; select a ready Host for new work. Existing output, answers, reads and Stop remain available.",
            ));
        }
        let stop = if OperationTarget::for_method(&method) == OperationTarget::Provider {
            self.stopping
                .prepare(id, &method, &params, &self.activity.snapshot())?
        } else {
            None
        };
        let operation = Operation {
            id,
            method,
            params,
            outcome: Outcome::Accepted,
            completed_sequence: None,
        };
        let weight = |op: &Operation| {
            serde_json::to_vec(op)
                .expect("operation JSON serialization")
                .len()
                + if matches!(op.outcome, Outcome::Accepted) {
                    MAX_PROVIDER_FRAME + 128
                } else {
                    0
                }
        };
        let required = weight(&operation);
        while self.operations.len() >= MAX_OPERATIONS
            || self.operations.iter().map(&weight).sum::<usize>() + required > MAX_OPERATION_BYTES
        {
            let Some(index) = self
                .operations
                .iter()
                .position(|op| !matches!(op.outcome, Outcome::Accepted))
            else {
                return Err(HostError::new(
                    "capacity",
                    "in-flight operation count/byte budget exhausted; command was not submitted",
                ));
            };
            let removed = self.operations.remove(index);
            self.snapshot.operation_ids.retain(|id| *id != removed.id);
        }
        // Transport must check and commit the durable journal before dispatch.
        // Dropping settled RAM entries never removes their on-disk identity.
        self.activity
            .admitted(id, &operation.method, &operation.params);
        self.operations.push(operation);
        self.snapshot.operation_ids.push(id);
        if let Some(stop) = stop {
            self.stopping.commit(stop);
        }
        Ok(true)
    }

    pub fn operation(&self, id: Uuid) -> Response {
        match self.operations.iter().find(|op| op.id == id) {
            Some(operation) => Response::Operation(operation.clone()),
            None => Response::error(
                "unknown_operation",
                "operation is not recorded in this host incarnation",
            ),
        }
    }

    pub fn admit_query(&mut self, lease: &Lease, method: &str) -> Result<Uuid, HostError> {
        self.authorize(lease)?;
        if !is_query_method(method) {
            return Err(HostError::new(
                "not_query",
                "method requires the operation journal; query was not submitted",
            ));
        }
        if self.queries.len() == MAX_QUERIES {
            return Err(HostError::new(
                "query_capacity",
                "all 32 query slots are in flight; query was not submitted",
            ));
        }
        let id = Uuid::new_v4();
        assert!(self.queries.insert(id), "query UUID collision");
        Ok(id)
    }

    pub fn record(&mut self, message: Value) -> Result<Option<QueryReply>, String> {
        self.record_from(message, OperationTarget::Provider)
    }

    fn check_response_target(
        &self,
        message: &Value,
        target: OperationTarget,
    ) -> Result<(), String> {
        if message.get("method").is_none()
            && let Some(id) = message.get("id").and_then(Value::as_str)
        {
            if id.starts_with("query:") {
                if target != OperationTarget::Provider {
                    return Err("terminal completion cannot answer a provider query".into());
                }
            } else {
                let id = Uuid::parse_str(id).map_err(|error| error.to_string())?;
                let operation = self
                    .operations
                    .iter()
                    .find(|op| op.id == id)
                    .ok_or("completion references an unrecorded operation")?;
                if OperationTarget::for_method(&operation.method) != target {
                    return Err("completion belongs to a different operation target".into());
                }
            }
        } else if target == OperationTarget::Terminal {
            return Err("terminal completion must be an operation response".into());
        }
        Ok(())
    }

    fn record_from(
        &mut self,
        message: Value,
        target: OperationTarget,
    ) -> Result<Option<QueryReply>, String> {
        self.check_response_target(&message, target)?;
        let plan = tasks::Plan::from_event(&message)?;
        let observation = usage::Observation::from_event(&message)?;
        let message = observation
            .as_ref()
            .map_or(message, usage::Observation::notification);
        let bytes = serde_json::to_vec(&message)
            .map_err(|error| error.to_string())?
            .len();
        if bytes + 1 > MAX_PROVIDER_FRAME {
            return Err("provider message exceeds frame limit".into());
        }
        let sequence = self
            .snapshot
            .sequence
            .checked_add(1)
            .ok_or("event sequence exhausted")?;
        let attention = attention::Record::from_event(&message, sequence)?;
        let directory = session_directories::Record::from_message(&message)?;
        if let Some(record) = &attention {
            self.snapshot
                .attention
                .as_ref()
                .expect("new Host has attention feed")
                .check(record)?;
        }
        self.activity.record(&message)?;
        let mut query_id = None;
        if let Some(method) = message.get("method") {
            if !method.is_string() {
                return Err("provider method must be a string".into());
            }
            if method == "serverRequest/resolved" {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Resolved {
                    request_id: Value,
                    thread_id: String,
                }
                let resolved: Resolved = serde_json::from_value(
                    message
                        .get("params")
                        .cloned()
                        .ok_or("resolved request has no params")?,
                )
                .map_err(|error| format!("invalid resolved request: {error}"))?;
                if !resolved.request_id.is_string() && resolved.request_id.as_i64().is_none() {
                    return Err("resolved request ID must be a string or integer".into());
                }
                if self.snapshot.pending_requests.iter().any(|pending| {
                    pending.get("id") == Some(&resolved.request_id)
                        && pending
                            .get("params")
                            .and_then(|params| params.get("threadId"))
                            .is_some_and(|id| id != &resolved.thread_id)
                }) {
                    return Err("resolved request belongs to a different thread".into());
                }
                self.answered(&resolved.request_id);
            }
            if message.get("id").is_some() {
                if let Some(existing) = self
                    .snapshot
                    .pending_requests
                    .iter()
                    .find(|pending| pending.get("id") == message.get("id"))
                {
                    if existing != &message {
                        return Err(
                            "provider reused a pending request ID with different contents".into(),
                        );
                    }
                } else {
                    if self.snapshot.pending_requests.len() >= 64 {
                        return Err("pending provider request capacity exceeded".into());
                    }
                    let pending_bytes = serde_json::to_vec(&self.snapshot.pending_requests)
                        .map_err(|error| error.to_string())?
                        .len();
                    if pending_bytes + bytes + 1 > MAX_FRAME / 2 {
                        return Err("pending provider request byte capacity exceeded".into());
                    }
                    self.snapshot.pending_requests.push(message.clone());
                }
            }
        } else if let Some(id) = message.get("id").and_then(Value::as_str) {
            if message.get("result").is_some() == message.get("error").is_some() {
                return Err("provider response must contain exactly one of result or error".into());
            }
            if let Some(id) = id.strip_prefix("query:") {
                let id = Uuid::parse_str(id)
                    .map_err(|error| format!("unexpected query response ID: {error}"))?;
                if !self.queries.remove(&id) {
                    return Err("provider replied to an unrecorded query".into());
                }
                query_id = Some(id);
            } else {
                let id = Uuid::parse_str(id)
                    .map_err(|error| format!("unexpected provider response ID: {error}"))?;
                let operation = self
                    .operations
                    .iter_mut()
                    .find(|op| op.id == id)
                    .ok_or_else(|| "provider replied to an unrecorded operation".to_owned())?;
                if !matches!(operation.outcome, Outcome::Accepted) {
                    return Err("provider replied twice to the same operation".into());
                }
                self.activity.replied(id, &message)?;
                operation.outcome = Outcome::Finished(message.clone());
                operation.completed_sequence = Some(sequence);
            }
        } else {
            return Err("invalid provider envelope".into());
        }
        if target == OperationTarget::Provider {
            self.stopping.record(&message)?;
        }
        if let Some(plan) = plan {
            self.tasks.record(plan);
        }
        if let Some(observation) = observation {
            self.usage.record(observation);
        }
        if let Some(record) = attention {
            self.snapshot
                .attention
                .as_mut()
                .expect("new Host has attention feed")
                .record(record);
        }
        if let Some(record) = directory {
            self.snapshot
                .session_directories
                .as_mut()
                .expect("new Host observes directories")
                .record(record);
        }
        self.snapshot.sequence = sequence;
        let query_reply = query_id.map(|id| QueryReply {
            id,
            message: message.clone(),
            sequence,
        });
        self.events.push_back((
            Event {
                sequence: self.snapshot.sequence,
                message,
            },
            bytes,
        ));
        self.event_bytes += bytes;
        while self.events.len() > MAX_EVENTS || self.event_bytes > MAX_EVENT_BYTES {
            let (_, removed_bytes) = self.events.pop_front().expect("nonempty event ring");
            self.event_bytes -= removed_bytes;
        }
        Ok(query_reply)
    }

    pub fn events(&self, host_id: Uuid, after: u64) -> Response {
        if host_id != self.snapshot.host_id
            || after > self.snapshot.sequence
            || self
                .events
                .front()
                .is_some_and(|(event, _)| after < event.sequence - 1)
        {
            return Response::error(
                "resync_required",
                "event cursor expired or belongs to another host; read snapshot",
            );
        }
        let mut size = 0;
        let events = self
            .events
            .iter()
            .filter(|(event, _)| event.sequence > after)
            .take_while(|(_, bytes)| {
                size += bytes + 128;
                size < MAX_FRAME / 2
            })
            .map(|(event, _)| event.clone())
            .collect();
        Response::Events(events)
    }

    pub fn pending(&self, id: &Value) -> bool {
        self.pending_request(id).is_some()
    }

    pub fn pending_request(&self, id: &Value) -> Option<&Value> {
        self.snapshot
            .pending_requests
            .iter()
            .find(|request| request.get("id") == Some(id))
    }

    pub fn answered(&mut self, id: &Value) {
        self.snapshot
            .pending_requests
            .retain(|request| request.get("id") != Some(id));
    }

    pub fn fail(&mut self, reason: String) {
        self.fail_in_scope(reason, FaultScope::Host);
    }

    pub fn fail_provider(&mut self, reason: String) {
        self.fail_in_scope(reason, FaultScope::Provider);
    }

    fn fail_in_scope(&mut self, reason: String, scope: FaultScope) {
        self.stopping.fail(&reason);
        self.queries.clear();
        for operation in &mut self.operations {
            if matches!(operation.outcome, Outcome::Accepted) && scope.includes(&operation.method) {
                operation.outcome = Outcome::Unknown(reason.clone());
            }
        }
        // A later provider error must never downgrade a storage/Host failure.
        if self.snapshot.fault_scope == Some(FaultScope::Host) {
            return;
        }
        self.snapshot.fault = Some(reason);
        self.snapshot.fault_scope = Some(scope);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn claim(state: &mut State, expected: u64) -> Lease {
        match state.claim(
            PROTOCOL_VERSION,
            state.snapshot.host_id,
            expected,
            Uuid::new_v4(),
        ) {
            Response::Lease(lease) => lease,
            other => panic!("expected lease: {other:?}"),
        }
    }

    #[test]
    fn start_reply_identifies_activity_before_the_started_notification() {
        let mut state = State::new(42, json!({}));
        let lease = claim(&mut state, 0);
        let id = Uuid::new_v4();
        state
            .admit(&lease, id, "turn/start".into(), json!({"threadId":"s"}))
            .unwrap();
        state.record(json!({"id":id.to_string(),"result":{"turn":{"id":"target","status":"inProgress"}}})).unwrap();
        assert_eq!(
            state.snapshot().activity.turns,
            vec![lifecycle::ActiveTurn {
                thread_id: "s".into(),
                turn_id: Some("target".into())
            }]
        );
        state.record(json!({"method":"thread/status/changed","params":{"threadId":"s","status":{"type":"idle"}}})).unwrap();
        state
            .record(
                json!({"method":"turn/started","params":{"threadId":"s","turn":{"id":"target"}}}),
            )
            .unwrap();
        assert_eq!(
            state.snapshot().activity.turns[0].turn_id.as_deref(),
            Some("target")
        );
    }

    #[test]
    fn completion_before_start_reply_cannot_reactivate_the_finished_turn() {
        let mut state = State::new(42, json!({}));
        let lease = claim(&mut state, 0);
        let id = Uuid::new_v4();
        state
            .admit(&lease, id, "turn/start".into(), json!({"threadId":"s"}))
            .unwrap();
        state
            .record(
                json!({"method":"turn/started","params":{"threadId":"s","turn":{"id":"target"}}}),
            )
            .unwrap();
        state.record(json!({"method":"turn/completed","params":{"threadId":"s","turn":{"id":"target","status":"completed"}}})).unwrap();
        state
            .record(
                json!({"method":"turn/started","params":{"threadId":"s","turn":{"id":"newer"}}}),
            )
            .unwrap();
        state.record(json!({"id":id.to_string(),"result":{"turn":{"id":"target","status":"inProgress"}}})).unwrap();
        assert_eq!(
            state.snapshot().activity.turns,
            vec![lifecycle::ActiveTurn {
                thread_id: "s".into(),
                turn_id: Some("newer".into())
            }]
        );
    }

    #[test]
    fn queries_do_not_consume_or_evict_mutation_identities() {
        let mut state = State::new(42, json!({}));
        let lease = claim(&mut state, 0);
        let first = Uuid::new_v4();
        state
            .admit(&lease, first, "turn/start".into(), json!({"input":"once"}))
            .unwrap();
        for _ in 1..24 {
            state
                .admit(&lease, Uuid::new_v4(), "thread/resume".into(), json!({}))
                .unwrap();
        }
        for _ in 0..MAX_OPERATIONS * 2 {
            let id = state.admit_query(&lease, "thread/items/list").unwrap();
            let reply = state
                .record(json!({"id":format!("query:{id}"),"result":{"data":[]}}))
                .unwrap()
                .unwrap();
            assert_eq!(reply.id, id);
            assert_eq!(reply.sequence, state.snapshot.sequence);
            assert_eq!(reply.message["result"], json!({"data":[]}));
            assert!(state.queries.is_empty());
        }
        assert_eq!(state.snapshot.operation_ids.len(), 24);
        assert!(
            !state
                .admit(&lease, first, "turn/start".into(), json!({"input":"once"}))
                .unwrap()
        );
        while state
            .admit(&lease, Uuid::new_v4(), "turn/start".into(), json!({}))
            .is_ok()
        {}
        assert!(state.operations.len() < MAX_OPERATIONS);
    }

    #[test]
    fn queries_are_fenced_allowlisted_and_bounded_until_provider_completion() {
        let mut state = State::new(42, json!({}));
        let first = claim(&mut state, 0);
        for method in ["thread/resume", "turn/start", "command/exec", "future/read"] {
            assert_eq!(
                state.admit_query(&first, method).unwrap_err().code,
                "not_query"
            );
        }
        for method in [
            "thread/list",
            "model/list",
            "thread/read",
            "thread/turns/list",
            "thread/items/list",
        ] {
            assert!(is_query_method(method));
        }
        let second = claim(&mut state, 1);
        assert_eq!(
            state.admit_query(&first, "thread/read").unwrap_err().code,
            "stale_controller"
        );
        let id = state.admit_query(&second, "thread/read").unwrap();
        for _ in 1..MAX_QUERIES {
            state.admit_query(&second, "thread/read").unwrap();
        }
        assert_eq!(
            state.admit_query(&second, "thread/read").unwrap_err().code,
            "query_capacity"
        );
        assert!(
            state
                .record(json!({"id":format!("query:{id}"),"result":{},"error":{}}))
                .is_err()
        );
        assert_eq!(state.queries.len(), MAX_QUERIES);
        state
            .record(json!({"id":format!("query:{id}"),"error":{"message":"read failed"}}))
            .unwrap();
        assert!(state.admit_query(&second, "thread/read").is_ok());
        assert!(
            state
                .admit(&second, Uuid::new_v4(), "turn/start".into(), json!({}))
                .unwrap()
        );
        state.fail_provider("provider exited".into());
        assert!(state.queries.is_empty());
        assert_eq!(
            state.admit_query(&second, "thread/read").unwrap_err().code,
            "provider_unavailable"
        );
    }

    #[test]
    fn query_response_namespace_cannot_complete_a_mutation_with_the_same_uuid() {
        let mut state = State::new(42, json!({}));
        let lease = claim(&mut state, 0);
        let id = state.admit_query(&lease, "thread/read").unwrap();
        state
            .admit(&lease, id, "turn/start".into(), json!({}))
            .unwrap();
        let message = json!({"id":format!("query:{id}"),"result":{}});
        state.record(message.clone()).unwrap();
        assert!(matches!(
            state.operation(id),
            Response::Operation(Operation {
                outcome: Outcome::Accepted,
                ..
            })
        ));
        assert!(state.record(message).is_err());
        state
            .record(json!({"id":id.to_string(),"result":{}}))
            .unwrap();
        assert!(matches!(
            state.operation(id),
            Response::Operation(Operation {
                outcome: Outcome::Finished(_),
                ..
            })
        ));
    }

    #[test]
    fn provider_limits_preserve_deliverable_snapshots_and_event_pages() {
        let mut state = State::new(42, json!({"userAgent":"synthetic-provider/1"}));
        let payload = "x".repeat(MAX_PROVIDER_FRAME - 150);
        for id in 0..2 {
            state
                .record(json!({"id":id,"method":"approval","params":payload}))
                .unwrap();
        }
        assert!(
            state
                .record(json!({"id":2,"method":"approval","params":payload}))
                .is_err()
        );
        assert!(
            serde_json::to_vec(&Response::Snapshot(state.snapshot()))
                .unwrap()
                .len()
                < MAX_FRAME
        );
        assert!(
            matches!(state.events(state.snapshot.host_id, 0), Response::Events(events) if !events.is_empty())
        );
        assert!(
            state
                .record(json!({"method":"delta","params":"x".repeat(MAX_PROVIDER_FRAME)}))
                .is_err()
        );
        assert!(
            state
                .record(json!({"id":0,"method":"other","params":{}}))
                .is_err()
        );
    }

    #[test]
    fn handoff_fences_old_writes_and_preserves_accepted_result() {
        let mut state = State::new(42, json!({"userAgent":"synthetic-provider/1"}));
        let first = claim(&mut state, 0);
        let id = Uuid::new_v4();
        assert!(
            state
                .admit(&first, id, "turn/start".into(), json!({"input":"one"}))
                .unwrap()
        );
        let second = claim(&mut state, 1);
        assert!(
            state
                .admit(&first, Uuid::new_v4(), "turn/interrupt".into(), json!({}))
                .is_err()
        );
        assert!(
            !state
                .admit(&second, id, "turn/start".into(), json!({"input":"one"}))
                .unwrap()
        );
        state
            .record(json!({"id":id.to_string(),"result":{"turn":"same"}}))
            .unwrap();
        assert!(matches!(
            state.operation(id),
            Response::Operation(Operation {
                outcome: Outcome::Finished(_),
                ..
            })
        ));
        assert_eq!(state.snapshot().provider_pid, 42);
        let Response::Operation(operation) = state.operation(id) else {
            panic!("missing operation")
        };
        assert_eq!(
            operation.completed_sequence,
            Some(state.snapshot().sequence)
        );
        assert!(matches!(
            state.claim(PROTOCOL_VERSION, second.host_id, 1, Uuid::new_v4()),
            Response::Error { .. }
        ));
    }

    #[test]
    fn approval_is_not_resolved_by_reconnect_or_handoff() {
        let mut state = State::new(42, json!({"userAgent":"synthetic-provider/1"}));
        let first = claim(&mut state, 0);
        state
            .record(json!({"id":7,"method":"item/commandExecution/requestApproval","params":{}}))
            .unwrap();
        let second = claim(&mut state, 1);
        assert!(state.authorize(&first).is_err());
        assert!(state.authorize(&second).is_ok());
        assert!(state.pending(&json!(7)));
        state.answered(&json!(7));
        assert!(!state.pending(&json!(7)));
    }

    #[test]
    fn provider_resolution_removes_only_the_matching_pending_approval() {
        let mut state = State::new(42, json!({"userAgent":"synthetic-provider/1"}));
        for id in [7, 8] {
            state.record(json!({"id":id,"method":"item/commandExecution/requestApproval","params":{"threadId":"t"}})).unwrap();
        }
        assert!(state.record(json!({"method":"serverRequest/resolved","params":{"threadId":"other","requestId":7}})).is_err());
        assert!(state.pending(&json!(7)));
        state
            .record(
                json!({"method":"serverRequest/resolved","params":{"threadId":"t","requestId":7}}),
            )
            .unwrap();
        assert!(!state.pending(&json!(7)));
        assert!(state.pending(&json!(8)));
        state
            .record(
                json!({"method":"serverRequest/resolved","params":{"threadId":"t","requestId":7}}),
            )
            .unwrap();
        assert!(
            state
                .record(json!({"method":"serverRequest/resolved","params":{}}))
                .is_err()
        );
    }

    #[test]
    fn provider_fault_preserves_terminal_admission_receipts_and_owner_fences() {
        let mut state = State::new(42, json!({}));
        let old = claim(&mut state, 0);
        let lease = claim(&mut state, 1);
        let terminal = Uuid::new_v4();
        let provider = Uuid::new_v4();
        state
            .admit(
                &lease,
                terminal,
                "fleetpane/terminal/input".into(),
                json!({}),
            )
            .unwrap();
        state
            .admit(&lease, provider, "turn/start".into(), json!({}))
            .unwrap();
        state.fail_provider("provider exited".into());
        assert_eq!(state.snapshot().fault_scope, Some(FaultScope::Provider));
        assert!(matches!(
            state.operation(terminal),
            Response::Operation(Operation {
                outcome: Outcome::Accepted,
                ..
            })
        ));
        assert!(matches!(
            state.operation(provider),
            Response::Operation(Operation {
                outcome: Outcome::Unknown(_),
                ..
            })
        ));
        assert!(
            state.record(json!({"id":terminal,"result":{}})).is_err(),
            "provider cannot finish a terminal receipt"
        );
        state
            .record_from(
                json!({"id":terminal,"result":{}}),
                OperationTarget::Terminal,
            )
            .unwrap();
        assert_eq!(
            state
                .admit(
                    &old,
                    Uuid::new_v4(),
                    "fleetpane/terminal/close".into(),
                    json!({})
                )
                .unwrap_err()
                .code,
            "stale_controller"
        );
        for method in ["turn/start", "fleetpane/terminal/future"] {
            assert_eq!(
                state
                    .admit(&lease, Uuid::new_v4(), method.into(), json!({}))
                    .unwrap_err()
                    .code,
                "provider_unavailable"
            );
        }
        let close = Uuid::new_v4();
        state
            .admit(&lease, close, "fleetpane/terminal/close".into(), json!({}))
            .unwrap();
        state.fail("journal failed".into());
        state.fail_provider("later provider error".into());
        assert_eq!(state.snapshot().fault_scope, Some(FaultScope::Host));
        assert_eq!(state.fault(), Some("journal failed"));
        assert!(matches!(
            state.operation(close),
            Response::Operation(Operation {
                outcome: Outcome::Unknown(_),
                ..
            })
        ));
        assert_eq!(
            state
                .admit(
                    &lease,
                    Uuid::new_v4(),
                    "fleetpane/terminal/close".into(),
                    json!({})
                )
                .unwrap_err()
                .code,
            "host_unavailable"
        );
        assert!(
            state.authorize_controller(&lease).is_ok(),
            "observational local reads remain available"
        );
    }

    #[test]
    fn completions_cannot_cross_operation_targets() {
        let mut state = State::new(42, json!({}));
        let lease = claim(&mut state, 0);
        let id = Uuid::new_v4();
        state
            .admit(&lease, id, "turn/start".into(), json!({}))
            .unwrap();
        assert!(
            state
                .record_from(json!({"id":id,"result":{}}), OperationTarget::Terminal)
                .is_err()
        );
        assert!(
            state
                .record_from(
                    json!({"method":"delta","params":{}}),
                    OperationTarget::Terminal
                )
                .is_err()
        );
        state.record(json!({"id":id,"result":{}})).unwrap();
    }

    #[test]
    fn expired_cursor_and_conflicting_id_do_not_silently_lose_or_repeat_work() {
        let mut state = State::new(42, json!({"userAgent":"synthetic-provider/1"}));
        let lease = claim(&mut state, 0);
        for _ in 0..129 {
            state
                .record(json!({"method":"delta","params":"x"}))
                .unwrap();
        }
        assert!(
            matches!(state.events(lease.host_id, 0), Response::Error { code, .. } if code == "resync_required")
        );
        let id = Uuid::new_v4();
        state
            .admit(&lease, id, "test".into(), json!({"a":1}))
            .unwrap();
        assert!(
            state
                .admit(&lease, id, "test".into(), json!({"a":2}))
                .is_err()
        );
        state.fail_provider("provider exited".into());
        assert!(matches!(
            state.operation(id),
            Response::Operation(Operation {
                outcome: Outcome::Unknown(_),
                ..
            })
        ));
        assert!(
            state
                .admit(&lease, Uuid::new_v4(), "test".into(), json!({}))
                .is_err()
        );
    }
}
