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
    List(crate::directory::PageRequest),
    Models {
        cursor: Option<String>,
    },
    Rename {
        thread_id: String,
        name: String,
    },
    Create {
        cwd: String,
    },
    Open {
        thread_id: String,
        resume: bool,
    },
    Inspect {
        thread_id: String,
        resume: bool,
    },
    History(crate::history::Request),
    Send {
        thread_id: String,
        text: String,
        options: crate::models::TurnOptions,
    },
    Stop {
        thread_id: String,
        turn_id: String,
    },
}

impl Command {
    pub fn permitted_by(
        &self,
        admission: &fleetpane_runtime::lifecycle::Admission,
    ) -> Result<bool, Error> {
        let (method, _) = self.rpc()?;
        Ok(fleetpane_runtime::is_query_method(method) || admission.permits(method))
    }

    pub(crate) fn rpc(&self) -> Result<(&'static str, Value), Error> {
        Ok(match self {
            Self::Tasks { thread_id } => {
                if thread_id.is_empty() || thread_id.len() > 4096 {
                    return Err(Error::Input("Invalid task-plan session identity".into()));
                }
                (
                    fleetpane_runtime::tasks::READ_METHOD,
                    json!({"threadId":thread_id}),
                )
            }
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
                options,
            } => {
                if text.trim().is_empty() || text.len() > MAX_PROMPT_BYTES {
                    return Err(Error::Input(
                        "Message must contain text and fit within 64 KiB.".into(),
                    ));
                }
                let mut params =
                    json!({"threadId":thread_id,"input":[{"type":"text","text":text}]});
                if let Some(model) = options.model() {
                    params["model"] = json!(model);
                }
                if let Some(effort) = options.effort() {
                    params["effort"] = json!(effort);
                }
                ("turn/start", params)
            }
            Self::Stop { thread_id, turn_id } => (
                "turn/interrupt",
                json!({"threadId":thread_id,"turnId":turn_id}),
            ),
        })
    }
}

pub struct Completed {
    pub result: Value,
    pub sequence: u64,
}

async fn call(socket: &std::path::Path, command: Request) -> Result<Response, Error> {
    let response = request(socket, &command)
        .await
        .map_err(|error| Error::Transport(error.to_string()))?;
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
        let mut operation = match response {
            Ok(Response::Operation(operation)) => operation,
            Ok(_) => return Err(Error::Protocol("expected operation admission".into())),
            Err(Error::Transport(_)) => return Err(Error::Pending(operation_id)),
            Err(error) => return Err(error),
        };
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
                options: crate::models::TurnOptions::default()
            }
            .rpc()
            .is_err()
        );
        assert!(
            Command::Send {
                thread_id: "t".into(),
                text: "x".repeat(MAX_PROMPT_BYTES + 1),
                options: crate::models::TurnOptions::default()
            }
            .rpc()
            .is_err()
        );
        let (_, params) = Command::Send {
            thread_id: "t".into(),
            text: "你好\nworld".into(),
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
