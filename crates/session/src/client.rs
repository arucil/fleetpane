//! Local Unix Host adapter. No provider process is launched or terminated here.

use crate::MAX_PROMPT_BYTES;
use fleetpane_runtime::{
    Event, Lease, Operation, Outcome, PROTOCOL_VERSION, Request, Response, Snapshot, unix::request,
};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("host transport: {0}")]
    Transport(String),
    #[error("{code}: {message}")]
    Rejected { code: String, message: String },
    #[error("operation {0} may have been accepted; check its status before sending again")]
    Pending(Uuid),
    #[error("operation {id} has an unknown outcome: {reason}; do not resend automatically")]
    Uncertain { id: Uuid, reason: String },
    #[error(
        "operation {0} has a saved result from an earlier Host; inspect its receipt and reopen the session before continuing"
    )]
    Historical(Uuid),
    #[error("provider rejected operation: {0}")]
    Provider(Value),
    #[error("unexpected host response: {0}")]
    Protocol(String),
    #[error("{0}")]
    Input(String),
}

#[derive(Clone)]
pub struct Connection {
    pub socket: PathBuf,
    pub lease: Lease,
}

#[derive(Clone, Debug)]
pub enum Command {
    Tasks {
        thread_id: String,
    },
    Config {
        thread_id: String,
        config_id: String,
        value: String,
    },
    Mode {
        thread_id: String,
        mode_id: String,
    },
    List(crate::directory::PageRequest),
    Models {
        cursor: Option<String>,
    },
    Mcp {
        cursor: Option<String>,
    },
    Rename {
        thread_id: String,
        name: String,
    },
    Restore {
        thread_id: String,
    },
    Create {
        cwd: String,
    },
    Open {
        thread_id: String,
        resume: bool,
    },
    Fork {
        thread_id: String,
        path: PathBuf,
        cwd: String,
    },
    Inspect {
        thread_id: String,
        resume: bool,
    },
    History(crate::history::Request),
    Send {
        thread_id: String,
        text: String,
        images: Vec<fleetpane_runtime::attachments::Image>,
        options: crate::models::TurnOptions,
    },
    Steer {
        thread_id: String,
        turn_id: String,
        text: String,
    },
    Stop {
        thread_id: String,
        turn_id: String,
    },
}

impl Command {
    pub fn is_composer_input(&self) -> bool {
        matches!(self, Self::Send { .. } | Self::Steer { .. })
    }

    pub fn permitted_by(
        &self,
        admission: &fleetpane_runtime::lifecycle::Admission,
    ) -> Result<bool, Error> {
        let (method, _) = self.rpc()?;
        Ok(fleetpane_runtime::is_query_method(method) || admission.permits(method))
    }

    pub(crate) fn rpc(&self) -> Result<(&'static str, Value), Error> {
        Ok(match self {
            Self::Fork {
                thread_id,
                path,
                cwd,
            } => {
                if thread_id.is_empty()
                    || thread_id.len() > 4096
                    || !path.is_absolute()
                    || !std::path::Path::new(cwd).is_absolute()
                    || cwd.len() > 4096
                {
                    return Err(Error::Input(
                        "Fork requires a session identity and absolute snapshot/workspace paths"
                            .into(),
                    ));
                }
                (
                    "thread/fork",
                    json!({"threadId":thread_id,"path":path,"cwd":cwd,"excludeTurns":true,"deferGoalContinuation":true}),
                )
            }
            Self::Restore { thread_id } => {
                if thread_id.is_empty() || thread_id.len() > 4096 {
                    return Err(Error::Input("Invalid session identity for restore".into()));
                }
                ("thread/unarchive", json!({"threadId":thread_id}))
            }
            Self::Tasks { thread_id } => {
                if thread_id.is_empty() || thread_id.len() > 4096 {
                    return Err(Error::Input("Invalid task-plan session identity".into()));
                }
                (
                    fleetpane_runtime::tasks::READ_METHOD,
                    json!({"threadId":thread_id}),
                )
            }
            Self::Mode { thread_id, mode_id } => (
                "acp/mode/set",
                json!({"threadId":thread_id,"modeId":mode_id}),
            ),
            Self::Config {
                thread_id,
                config_id,
                value,
            } => (
                "acp/config/set",
                json!({"threadId":thread_id,"configId":config_id,"value":value}),
            ),
            Self::History(request) => request
                .rpc()
                .map_err(|error| Error::Input(error.to_string()))?,
            Self::Inspect { thread_id, resume } => {
                if *resume {
                    (
                        "thread/resume",
                        json!({"threadId":thread_id,"excludeTurns":true}),
                    )
                } else {
                    (
                        "thread/read",
                        json!({"threadId":thread_id,"includeTurns":false}),
                    )
                }
            }
            Self::Mcp { cursor } => (
                "mcpServerStatus/list",
                crate::mcp::params(cursor).map_err(|error| Error::Input(error.to_string()))?,
            ),
            Self::Models { cursor } => (
                "model/list",
                crate::models::params(cursor).map_err(|error| Error::Input(error.to_string()))?,
            ),
            Self::List(page) => (
                "thread/list",
                page.params()
                    .map_err(|error| Error::Input(error.to_string()))?,
            ),
            Self::Rename { thread_id, name } => {
                if thread_id.is_empty() || name.trim().is_empty() || name.len() > 4096 {
                    return Err(Error::Input(
                        "Session name must contain text and fit within 4 KiB.".into(),
                    ));
                }
                (
                    "thread/name/set",
                    json!({"threadId":thread_id, "name":name.trim()}),
                )
            }
            Self::Create { cwd } => {
                if !std::path::Path::new(cwd).is_absolute() {
                    return Err(Error::Input(
                        "Choose an absolute workspace directory.".into(),
                    ));
                }
                // Paginated storage persists nested code-mode command items.
                // Empty threads materialize on their first turn; a pre-turn
                // history read is not a capability probe for this contract.
                ("thread/start", json!({"cwd":cwd,"historyMode":"paginated"}))
            }
            Self::Open { thread_id, resume } => {
                if *resume {
                    ("thread/resume", json!({"threadId":thread_id}))
                } else {
                    (
                        "thread/read",
                        json!({"threadId":thread_id,"includeTurns":true}),
                    )
                }
            }
            Self::Send {
                thread_id,
                text,
                images,
                options,
            } => {
                fleetpane_runtime::attachments::validate(images).map_err(Error::Input)?;
                if (text.trim().is_empty() && images.is_empty()) || text.len() > MAX_PROMPT_BYTES {
                    return Err(Error::Input(
                        "Message requires text or images; text must fit within 64 KiB.".into(),
                    ));
                }
                let mut input = Vec::new();
                if !text.trim().is_empty() {
                    input.push(json!({"type":"text","text":text}));
                }
                input.extend(
                    images
                        .iter()
                        .map(|image| json!({"type":"fleetpaneImage","image":image})),
                );
                let mut params = json!({"threadId":thread_id,"input":input});
                if let Some(model) = options.model() {
                    params["model"] = json!(model);
                }
                if let Some(effort) = options.effort() {
                    params["effort"] = json!(effort);
                }
                ("turn/start", params)
            }
            Self::Stop { thread_id, turn_id } => {
                if [thread_id, turn_id]
                    .iter()
                    .any(|id| id.is_empty() || id.len() > 512)
                {
                    return Err(Error::Input(
                        "Stop requires session and turn IDs of 1 to 512 bytes.".into(),
                    ));
                }
                (
                    "turn/interrupt",
                    json!({"threadId":thread_id,"turnId":turn_id}),
                )
            }
            Self::Steer {
                thread_id,
                turn_id,
                text,
            } => {
                if thread_id.is_empty()
                    || turn_id.is_empty()
                    || thread_id.len() > 4096
                    || turn_id.len() > 4096
                    || text.trim().is_empty()
                    || text.len() > MAX_PROMPT_BYTES
                {
                    return Err(Error::Input("Follow-up requires session and turn IDs up to 4 KiB and nonempty text up to 64 KiB.".into()));
                }
                (
                    "turn/steer",
                    json!({"threadId":thread_id,"expectedTurnId":turn_id,"input":[{"type":"text","text":text}]}),
                )
            }
        })
    }

    pub fn validate_completed(&self, completed: &Completed) -> Result<(), Error> {
        if let Self::Mcp { cursor } = self {
            let page: crate::mcp::Page = serde_json::from_value(completed.result.clone())
                .map_err(|error| Error::Protocol(format!("invalid MCP catalog: {error}")))?;
            if cursor.is_some() && page.next_cursor == *cursor {
                return Err(Error::Protocol("MCP discovery repeated its cursor".into()));
            }
            crate::mcp::Catalog::default()
                .apply(&None, page)
                .map_err(|error| Error::Protocol(error.to_string()))?;
        }
        if let Self::Fork { thread_id, cwd, .. } = self {
            let thread: crate::Thread = serde_json::from_value(
                completed
                    .result
                    .get("thread")
                    .cloned()
                    .ok_or_else(|| Error::Protocol("Fork reply lacks a thread".into()))?,
            )
            .map_err(|e| Error::Protocol(e.to_string()))?;
            thread
                .validate()
                .map_err(|error| Error::Protocol(error.to_string()))?;
            if thread.id == *thread_id
                || thread.id.is_empty()
                || thread.cwd != *cwd
                || !thread.turns.is_empty()
                || thread.status.kind != "idle"
            {
                return Err(Error::Protocol("Fork reply did not create an idle, metadata-only session in the selected workspace; inspect its receipt before proceeding".into()));
            }
        }
        if let Self::Restore { thread_id } = self {
            let thread: crate::Thread = serde_json::from_value(
                completed
                    .result
                    .get("thread")
                    .cloned()
                    .ok_or_else(|| Error::Protocol("Restore reply lacks a thread".into()))?,
            )
            .map_err(|error| Error::Protocol(error.to_string()))?;
            thread
                .validate()
                .map_err(|error| Error::Protocol(error.to_string()))?;
            if thread.id != *thread_id {
                return Err(Error::Protocol(
                    "Restore reply belongs to another session; do not resend automatically".into(),
                ));
            }
        }
        if let Self::Mode { .. } = self {
            let modes: fleetpane_runtime::acp::modes::State = serde_json::from_value(
                completed
                    .result
                    .get("modes")
                    .cloned()
                    .ok_or_else(|| Error::Protocol("ACP mode result lacks state".into()))?,
            )
            .map_err(|error| Error::Protocol(format!("invalid ACP mode result: {error}")))?;
            modes.validate().map_err(Error::Protocol)?;
        }
        if let Self::Steer { turn_id, .. } = self {
            validate_steer_ack(&completed.result, turn_id)?;
        }
        Ok(())
    }
}

pub(crate) fn validate_steer_ack(result: &Value, expected: &str) -> Result<(), Error> {
    if result.get("turnId").and_then(Value::as_str) != Some(expected) {
        return Err(Error::Protocol("Follow-up acknowledgement does not match the expected active turn; do not resend automatically.".into()));
    }
    Ok(())
}

pub struct Completed {
    pub result: Value,
    pub sequence: u64,
}

async fn call(socket: &std::path::Path, command: Request) -> Result<Response, Error> {
    let observational = matches!(
        command,
        Request::Snapshot | Request::Events { .. } | Request::Query { .. }
    );
    let response = request(socket, &command).await.map_err(|error| {
        if observational
            && (error.downcast_ref::<serde_json::Error>().is_some()
                || error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::InvalidData))
        {
            Error::Protocol(error.to_string())
        } else {
            Error::Transport(error.to_string())
        }
    })?;
    match response {
        Response::Error { code, message } => Err(Error::Rejected { code, message }),
        response => Ok(response),
    }
}

pub async fn snapshot(socket: &std::path::Path) -> Result<Snapshot, Error> {
    match call(socket, Request::Snapshot).await? {
        Response::Snapshot(snapshot) if snapshot.protocol == PROTOCOL_VERSION => Ok(snapshot),
        _ => Err(Error::Protocol(
            "snapshot or supported protocol missing".into(),
        )),
    }
}

impl Connection {
    /// Return the admission receipt without waiting for provider acknowledgement.
    /// Stop progress remains authoritative in the Host snapshot.
    pub async fn request_stop(
        &self,
        command: &Command,
        operation_id: Uuid,
    ) -> Result<Operation, Error> {
        if !matches!(command, Command::Stop { .. }) {
            return Err(Error::Input(
                "Stop admission requires a Stop command".into(),
            ));
        }
        let (method, params) = command.rpc()?;
        self.check_stop_capability().await?;
        self.admit_rpc(method, params, operation_id).await
    }

    async fn check_stop_capability(&self) -> Result<(), Error> {
        let state = snapshot(&self.socket).await?;
        if state.activity.stopping.is_none() {
            return Err(Error::Input("This Host does not report Stop progress. Select an updated Host before stopping a turn.".into()));
        }
        if state.owner.as_ref() != Some(&self.lease) {
            return Err(Error::Rejected {
                code: "stale_controller".into(),
                message: "Controller ownership changed before Stop; refresh before continuing."
                    .into(),
            });
        }
        Ok(())
    }
    /// A Host-level admission fence, committed before the returned confirmation.
    /// On transport loss reread snapshot; do not infer whether it committed.
    pub async fn set_admission(
        &self,
        expected_generation: u64,
        mode: fleetpane_runtime::lifecycle::Mode,
    ) -> Result<fleetpane_runtime::lifecycle::Admission, Error> {
        match call(
            &self.socket,
            Request::SetAdmission {
                lease: self.lease.clone(),
                expected_generation,
                mode,
            },
        )
        .await?
        {
            Response::Admission(admission) => Ok(admission),
            _ => Err(Error::Protocol("expected admission confirmation".into())),
        }
    }

    /// Explicit CAS claim. Never called as automatic recovery from stale ownership.
    pub async fn claim(socket: PathBuf, state: &Snapshot) -> Result<Self, Error> {
        let response = call(
            &socket,
            Request::Claim {
                protocol: PROTOCOL_VERSION,
                host_id: state.host_id,
                expected_generation: state.owner.as_ref().map_or(0, |owner| owner.generation),
                controller_id: Uuid::new_v4(),
            },
        )
        .await?;
        match response {
            Response::Lease(lease) => Ok(Self { socket, lease }),
            _ => Err(Error::Protocol("expected controller lease".into())),
        }
    }

    pub async fn execute(&self, command: &Command, operation_id: Uuid) -> Result<Completed, Error> {
        let (method, params) = command.rpc()?;
        if matches!(command, Command::Stop { .. }) {
            self.check_stop_capability().await?;
        }
        let completed = self.execute_rpc(method, params, operation_id).await?;
        command
            .validate_completed(&completed)
            .map_err(|error| Error::Uncertain {
                id: operation_id,
                reason: error.to_string(),
            })?;
        Ok(completed)
    }

    pub(crate) async fn execute_rpc(
        &self,
        method: &str,
        params: Value,
        operation_id: Uuid,
    ) -> Result<Completed, Error> {
        if fleetpane_runtime::is_query_method(method) {
            // Reads have no side effects to reconcile. A failed read is visible and
            // can be explicitly refreshed; it never creates a pending mutation.
            return match call(
                &self.socket,
                Request::Query {
                    lease: self.lease.clone(),
                    method: method.into(),
                    params,
                },
            )
            .await?
            {
                Response::Query { message, sequence } => provider_result(message, sequence),
                _ => Err(Error::Protocol("expected query result".into())),
            };
        }
        let mut operation = self.admit_rpc(method, params, operation_id).await?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            if !matches!(operation.outcome, Outcome::Accepted) {
                return completed(operation);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(Error::Pending(operation_id));
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
            operation = match self.operation(operation_id).await {
                Ok(operation) => operation,
                Err(_) => return Err(Error::Pending(operation_id)),
            };
        }
    }

    async fn admit_rpc(
        &self,
        method: &str,
        params: Value,
        operation_id: Uuid,
    ) -> Result<Operation, Error> {
        let response = call(
            &self.socket,
            Request::Execute {
                lease: self.lease.clone(),
                operation_id,
                method: method.into(),
                params,
            },
        )
        .await;
        match response {
            Ok(Response::Operation(operation)) => Ok(operation),
            Ok(_) => Err(Error::Protocol("expected operation admission".into())),
            Err(Error::Transport(_)) => Err(Error::Pending(operation_id)),
            Err(error) => Err(error),
        }
    }

    pub async fn operation(&self, operation_id: Uuid) -> Result<Operation, Error> {
        match call(&self.socket, Request::Operation { operation_id }).await? {
            Response::Operation(operation) => Ok(operation),
            _ => Err(Error::Protocol("expected operation status".into())),
        }
    }

    pub async fn poll(&self, after: u64) -> Result<(Snapshot, Vec<Event>), Error> {
        let state = snapshot(&self.socket).await?;
        if state.host_id != self.lease.host_id {
            return Err(Error::Protocol(
                "Host incarnation changed; reconnect explicitly".into(),
            ));
        }
        let events = match call(
            &self.socket,
            Request::Events {
                host_id: self.lease.host_id,
                after,
            },
        )
        .await?
        {
            Response::Events(events) => events,
            _ => return Err(Error::Protocol("expected ordered events".into())),
        };
        Ok((state, events))
    }

    /// The two Codex execution/file approval contracts.
    pub async fn answer(&self, pending: &Value, accept: bool) -> Result<(), Error> {
        let method = pending.get("method").and_then(Value::as_str);
        if !matches!(
            method,
            Some("item/commandExecution/requestApproval" | "item/fileChange/requestApproval")
        ) {
            return Err(Error::Input(
                "This provider request needs a response UI that is not implemented yet.".into(),
            ));
        }
        let id = pending
            .get("id")
            .ok_or_else(|| Error::Protocol("approval request has no ID".into()))?;
        self.respond(
            id.clone(),
            json!({"decision":if accept { "accept" } else { "decline" }}),
        )
        .await
    }

    pub async fn answer_questions(
        &self,
        request: &crate::questions::Request,
        response: &crate::questions::Response,
    ) -> Result<(), Error> {
        request
            .validate_response(response)
            .map_err(|error| Error::Input(error.to_string()))?;
        self.respond(
            request.id.clone(),
            serde_json::to_value(response).map_err(|error| Error::Protocol(error.to_string()))?,
        )
        .await
    }

    pub async fn answer_permission(&self, pending: &Value, choice: &str) -> Result<(), Error> {
        let request = crate::acp::Permission::decode(pending)
            .map_err(Error::Input)?
            .ok_or_else(|| Error::Input("Not an ACP permission request".into()))?;
        if !request.options.iter().any(|o| o.option_id == choice) {
            return Err(Error::Input(
                "This ACP permission choice is no longer offered".into(),
            ));
        }
        let id = pending
            .get("id")
            .cloned()
            .ok_or_else(|| Error::Protocol("ACP permission lacks ID".into()))?;
        self.respond(
            id,
            json!({"outcome":{"outcome":"selected","optionId":choice}}),
        )
        .await
    }

    async fn respond(&self, id: Value, result: Value) -> Result<(), Error> {
        match call(
            &self.socket,
            Request::Respond {
                lease: self.lease.clone(),
                request_id: id,
                result,
            },
        )
        .await?
        {
            Response::Answered => Ok(()),
            _ => Err(Error::Protocol("expected answer confirmation".into())),
        }
    }
}

pub fn completed(operation: Operation) -> Result<Completed, Error> {
    let id = operation.id;
    match operation.outcome {
        Outcome::Accepted => Err(Error::Pending(id)),
        Outcome::Unknown(reason) => Err(Error::Uncertain { id, reason }),
        Outcome::Finished(message) => {
            let sequence = operation.completed_sequence.ok_or(Error::Historical(id))?;
            provider_result(message, sequence)
        }
    }
}

fn provider_result(message: Value, sequence: u64) -> Result<Completed, Error> {
    if let Some(error) = message.get("error") {
        return Err(Error::Provider(error.clone()));
    }
    let result = message
        .get("result")
        .ok_or_else(|| Error::Protocol("provider response has no result".into()))?;
    Ok(Completed {
        result: result.clone(),
        sequence,
    })
}

#[cfg(test)]
mod tests {
    #[tokio::test(flavor = "current_thread")]
    async fn read_recovery_preserves_cursor_and_rejects_invalid_wire_data() {
        use super::*;
        use tokio::{
            io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
            net::UnixListener,
        };
        // macOS TMPDIR can exceed sockaddr_un's address bound before our suffix.
        let directory =
            std::path::PathBuf::from("/tmp").join(format!("fp-read-recovery-{}", Uuid::new_v4()));
        fleetpane_runtime::unix::prepare_directory(&directory).unwrap();
        let socket = directory.join("control.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let mut state = fleetpane_runtime::State::new(1, json!({}));
        let Response::Lease(lease) = state.claim(
            PROTOCOL_VERSION,
            state.snapshot().host_id,
            0,
            Uuid::new_v4(),
        ) else {
            panic!("claim")
        };
        let connection = Connection {
            socket: socket.clone(),
            lease,
        };
        for index in 0..8 {
            state
                .record(json!({"method":"fixture/read","params":{"index":index}}))
                .unwrap();
        }
        let expected = state.snapshot();
        let server = tokio::spawn(async move {
            for step in 0..8 {
                let (stream, _) = listener.accept().await.unwrap();
                let mut stream = BufReader::new(stream);
                let mut line = String::new();
                stream.read_line(&mut line).await.unwrap();
                let request: Request = serde_json::from_str(&line).unwrap();
                let reply = match step {
                    0 => b"{}".to_vec(), // Unterminated frame.
                    1 => b"not-json\n".to_vec(),
                    2 => b"{\"response\":\"future\"}\n".to_vec(),
                    3 => Vec::new(), // Transport closes without a response.
                    4 => format!(
                        "{}\n",
                        serde_json::to_string(&Response::Snapshot(state.snapshot())).unwrap()
                    )
                    .into_bytes(),
                    5 => {
                        let Request::Events { host_id, after } = request else {
                            panic!("must replay reads only")
                        };
                        assert_eq!(after, 7);
                        assert_eq!(host_id, state.snapshot().host_id);
                        format!(
                            "{}\n",
                            serde_json::to_string(&state.events(host_id, after)).unwrap()
                        )
                        .into_bytes()
                    }
                    6 => {
                        assert!(matches!(request, Request::Execute { .. }));
                        b"invalid-admission\n".to_vec()
                    }
                    7 => {
                        assert!(matches!(request, Request::Respond { .. }));
                        b"invalid-answer\n".to_vec()
                    }
                    _ => unreachable!(),
                };
                if step < 5 {
                    assert!(matches!(request, Request::Snapshot));
                }
                stream.get_mut().write_all(&reply).await.unwrap();
            }
        });
        for _ in 0..3 {
            assert!(matches!(connection.poll(7).await, Err(Error::Protocol(_))));
        }
        assert!(matches!(connection.poll(7).await, Err(Error::Transport(_))));
        let (snapshot, events) = connection.poll(7).await.unwrap();
        assert_eq!(snapshot.owner, expected.owner);
        assert_eq!(snapshot.sequence, expected.sequence);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sequence, 8);
        assert_eq!(events[0].message.pointer("/params/index"), Some(&json!(7)));
        let id = Uuid::new_v4();
        assert!(
            matches!(connection.admit_rpc("turn/start",json!({}),id).await,Err(Error::Pending(pending)) if pending == id)
        );
        assert!(matches!(
            connection.respond(json!(1), json!({})).await,
            Err(Error::Transport(_))
        ));
        server.await.unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stop_admission_returns_pending_receipt_without_waiting_and_refuses_legacy_host() {
        use super::*;
        use tokio::{
            io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
            net::UnixListener,
        };
        let directory = std::path::PathBuf::from("/tmp")
            .join(format!("fleetpane-stop-client-{}", Uuid::new_v4()));
        fleetpane_runtime::unix::prepare_directory(&directory).unwrap();
        let socket = directory.join("control.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let mut state = fleetpane_runtime::State::new(1, json!({}));
        let Response::Lease(lease) = state.claim(
            PROTOCOL_VERSION,
            state.snapshot().host_id,
            0,
            Uuid::new_v4(),
        ) else {
            panic!("claim")
        };
        let connection = Connection {
            socket: socket.clone(),
            lease: lease.clone(),
        };
        let operation_id = Uuid::new_v4();
        let server = tokio::spawn(async move {
            for step in 0..3 {
                let (stream, _) = listener.accept().await.unwrap();
                let mut stream = BufReader::new(stream);
                let mut line = String::new();
                stream.read_line(&mut line).await.unwrap();
                let request: Request = serde_json::from_str(&line).unwrap();
                let reply = if step == 1 {
                    let Request::Execute {
                        operation_id: id,
                        method,
                        params,
                        ..
                    } = request
                    else {
                        panic!("must submit Stop, never poll operation")
                    };
                    assert_eq!(id, operation_id);
                    assert_eq!(method, "turn/interrupt");
                    assert_eq!(params, json!({"threadId":"session","turnId":"turn"}));
                    Response::Operation(Operation {
                        id,
                        method,
                        params,
                        outcome: Outcome::Accepted,
                        completed_sequence: None,
                    })
                } else {
                    assert!(matches!(request, Request::Snapshot));
                    let mut snapshot = state.snapshot();
                    if step == 2 {
                        snapshot.activity.stopping = None;
                    }
                    Response::Snapshot(snapshot)
                };
                stream
                    .get_mut()
                    .write_all(format!("{}\n", serde_json::to_string(&reply).unwrap()).as_bytes())
                    .await
                    .unwrap();
            }
        });
        let command = Command::Stop {
            thread_id: "session".into(),
            turn_id: "turn".into(),
        };
        let operation = tokio::time::timeout(
            Duration::from_secs(2),
            connection.request_stop(&command, operation_id),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(operation.id, operation_id);
        assert!(matches!(operation.outcome, Outcome::Accepted));
        assert!(
            matches!(connection.request_stop(&command,Uuid::new_v4()).await,Err(Error::Input(message)) if message.contains("does not report Stop progress"))
        );
        server.await.unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn mode_rpc_requires_a_valid_confirmed_state() {
        use super::*;
        let command = Command::Mode {
            thread_id: "s".into(),
            mode_id: "code".into(),
        };
        assert_eq!(
            command.rpc().unwrap(),
            ("acp/mode/set", json!({"threadId":"s","modeId":"code"}))
        );
        for result in [
            json!({}),
            json!({"modes":null}),
            json!({"modes":{"currentModeId":"","availableModes":[]}}),
        ] {
            assert!(
                command
                    .validate_completed(&Completed {
                        result,
                        sequence: 1
                    })
                    .is_err()
            );
        }
        command.validate_completed(&Completed { result:json!({"modes":{"currentModeId":"newer-provider-mode","availableModes":[]}}), sequence:2 }).unwrap();
    }

    #[test]
    fn steer_requires_exact_turn_and_preserves_text_without_new_turn_options() {
        use super::*;
        let text = "追加中文\n  keep indentation ";
        let command = Command::Steer {
            thread_id: "s".into(),
            turn_id: "t".into(),
            text: text.into(),
        };
        let (method, params) = command.rpc().unwrap();
        assert_eq!(method, "turn/steer");
        assert_eq!(
            params,
            json!({"threadId":"s","expectedTurnId":"t","input":[{"type":"text","text":text}]})
        );
        assert!(
            !command
                .permitted_by(&fleetpane_runtime::lifecycle::Admission {
                    mode: fleetpane_runtime::lifecycle::Mode::Draining,
                    generation: 1
                })
                .unwrap()
        );
        for turn in ["t", "wrong", ""] {
            assert_eq!(
                command
                    .validate_completed(&Completed {
                        sequence: 4,
                        result: json!({"turnId":turn})
                    })
                    .is_ok(),
                turn == "t"
            );
        }
        assert!(
            command
                .validate_completed(&Completed {
                    sequence: 4,
                    result: json!({"turn":{"id":"t"}})
                })
                .is_err()
        );
        for (thread, turn, text) in [("", "t", "hi"), ("s", "", "hi"), ("s", "t", " ")] {
            assert!(
                Command::Steer {
                    thread_id: thread.into(),
                    turn_id: turn.into(),
                    text: text.into()
                }
                .rpc()
                .is_err()
            );
        }
    }

    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn lost_read_reply_is_refreshable_but_lost_mutation_reply_keeps_identity() {
        use tokio::{io::AsyncBufReadExt, net::UnixListener};

        let directory =
            std::path::PathBuf::from("/tmp").join(format!("fleetpane-query-{}", Uuid::new_v4()));
        fleetpane_runtime::unix::prepare_directory(&directory).unwrap();
        let socket = directory.join("control.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let mutation_id = Uuid::new_v4();
        let server = tokio::spawn(async move {
            for read in [true, false] {
                let (stream, _) = listener.accept().await.unwrap();
                let mut reader = tokio::io::BufReader::new(stream);
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                let request: Request = serde_json::from_str(&line).unwrap();
                if read {
                    assert!(
                        matches!(request, Request::Query { method, .. } if method == "model/list")
                    );
                } else {
                    assert!(
                        matches!(request, Request::Execute { operation_id, method, .. } if operation_id == mutation_id && method == "thread/name/set")
                    );
                }
                // Drop the control connection after receiving the request, before
                // returning a reply. Only a mutation now needs reconciliation.
            }
        });
        let connection = Connection {
            socket: socket.clone(),
            lease: Lease {
                host_id: Uuid::new_v4(),
                controller_id: Uuid::new_v4(),
                generation: 1,
            },
        };
        assert!(matches!(
            connection
                .execute(&Command::Models { cursor: None }, Uuid::new_v4())
                .await,
            Err(Error::Transport(_))
        ));
        assert!(matches!(
            connection
                .execute(
                    &Command::Rename { thread_id: "synthetic".into(), name: "title".into() },
                    mutation_id,
                )
                .await,
            Err(Error::Pending(id)) if id == mutation_id
        ));
        server.await.unwrap();
        std::fs::remove_file(socket).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn commands_validate_before_provider_submission() {
        let (method, named) = Command::Rename {
            thread_id: "t".into(),
            name: " 中文标题 ".into(),
        }
        .rpc()
        .unwrap();
        assert_eq!(method, "thread/name/set");
        assert_eq!(named["name"], "中文标题");
        for name in [" ".into(), "x".repeat(4097)] {
            assert!(
                Command::Rename {
                    thread_id: "t".into(),
                    name
                }
                .rpc()
                .is_err()
            );
        }
        let (_, create) = Command::Create {
            cwd: "/example".into(),
        }
        .rpc()
        .unwrap();
        assert_eq!(create["historyMode"], "paginated");
        assert!(
            Command::Create {
                cwd: "relative".into()
            }
            .rpc()
            .is_err()
        );
        assert!(
            Command::Send {
                thread_id: "t".into(),
                text: " \n".into(),
                images: Vec::new(),
                options: crate::models::TurnOptions::default()
            }
            .rpc()
            .is_err()
        );
        assert!(
            Command::Send {
                thread_id: "t".into(),
                text: "x".repeat(MAX_PROMPT_BYTES + 1),
                images: Vec::new(),
                options: crate::models::TurnOptions::default()
            }
            .rpc()
            .is_err()
        );
        let (_, params) = Command::Send {
            thread_id: "t".into(),
            text: "你好\nworld".into(),
            images: Vec::new(),
            options: crate::models::TurnOptions::default(),
        }
        .rpc()
        .unwrap();
        assert_eq!(params["input"][0]["text"], "你好\nworld");
        assert!(params.get("model").is_none() && params.get("effort").is_none());
        let (method, params) = Command::Models {
            cursor: Some("opaque".into()),
        }
        .rpc()
        .unwrap();
        assert_eq!(method, "model/list");
        let (mcp_method, mcp_params) = Command::Mcp {
            cursor: Some("opaque".into()),
        }
        .rpc()
        .unwrap();
        assert_eq!(mcp_method, "mcpServerStatus/list");
        assert!(!fleetpane_runtime::is_query_method(mcp_method));
        assert_eq!(
            mcp_params,
            json!({"limit":32,"cursor":"opaque","detail":"toolsAndAuthOnly"})
        );
        assert!(
            !Command::Mcp { cursor: None }
                .permitted_by(&fleetpane_runtime::lifecycle::Admission {
                    mode: fleetpane_runtime::lifecycle::Mode::Draining,
                    ..Default::default()
                })
                .unwrap()
        );
        assert_eq!(
            params,
            json!({"limit":64,"cursor":"opaque","includeHidden":false})
        );
        assert!(
            Command::Models {
                cursor: Some(String::new())
            }
            .rpc()
            .is_err()
        );
        let mut catalog = crate::models::Catalog::default();
        catalog.apply(&None, serde_json::from_value(json!({"data":[{
            "id":"picker-id","model":"wire-model","displayName":"Synthetic",
            "description":"fixture", "hidden":false,"isDefault":true,
            "defaultReasoningEffort":"future", "supportedReasoningEfforts":[{"reasoningEffort":"future","description":"fixture"}]
        }],"nextCursor":null})).unwrap()).unwrap();
        let (_, params) = Command::Send {
            thread_id: "t".into(),
            text: "hi".into(),
            images: Vec::new(),
            options: crate::models::TurnOptions::new(
                &catalog,
                Some("wire-model".into()),
                Some("future".into()),
            )
            .unwrap(),
        }
        .rpc()
        .unwrap();
        assert_eq!(params["model"], "wire-model");
        assert_eq!(params["effort"], "future");
    }

    #[test]
    fn uncertain_outcomes_keep_identity_and_ordered_reads_require_boundary() {
        let id = Uuid::new_v4();
        let operation = |outcome, sequence| Operation {
            id,
            method: "thread/read".into(),
            params: json!({}),
            outcome,
            completed_sequence: sequence,
        };
        assert!(
            matches!(completed(operation(Outcome::Accepted, None)), Err(Error::Pending(actual)) if actual == id)
        );
        assert!(
            matches!(completed(operation(Outcome::Unknown("provider exited".into()), None)), Err(Error::Uncertain { id: actual, reason }) if actual == id && reason == "provider exited")
        );
        assert!(matches!(
            completed(operation(Outcome::Finished(json!({"result": {}})), None)),
            Err(Error::Historical(actual)) if actual == id
        ));
        let reply = completed(operation(
            Outcome::Finished(json!({"result": {"ok": true}})),
            Some(42),
        ))
        .unwrap();
        assert_eq!(reply.sequence, 42);
        assert_eq!(reply.result["ok"], true);
        assert!(matches!(
            completed(operation(
                Outcome::Finished(json!({"error": {"message":"denied"}})),
                Some(43)
            )),
            Err(Error::Provider(_))
        ));
    }
}

#[cfg(test)]
mod restore_tests {
    use super::*;
    #[test]
    fn restore_is_a_fenced_mutation_and_requires_the_original_reply_identity() {
        let command = Command::Restore {
            thread_id: "exact-id".into(),
        };
        let (method, params) = command.rpc().unwrap();
        assert_eq!(method, "thread/unarchive");
        assert_eq!(params, json!({"threadId":"exact-id"}));
        assert!(!fleetpane_runtime::is_query_method(method));
        let draining = fleetpane_runtime::lifecycle::Admission {
            generation: 1,
            mode: fleetpane_runtime::lifecycle::Mode::Draining,
        };
        assert!(!command.permitted_by(&draining).unwrap());
        let mut reply = Completed {
            result: json!({"thread":{"id":"other-id","cwd":"/synthetic","preview":"archived","name":null,"historyMode":"paginated","status":{"type":"notLoaded"},"turns":[]}}),
            sequence: 1,
        };
        assert!(command.validate_completed(&reply).is_err());
        reply.result["thread"]["id"] = json!("exact-id");
        command.validate_completed(&reply).unwrap();
        reply.result = json!({});
        assert!(command.validate_completed(&reply).is_err());
        assert!(
            Command::Restore {
                thread_id: String::new()
            }
            .rpc()
            .is_err()
        );
    }
}
