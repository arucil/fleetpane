//! Unix prototype: a host owns one provider connection across controller lifetimes.

use crate::{MAX_FRAME, MAX_PROVIDER_FRAME, Operation, Outcome, Request, Response, State};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io,
    os::unix::fs::{DirBuilderExt, FileTypeExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    process::{ChildStdin, Command},
    sync::{Semaphore, mpsc, oneshot},
    time::timeout,
};

type Error = Box<dyn std::error::Error + Send + Sync>;
const IO_TIMEOUT: Duration = Duration::from_secs(5);

pub struct Config {
    pub socket: PathBuf,
    pub provider: PathBuf,
    pub provider_args: Vec<String>,
    pub cwd: PathBuf,
    pub acp: bool,
}

fn provider_identity(provider: &Path) -> Result<PathBuf, Error> {
    if provider.components().count() > 1 || provider.is_absolute() {
        return Ok(std::fs::canonicalize(provider)?);
    }
    let search = std::env::var_os("PATH").ok_or("provider lookup requires PATH")?;
    for directory in std::env::split_paths(&search) {
        let candidate = directory.join(provider);
        if candidate.is_file() {
            return Ok(std::fs::canonicalize(candidate)?);
        }
    }
    Err("provider executable was not found in PATH".into())
}

type QueryReplies = HashMap<uuid::Uuid, oneshot::Sender<Response>>;

async fn record(
    state: &mut State,
    replies: &mut QueryReplies,
    message: Value,
    target: crate::OperationTarget,
) -> Result<(), Error> {
    state.check_response_target(&message, target)?;
    if message.get("method").is_none()
        && let Some(id) = message.get("id").and_then(Value::as_str)
        && !id.starts_with("query:")
    {
        if serde_json::to_vec(&message)?.len() + 1 > MAX_PROVIDER_FRAME
            || message.get("result").is_some() == message.get("error").is_some()
        {
            return Err("invalid or oversized provider response".into());
        }
        let id = uuid::Uuid::parse_str(id)?;
        let mut operation = state
            .operations
            .iter()
            .find(|op| op.id == id)
            .ok_or("provider replied to an unrecorded operation")?
            .clone();
        if !matches!(operation.outcome, Outcome::Accepted) {
            return Err("provider replied twice to the same operation".into());
        }
        operation.outcome = Outcome::Finished(message.clone());
        operation.completed_sequence = Some(
            state
                .snapshot
                .sequence
                .checked_add(1)
                .ok_or("event sequence exhausted")?,
        );
        if let Err(error) = state
            .journal
            .as_ref()
            .expect("Host journal initialized")
            .finish(operation)
            .await
        {
            state.fail(format!(
                "journal completion failed; result uncertain: {error}"
            ));
            return Err(error);
        }
    }
    if let Some(query) = state.record_from(message, target)? {
        let reply = replies
            .remove(&query.id)
            .ok_or("query response channel missing")?;
        let _disconnected = reply.send(Response::Query {
            message: query.message,
            sequence: query.sequence,
        });
    }
    Ok(())
}

async fn batch(
    state: &mut State,
    stdin: &mut ChildStdin,
    replies: &mut QueryReplies,
    batch: crate::acp::Batch,
) -> Result<(), Error> {
    if state.snapshot.fault_scope == Some(crate::FaultScope::Host) {
        return Err("Host storage is fenced; ACP activity cannot be published".into());
    }
    if !batch.archive.is_empty() {
        let result = state
            .journal
            .as_ref()
            .ok_or("ACP archive needs its Host journal")?
            .archive_write(batch.archive)
            .await;
        if let Err(error) = result {
            state.fail(format!("ACP archive write failed: {error}"));
            return Err(error);
        }
    }
    for message in batch.wire {
        // ACP images are expanded only on this outgoing provider pipe. Control,
        // events, responses and persisted operations keep their existing limits.
        write_frame_limit(
            stdin,
            &message,
            crate::attachments::MAX_IMAGES
                * ((crate::attachments::MAX_IMAGE_BYTES as usize).div_ceil(3) * 4)
                + MAX_FRAME,
        )
        .await?;
    }
    for message in batch.events {
        record(state, replies, message, crate::OperationTarget::Provider).await?;
    }
    Ok(())
}

struct Submission {
    id: String,
    method: String,
    params: Value,
    images: Option<crate::attachments::Prepared>,
}

async fn submit(
    state: &mut State,
    stdin: &mut ChildStdin,
    adapter: &mut Option<crate::acp::Adapter>,
    replies: &mut QueryReplies,
    submission: Submission,
) -> Result<(), Error> {
    let Submission {
        id,
        method,
        mut params,
        images,
    } = submission;
    if let Some(adapter) = adapter {
        let result = if let Some(images) = &images {
            adapter.dispatch_images(id, params, &images.prompt)?
        } else {
            adapter.dispatch(id, &method, params)?
        };
        batch(state, stdin, replies, result).await
    } else {
        if let Some(images) = &images {
            params["input"] = json!(images.prompt.wire);
        }
        write_frame(stdin, &json!({"id":id,"method":method,"params":params})).await
    }
}

struct SocketGuard(PathBuf);
impl Drop for SocketGuard {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.0) {
            eprintln!("host socket cleanup: {error}");
        }
    }
}

pub fn prepare_directory(path: &Path) -> io::Result<()> {
    match std::fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => std::fs::File::open(
            path.parent()
                .ok_or_else(|| io::Error::other("directory needs a parent"))?,
        )?
        .sync_all()?,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "host socket parent must be a private, non-symlink directory (0700)",
        ));
    }
    Ok(())
}

async fn read_frame<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    limit: usize,
) -> Result<Option<Value>, Error> {
    let mut bytes = Vec::new();
    let count = reader
        .take((limit + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .await?;
    if count == 0 {
        return Ok(None);
    }
    if count > limit || bytes.last() != Some(&b'\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unterminated or oversized JSONL frame",
        )
        .into());
    }
    Ok(Some(serde_json::from_slice(&bytes)?))
}

async fn write_frame<W: AsyncWrite + Unpin, T: serde::Serialize>(
    writer: &mut W,
    value: &T,
) -> Result<(), Error> {
    write_frame_limit(writer, value, MAX_FRAME).await
}

async fn write_frame_limit<W: AsyncWrite + Unpin, T: serde::Serialize>(
    writer: &mut W,
    value: &T,
    limit: usize,
) -> Result<(), Error> {
    let mut bytes = serde_json::to_vec(value)?;
    if bytes.len() + 1 > limit {
        return Err("outbound JSONL frame exceeds limit".into());
    }
    bytes.push(b'\n');
    timeout(IO_TIMEOUT, writer.write_all(&bytes)).await??;
    Ok(())
}

struct Call {
    request: Request,
    reply: oneshot::Sender<Response>,
    prepared: PreparedInput,
}

type PreparedInput = Result<Option<crate::attachments::Prepared>, (&'static str, String)>;

async fn prepare_input(
    request: &Request,
    store: &crate::attachments::Store,
    acp: bool,
) -> PreparedInput {
    let Request::Execute { method, params, .. } = request else {
        return Ok(None);
    };
    crate::creation_directory::validate(method, params)
        .await
        .map_err(|error| ("invalid_workspace_directory", error))?;
    crate::attachments::prepare(store, method, params, acp)
        .await
        .map_err(|error| ("invalid_images", error))
}

async fn serve_client(
    stream: UnixStream,
    calls: mpsc::Sender<Call>,
    store: crate::attachments::Store,
    acp: bool,
) -> Result<(), Error> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    while let Some(message) = timeout(
        Duration::from_secs(30),
        read_frame(&mut reader, MAX_PROVIDER_FRAME),
    )
    .await??
    {
        let request: Request = match serde_json::from_value(message) {
            Ok(request) => request,
            Err(error) => {
                write_frame(
                    &mut writer,
                    &Response::error("invalid_request", error.to_string()),
                )
                .await?;
                continue;
            }
        };
        let prepared = prepare_input(&request, &store, acp).await;
        let (reply, response) = oneshot::channel();
        timeout(
            IO_TIMEOUT,
            calls.send(Call {
                request,
                reply,
                prepared,
            }),
        )
        .await??;
        let result = timeout(IO_TIMEOUT, response).await??;
        write_frame(&mut writer, &result).await?;
    }
    Ok(())
}

async fn handle(
    state: &mut State,
    stdin: &mut ChildStdin,
    adapter: &mut Option<crate::acp::Adapter>,
    replies: &mut QueryReplies,
    terminals: &mut crate::terminal::Manager,
    request: Request,
    prepared: PreparedInput,
) -> Response {
    match request {
        Request::Snapshot => {
            let mut snapshot = state.snapshot();
            snapshot.activity.terminal_ids = terminals.live();
            Response::Snapshot(snapshot)
        }
        Request::SetAdmission {
            lease,
            expected_generation,
            mode,
        } => {
            if let Err(error) = state.authorize_host(&lease) {
                return error.into();
            }
            let next = match state.snapshot.admission.change(expected_generation, mode) {
                Ok(next) => next,
                Err(error) => return error.into(),
            };
            if next == state.snapshot.admission {
                return Response::Admission(next);
            }
            match state
                .journal
                .as_ref()
                .expect("Host journal initialized")
                .set_admission(state.snapshot.admission.clone(), next.clone())
                .await
            {
                Ok(()) => {
                    state.snapshot.admission = next.clone();
                    Response::Admission(next)
                }
                Err(error) => {
                    state.fail(format!(
                        "admission persistence failed; no further writes allowed: {error}"
                    ));
                    Response::error("journal_unavailable", error.to_string())
                }
            }
        }
        Request::Claim {
            protocol,
            host_id,
            expected_generation,
            controller_id,
        } => state.claim(protocol, host_id, expected_generation, controller_id),
        Request::Operation { operation_id } => {
            if state.fault().is_some()
                && let Response::Operation(operation) = state.operation(operation_id)
                && matches!(operation.outcome, Outcome::Unknown(_))
            {
                return Response::Operation(operation);
            }
            match state
                .journal
                .as_ref()
                .expect("Host journal initialized")
                .get(operation_id)
                .await
            {
                Ok(Some(operation)) => Response::Operation(operation),
                Ok(None) => Response::error(
                    "unknown_operation",
                    "operation is not recorded in this journal",
                ),
                Err(error) => Response::error("journal_unavailable", error.to_string()),
            }
        }
        Request::Operations { cursor } => {
            match state
                .journal
                .as_ref()
                .expect("Host journal initialized")
                .page(cursor)
                .await
            {
                Ok(page) => Response::Operations(page),
                Err(error) => Response::error("journal_read_failed", error.to_string()),
            }
        }
        Request::Events { host_id, after } => state.events(host_id, after),
        Request::Execute {
            lease,
            operation_id,
            method,
            params,
        } => {
            if let Err(error) = state.authorize_mutation(&lease, &method) {
                return error.into();
            }
            match state
                .journal
                .as_ref()
                .expect("Host journal initialized")
                .get(operation_id)
                .await
            {
                Ok(Some(operation)) => {
                    return if operation.method == method && operation.params == params {
                        Response::Operation(operation)
                    } else {
                        Response::error(
                            "operation_conflict",
                            "operation ID already identifies another command",
                        )
                    };
                }
                Ok(None) => {}
                Err(error) => {
                    state.fail(format!(
                        "journal lookup failed; command not dispatched: {error}"
                    ));
                    return Response::error("journal_unavailable", error.to_string());
                }
            }
            let images = match prepared {
                Ok(images) => images,
                Err((code, error)) => return Response::error(code, error),
            };
            if images.is_some()
                && adapter.is_some()
                && state
                    .snapshot
                    .provider_info
                    .pointer("/agentCapabilities/promptCapabilities/image")
                    != Some(&json!(true))
            {
                return Response::error(
                    "unsupported_images",
                    "ACP provider did not advertise v1 image prompt support",
                );
            }
            match state.admit(&lease, operation_id, method.clone(), params.clone()) {
                Err(error) => error.into(),
                Ok(fresh) => {
                    if fresh {
                        let operation = Operation {
                            id: operation_id,
                            method: method.clone(),
                            params: params.clone(),
                            outcome: Outcome::Accepted,
                            completed_sequence: None,
                        };
                        match state
                            .journal
                            .as_ref()
                            .expect("Host journal initialized")
                            .admit(operation)
                            .await
                        {
                            Ok(total) => state.snapshot.journal_total = total,
                            Err(error) => {
                                state.fail(format!(
                                    "journal admission failed; command not dispatched: {error}"
                                ));
                                return Response::error("journal_unavailable", error.to_string());
                            }
                        }
                    }
                    if fresh && crate::terminal::is_mutation(&method) {
                        terminals.dispatch(operation_id, &method, params);
                        return state.operation(operation_id);
                    }
                    if fresh
                        && let Err(error) = submit(
                            state,
                            stdin,
                            adapter,
                            replies,
                            Submission {
                                id: operation_id.to_string(),
                                method,
                                params,
                                images,
                            },
                        )
                        .await
                    {
                        // A partial write may have been accepted. Never resend it.
                        state.fail_provider(format!("provider write outcome unknown: {error}"));
                    }
                    state.operation(operation_id)
                }
            }
        }
        Request::Query { .. } => unreachable!("queries use deferred responses"),
        Request::Respond {
            lease,
            request_id,
            result,
        } => {
            if let Err(error) = state.authorize(&lease) {
                return error.into();
            }
            let Some(pending) = state.pending_request(&request_id) else {
                return Response::error("unknown_request", "provider request is no longer pending");
            };
            if adapter.is_none()
                && let Err(error) = crate::approvals::validate_response(pending, &result)
            {
                return Response::error("invalid_approval", error);
            }
            let message = if let Some(adapter) = adapter {
                match adapter.permission_response(&request_id, &result) {
                    Ok(message) => message,
                    Err(error) => return Response::error("invalid_permission", error),
                }
            } else {
                json!({"id":request_id,"result":result})
            };
            match write_frame(stdin, &message).await {
                Ok(()) => {
                    state.answered(&request_id);
                    if let Some(adapter) = adapter {
                        adapter.answered(&request_id);
                    }
                    Response::Answered
                }
                Err(error) => {
                    state.fail_provider(format!("approval write outcome unknown: {error}"));
                    Response::error(
                        "outcome_unknown",
                        "approval may have reached provider; not resubmitted",
                    )
                }
            }
        }
    }
}

pub async fn run(config: Config) -> Result<(), Error> {
    let parent = config
        .socket
        .parent()
        .ok_or("socket needs an explicit private parent directory")?;
    prepare_directory(parent)?;
    let mut journal_path = config.socket.as_os_str().to_os_string();
    journal_path.push(".journal");
    let mut state = State::new(0, Value::Null);
    let provider = provider_identity(&config.provider)?;
    let binding = serde_json::to_string(&json!({
        "provider":provider, "args":config.provider_args,
        "cwd":std::fs::canonicalize(&config.cwd)?, "acp":config.acp,
        "codex_home":std::env::var_os("CODEX_HOME"),
        "dsh_home":std::env::var_os("DSH_HOME"),
        "home":std::env::var_os("HOME"),
    }))?;
    let journal =
        crate::journal::Journal::open(PathBuf::from(journal_path), binding, state.snapshot.host_id)
            .await?;
    state.snapshot.journal_total = journal.total().await?;
    state.snapshot.admission = journal.admission().await?;
    state.snapshot.activity.unknown_operations = journal.unknown_count().await?;
    state.journal = Some(journal);
    // Only the exclusive journal owner may remove a refused, regular socket.
    // A live listener, symlink or arbitrary file is never unlinked.
    match std::fs::symlink_metadata(&config.socket) {
        Ok(metadata) if metadata.file_type().is_socket() => {
            match UnixStream::connect(&config.socket).await {
                Ok(_) => return Err("socket already has a live listener".into()),
                Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                    std::fs::remove_file(&config.socket)?
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(_) => return Err("socket path is not a regular Unix socket".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let listener = UnixListener::bind(&config.socket)?;
    let _socket_guard = SocketGuard(config.socket.clone());
    std::fs::set_permissions(&config.socket, std::fs::Permissions::from_mode(0o600))?;

    let mut child = Command::new(&provider)
        .args(&config.provider_args)
        .current_dir(&config.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;
    let pid = child.id().ok_or("provider exited before initialization")?;
    let mut stdin = child.stdin.take().ok_or("provider stdin not piped")?;
    let mut stdout = BufReader::new(child.stdout.take().ok_or("provider stdout not piped")?);
    let initialize = if config.acp {
        json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{},"clientInfo":{"name":"fleetpane","version":env!("CARGO_PKG_VERSION")}}})
    } else {
        json!({"id":0,"method":"initialize","params":{
            "clientInfo":{"name":"fleetpane_host","version":env!("CARGO_PKG_VERSION")},
            "capabilities":{"experimentalApi":true}
        }})
    };
    write_frame(&mut stdin, &initialize).await?;
    let initialized = timeout(
        Duration::from_secs(15),
        read_frame(&mut stdout, MAX_PROVIDER_FRAME),
    )
    .await??
    .ok_or("provider closed during initialize")?;
    if initialized.get("id") != Some(&json!(0)) || initialized.get("result").is_none() {
        return Err(format!("provider rejected initialize: {initialized}").into());
    }
    let mut info = initialized["result"].clone();
    let mut adapter = if config.acp {
        if info["protocolVersion"] != 1
            || info
                .pointer("/agentCapabilities/sessionCapabilities/list")
                .is_none()
            || info
                .pointer("/agentCapabilities/sessionCapabilities/resume")
                .is_none()
        {
            return Err("DeepSeek adapter requires ACP v1 session/list and session/resume".into());
        }
        info["fleetpaneProvider"] = json!("deepseek-acp");
        Some(crate::acp::Adapter::restore(
            state
                .journal
                .as_ref()
                .ok_or("ACP archive needs its Host journal")?
                .archive_read()
                .await?,
        )?)
    } else {
        write_frame(&mut stdin, &json!({"method":"initialized"})).await?;
        info["fleetpaneProvider"] = json!("codex");
        None
    };
    if let Some(adapter) = &adapter {
        for observation in adapter.context_observations() {
            state.usage.record(observation.clone());
        }
    }
    state.snapshot.provider_pid = pid;
    state.snapshot.provider_info = info;
    state.snapshot.provider_info["fleetpaneTerminalVersion"] = json!(1);
    state.snapshot.provider_info["fleetpaneAttachmentVersion"] = json!(1);
    let attachment_store = crate::attachments::Store::for_socket(&config.socket)?;
    let (calls_tx, mut calls_rx) = mpsc::channel::<Call>(32);
    let (events_tx, mut events_rx) = mpsc::channel(16);
    let reader = tokio::spawn(async move {
        loop {
            match read_frame(&mut stdout, MAX_PROVIDER_FRAME).await {
                Ok(Some(message)) => {
                    if events_tx.send(Ok(message)).await.is_err() {
                        break;
                    }
                }
                end => {
                    let reason = match end {
                        Ok(None) => "provider output closed".to_owned(),
                        Err(error) => format!("provider output failed: {error}"),
                        Ok(Some(_)) => unreachable!(),
                    };
                    let _closed = events_tx.send(Err(reason)).await;
                    break;
                }
            }
        }
    });
    let semaphore = Arc::new(Semaphore::new(16));
    let mut clients = tokio::task::JoinSet::new();
    let mut provider_open = true;
    let mut query_replies = HashMap::new();
    let mut fault_recorded = None;
    let mut terminals = crate::terminal::Manager::default();
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    eprintln!(
        "fleetpane-host ready: {} (provider PID {pid})",
        config.socket.display()
    );
    loop {
        tokio::select! {
            result = &mut shutdown => { result?; break; }
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                if let Ok(permit) = semaphore.clone().try_acquire_owned() {
                    let calls = calls_tx.clone();
                    let store = attachment_store.clone();
                    let acp = config.acp;
                    clients.spawn(async move { let _permit = permit; serve_client(stream, calls, store, acp).await });
                }
            }
            Some(call) = calls_rx.recv() => {
                match call.request {
                    Request::Query { lease, method, params } => {
                        if method == crate::terminal::READ_METHOD {
                            let response = match state.authorize_controller(&lease) {
                                Err(error) => error.into(),
                                Ok(()) => match serde_json::from_value::<crate::terminal::Read>(params)
                                    .map_err(|e|e.to_string()).and_then(|read|terminals.read(read)) {
                                    Ok(result) => Response::Query { message:json!({"result":result}), sequence:state.snapshot.sequence },
                                    Err(error) => Response::error("terminal_read",error),
                                }
                            };
                            let _disconnected = call.reply.send(response);
                            continue;
                        }
                        if method == crate::usage::READ_METHOD {
                            let response = match state.authorize_controller(&lease) {
                                Err(error) => error.into(),
                                Ok(()) => match serde_json::from_value::<crate::usage::Read>(params) {
                                    Ok(read) if !read.thread_id.is_empty() && read.thread_id.len() <= 4096 => Response::Query {
                                        message: json!({"result":state.usage.read(&read.thread_id,state.snapshot.sequence)}),
                                        sequence: state.snapshot.sequence,
                                    },
                                    _ => Response::error("invalid_context_usage_read", "expected a nonempty threadId up to 4 KiB"),
                                }
                            };
                            let _disconnected = call.reply.send(response);
                            continue;
                        }
                        if method == crate::tasks::READ_METHOD {
                            let response = match state.authorize_controller(&lease) {
                                Err(error)=>error.into(),
                                Ok(())=>match serde_json::from_value::<crate::tasks::Read>(params) {
                                    Ok(read) if !read.thread_id.is_empty() && read.thread_id.len()<=4096=>Response::Query {
                                        message:serde_json::json!({"result":state.tasks.read(&read.thread_id,state.snapshot.sequence)}),
                                        sequence:state.snapshot.sequence,
                                    },
                                    _=>Response::error("invalid_task_read","expected a nonempty threadId up to 4 KiB"),
                                }
                            };
                            let _disconnected=call.reply.send(response);
                            continue;
                        }
                        match state.admit_query(&lease, &method) {
                            Err(error) => { let _disconnected = call.reply.send(error.into()); }
                            Ok(id) => {
                                query_replies.insert(id, call.reply);
                                match submit(&mut state, &mut stdin, &mut adapter, &mut query_replies, Submission { id:format!("query:{id}"), method, params, images:None }).await {
                                    Ok(()) => {}
                                    Err(error) => {
                                        state.fail_provider(format!("provider write outcome unknown: {error}"));
                                    }
                                }
                            }
                        }
                    }
                    request => {
                        let response = handle(&mut state, &mut stdin, &mut adapter, &mut query_replies, &mut terminals, request, call.prepared).await;
                        // Mutation results remain in host state if a controller disappeared.
                        let _disconnected = call.reply.send(response);
                    }
                }
            }
            Some(completion) = terminals.next(), if terminals.has_pending() => {
                match completion {
                    Ok(message) => if state.snapshot.fault_scope != Some(crate::FaultScope::Host)
                        && let Err(error) = record(&mut state, &mut query_replies, message, crate::OperationTarget::Terminal).await { state.fail(error.to_string()); },
                    Err(error) => state.fail(format!("terminal completion task failed: {error}")),
                }
            }
            event = events_rx.recv(), if provider_open => {
                match event {
                    Some(Ok(message)) => {
                        let outcome = if let Some(adapter) = &mut adapter {
                            match adapter.receive(message) {
                                Ok(messages) => batch(&mut state, &mut stdin, &mut query_replies, messages).await,
                                Err(error) => Err(error.into()),
                            }
                        } else { record(&mut state, &mut query_replies, message, crate::OperationTarget::Provider).await };
                        if let Err(error) = outcome {state.fail_provider(error.to_string());}
                    }
                    Some(Err(reason)) => { state.fail_provider(reason); provider_open = false; }
                    None => { state.fail_provider("provider reader stopped".into()); provider_open = false; }
                }
            }
            Some(result) = clients.join_next(), if !clients.is_empty() => {
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => eprintln!("host client disconnected: {error}"),
                    Err(error) => eprintln!("host client task failed: {error}"),
                }
            }
        }
        if let Some(reason) = state.fault().map(str::to_owned) {
            provider_open = false;
            let scope = state.snapshot.fault_scope.expect("fault has a scope");
            if fault_recorded != Some(scope) {
                if let Err(error) = state
                    .journal
                    .as_ref()
                    .expect("Host journal initialized")
                    .fail(reason.to_owned(), scope)
                    .await
                {
                    state.fail(format!(
                        "journal fault persistence failed; admissions remain uncertain: {error}"
                    ));
                }
                // If persisting a provider fault failed, the Host fault must
                // still be persisted for every target on the next iteration.
                fault_recorded = Some(scope);
                match state
                    .journal
                    .as_ref()
                    .expect("Host journal initialized")
                    .unknown_count()
                    .await
                {
                    Ok(count) => state.snapshot.activity.unknown_operations = count,
                    Err(error) => {
                        state.fail(format!("cannot read uncertain receipt count: {error}"))
                    }
                }
            }
            let code = match state.snapshot.fault_scope.expect("fault has a scope") {
                crate::FaultScope::Provider => "provider_unavailable",
                crate::FaultScope::Host => "host_unavailable",
            };
            for (_, reply) in query_replies.drain() {
                let _disconnected = reply.send(Response::error(
                    code,
                    state.fault().expect("fault has a reason"),
                ));
            }
        }
    }
    clients.abort_all();
    reader.abort();
    state
        .journal
        .as_ref()
        .expect("Host journal initialized")
        .fail(
            "Host shut down before an operation result".into(),
            crate::FaultScope::Host,
        )
        .await?;
    drop(stdin);
    if child.try_wait()?.is_none() {
        child.kill().await?;
    }
    child.wait().await?;
    Ok(())
}

/// One request per connection deliberately exercises replaceable clients.
pub async fn request(socket: &Path, request: &Request) -> Result<Response, Error> {
    let mut stream = timeout(IO_TIMEOUT, UnixStream::connect(socket)).await??;
    write_frame(&mut stream, request).await?;
    let message = timeout(
        IO_TIMEOUT,
        read_frame(&mut BufReader::new(stream), MAX_FRAME),
    )
    .await??
    .ok_or("host closed without a response")?;
    Ok(serde_json::from_value(message)?)
}
