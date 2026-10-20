use std::process::ExitCode;

#[cfg(unix)]
async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use fleetpane_runtime::unix::{Config, request, run};
    use std::{
        io::{self, Read},
        path::PathBuf,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" {
        println!(
            "fleetpane-host serve SOCKET CWD [PROVIDER [ARGS...]]\nfleetpane-host request SOCKET < request.json\nfleetpane-host session SOCKET [--take-control] list [--search TITLE] [--cwd CWD] [--cursor CURSOR]|models [--cursor CURSOR]|rename THREAD NAME|create CWD|open THREAD|send THREAD TEXT [--model MODEL] [--effort EFFORT]|stop THREAD TURN|answer JSON_REQUEST_ID < answers.json\n\nUnix prototype. Defaults to codex app-server --stdio. Keep serve running independently.\nOnly local, same-user trusted controllers. Lifecycle: session SOCKET [--take-control] drain|ready (drain pauses fresh mutations; existing output, reads, Stop and answers continue). Mutation receipts persist beside SOCKET in SOCKET.journal; interrupted admissions are never replayed."
        );
        println!(
            "History: inspect THREAD [--resume] | history-turns THREAD [--cursor CURSOR] | history-items THREAD TURN [--cursor CURSOR]. Native views use these bounded commands; open requests full history."
        );
        println!(
            "fleetpane-host operations SOCKET [--cursor CURSOR] | operation SOCKET UUID: read-only durable receipts.\nfleetpane-host hosts CONFIG.json: read-only parallel snapshots of configured local Hosts, without claiming control."
        );
        println!(
            "fleetpane-host route CONFIG.json GROUP: read-only ready-Host recommendation in an explicit route group; never creates or claims."
        );
        println!(
            "fleetpane-host creation STORE plan CONFIG GROUP CWD UUID | send UUID [--take-control] | check UUID | list [BEFORE_POSITION]: durable Host binding; send is allowed once, check never resends."
        );
        println!(
            "fleetpane-host workspace STORE add CONFIG HOST git|folder CWD | list [BEFORE_POSITION] | inspect UUID | create UUID [--take-control]: existing local directories, stable IDs and read-only Git observations. Creation is an explicit provider mutation. Read a file with: workspace STORE diff UUID staged|unstaged ROOT_RELATIVE_PATH."
        );
        println!(
            "fleetpane-host worktree STORE plan SOURCE_WORKSPACE_UUID BRANCH BASE_REF UUID | apply UUID | check UUID | list [BEFORE_POSITION]: preview a pinned baseline, then explicitly create once. Git mutation only; no provider/session is started."
        );
        println!(
            "DeepSeek ACP: serve-acp SOCKET CWD [PROVIDER [ARGS...]] (defaults to dsh acp). Session commands: config THREAD OPTION VALUE | permission JSON_ID OPTION_ID. ACP resume does not replay old history."
        );
        println!(
            "fleetpane-host session SOCKET tasks THREAD: read the latest Host-retained agent task list without claiming control or contacting the provider."
        );
        println!(
            "fleetpane-host draft STORE list [BEFORE_POSITION] | get KEY_JSON | edit KEY_JSON REVISION TEXT | check KEY_JSON | send KEY_JSON REVISION [--take-control]: private session drafts; send records one attempt before dispatch, check never resends."
        );
        return Ok(());
    }
    match args[0].as_str() {
        "worktree" if args.len() >= 3 => {
            use fleetpane_session::{creations::Store, worktrees::State};
            let store = Store::open(std::path::absolute(&args[1])?).await?;
            let job = match &args[2..] {
                [verb, source, branch, base, id] if verb == "plan" => Some(
                    store
                        .plan_worktree(id.parse()?, source.parse()?, branch.clone(), base.clone())
                        .await?,
                ),
                [verb, id] if verb == "apply" => Some(store.apply_worktree(id.parse()?).await?),
                [verb, id] if verb == "check" => Some(store.worktree_job(id.parse()?).await?),
                [verb] if verb == "list" => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&store.worktree_jobs(None).await?)?
                    );
                    None
                }
                [verb, before] if verb == "list" => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &store.worktree_jobs(Some(before.parse()?)).await?
                        )?
                    );
                    None
                }
                _ => return Err("invalid worktree command; see --help".into()),
            };
            if let Some(job) = job {
                println!("{}", serde_json::to_string_pretty(&job)?);
                if args[2] == "apply" && matches!(job.state, State::Failed { .. }) {
                    return Err("worktree creation failed; saved result includes the residual directory and branch".into());
                }
            }
            Ok(())
        }
        "draft" if args.len() >= 3 => {
            use fleetpane_session::{
                client::{self, Command, Connection},
                creations::Store,
                drafts::Key,
            };
            let store = Store::open(std::path::absolute(&args[1])?).await?;
            match &args[2..] {
                [verb] if verb == "list" => println!(
                    "{}",
                    serde_json::to_string_pretty(&store.drafts(None).await?)?
                ),
                [verb, before] if verb == "list" => println!(
                    "{}",
                    serde_json::to_string_pretty(&store.drafts(Some(before.parse()?)).await?)?
                ),
                [verb, key] if verb == "get" || verb == "check" => {
                    let key: Key = serde_json::from_str(key)?;
                    let draft = store.draft(key).await?;
                    if verb == "get" {
                        println!("{}", serde_json::to_string_pretty(&draft)?);
                    } else {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&store.check_draft(&draft).await?)?
                        );
                    }
                }
                [verb, key, revision, text] if verb == "edit" => {
                    let draft = store.draft(serde_json::from_str(key)?).await?;
                    if draft.revision != revision.parse::<i64>()? {
                        return Err("draft revision changed".into());
                    }
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &store.edit_draft(&draft, text.clone()).await?
                        )?
                    );
                }
                [verb, key, revision, rest @ ..]
                    if verb == "send" && (rest.is_empty() || rest == ["--take-control"]) =>
                {
                    let draft = store.draft(serde_json::from_str(key)?).await?;
                    if draft.revision != revision.parse::<i64>()? {
                        return Err("draft revision changed".into());
                    }
                    let snapshot = client::snapshot(&draft.key.socket).await?;
                    if snapshot.host_id != draft.key.host_id {
                        return Err("saved draft belongs to an earlier Host incarnation".into());
                    }
                    if snapshot.owner.is_some() && rest.is_empty() {
                        return Err("Host has a controller; use --take-control".into());
                    }
                    let connection = Connection::claim(draft.key.socket.clone(), &snapshot).await?;
                    let command = Command::Send {
                        thread_id: draft.key.thread_id.clone(),
                        text: draft.text.clone(),
                        options: Default::default(),
                    };
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &store
                                .send_draft(&draft, &connection, uuid::Uuid::new_v4(), &command)
                                .await?
                                .result
                        )?
                    );
                }
                _ => return Err("invalid draft command; see --help".into()),
            }
            Ok(())
        }
        "workspace" if args.len() >= 3 => {
            use fleetpane_session::{
                client::{self, Connection},
                creations::Store,
                workspaces::Kind,
            };
            let store = Store::open(std::path::absolute(&args[1])?).await?;
            match &args[2..] {
                [verb, config, host, kind, cwd] if verb == "add" => {
                    let configuration = fleetpane_session::hosts::Configuration::read(
                        std::path::Path::new(config),
                    )?;
                    let endpoint = configuration
                        .hosts
                        .into_iter()
                        .find(|endpoint| endpoint.name == *host)
                        .ok_or("configured Host not found")?;
                    let kind = match kind.as_str() {
                        "git" => Kind::Git,
                        "folder" => Kind::Folder,
                        _ => return Err("choose git or folder".into()),
                    };
                    let workspace = store
                        .register_workspace(endpoint, PathBuf::from(cwd), kind)
                        .await?;
                    println!("{}", serde_json::to_string_pretty(&workspace)?);
                }
                [verb] if verb == "list" => println!(
                    "{}",
                    serde_json::to_string_pretty(&store.workspaces(None).await?)?
                ),
                [verb, before] if verb == "list" => println!(
                    "{}",
                    serde_json::to_string_pretty(&store.workspaces(Some(before.parse()?)).await?)?
                ),
                [verb, id] if verb == "inspect" => println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &store.workspace(id.parse()?).await?.inspect().await?
                    )?
                ),
                [verb, id, scope, path] if verb == "diff" => {
                    let scope = match scope.as_str() {
                        "staged" => fleetpane_session::diffs::Scope::Staged,
                        "unstaged" => fleetpane_session::diffs::Scope::Unstaged,
                        _ => return Err("choose staged or unstaged".into()),
                    };
                    let diff = store
                        .workspace(id.parse()?)
                        .await?
                        .diff(scope, path)
                        .await?;
                    println!("{}", serde_json::to_string_pretty(&diff)?);
                }
                [verb, id, ..]
                    if verb == "create"
                        && (args.len() == 4
                            || (args.len() == 5 && args[4] == "--take-control")) =>
                {
                    let workspace = store.workspace(id.parse()?).await?;
                    workspace.inspect().await?;
                    let socket = workspace.registration.endpoint.socket.clone();
                    let state = client::snapshot(&socket).await?;
                    if state.owner.is_some() && args.len() != 5 {
                        return Err("Host already has a controller; use --take-control".into());
                    }
                    let connection = Connection::claim(socket, &state).await?;
                    let completed = workspace.create(&connection, uuid::Uuid::new_v4()).await?;
                    println!("{}", serde_json::to_string_pretty(&completed.result)?);
                }
                _ => return Err("invalid workspace command; see --help".into()),
            }
            Ok(())
        }
        "creation" if args.len() >= 3 => {
            use fleetpane_session::{
                client::{self, Connection},
                creations::Store,
            };
            let store = Store::open(std::path::absolute(&args[1])?).await?;
            match &args[2..] {
                [verb, config, group, cwd, id] if verb == "plan" => {
                    let configuration = fleetpane_session::hosts::Configuration::read(
                        std::path::Path::new(config),
                    )?;
                    let intent = store.plan(&configuration, group, cwd, id.parse()?).await?;
                    println!("{}", serde_json::to_string_pretty(&intent)?);
                }
                [verb, id] if verb == "check" => {
                    let intent = store.get(id.parse()?).await?.ok_or("creation not found")?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&intent.resolve().await?)?
                    );
                }
                [verb] if verb == "list" => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&store.page(None).await?)?
                    );
                }
                [verb, before] if verb == "list" => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&store.page(Some(before.parse()?)).await?)?
                    );
                }
                [verb, id, ..]
                    if verb == "send"
                        && (args.len() == 4
                            || (args.len() == 5 && args[4] == "--take-control")) =>
                {
                    let id = id.parse()?;
                    let intent = store.get(id).await?.ok_or("creation not found")?;
                    if intent.attempted {
                        return Err(
                            "creation was already attempted; use check, never resend".into()
                        );
                    }
                    let state = client::snapshot(&intent.target.endpoint.socket).await?;
                    intent.target.check(&state)?;
                    if state.owner.is_some() && args.len() != 5 {
                        return Err("Host already has a controller; use --take-control".into());
                    }
                    let connection =
                        Connection::claim(intent.target.endpoint.socket.clone(), &state).await?;
                    let completed = store.dispatch(id, &connection).await?;
                    println!("{}", serde_json::to_string_pretty(&completed.result)?);
                }
                _ => return Err("invalid creation command; see --help".into()),
            }
            Ok(())
        }
        "operations" if args.len() == 2 || (args.len() == 4 && args[2] == "--cursor") => {
            let response = request(
                std::path::Path::new(&args[1]),
                &fleetpane_runtime::Request::Operations {
                    cursor: args.get(3).cloned(),
                },
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&response)?);
            if matches!(response, fleetpane_runtime::Response::Error { .. }) {
                return Err("journal read rejected".into());
            }
            Ok(())
        }
        "operation" if args.len() == 3 => {
            let response = request(
                std::path::Path::new(&args[1]),
                &fleetpane_runtime::Request::Operation {
                    operation_id: args[2].parse()?,
                },
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&response)?);
            if matches!(response, fleetpane_runtime::Response::Error { .. }) {
                return Err("operation read rejected".into());
            }
            Ok(())
        }
        "route" if args.len() == 3 => {
            let configuration =
                fleetpane_session::hosts::Configuration::read(std::path::Path::new(&args[1]))?;
            let selection = fleetpane_session::routing::select(&configuration, &args[2]).await?;
            println!("{}", serde_json::to_string_pretty(&selection)?);
            if selection.target.is_none() {
                return Err("No ready Host in this route group; no action was taken".into());
            }
            Ok(())
        }
        "hosts" if args.len() == 2 => {
            let configuration =
                fleetpane_session::hosts::Configuration::read(std::path::Path::new(&args[1]))?;
            let reports = fleetpane_session::hosts::probe(configuration).await?;
            println!("{}", serde_json::to_string_pretty(&reports)?);
            Ok(())
        }
        "session" if args.len() >= 3 => {
            use fleetpane_session::client::{self, Command, Connection};
            use fleetpane_session::models::{Catalog, Page, TurnOptions};
            let socket = PathBuf::from(&args[1]);
            let take_control = args[2] == "--take-control";
            let command = &args[if take_control { 3 } else { 2 }..];
            if let [verb] = command
                && matches!(verb.as_str(), "drain" | "ready")
            {
                let state = client::snapshot(&socket).await?;
                if state.owner.is_some() && !take_control {
                    return Err("Host already has a controller; use --take-control".into());
                }
                let mode = if verb == "drain" {
                    fleetpane_runtime::lifecycle::Mode::Draining
                } else {
                    fleetpane_runtime::lifecycle::Mode::Ready
                };
                let connection = Connection::claim(socket, &state).await?;
                let admission = connection
                    .set_admission(state.admission.generation, mode)
                    .await?;
                println!("{}", serde_json::to_string_pretty(&admission)?);
                return Ok(());
            }
            if let [verb, id, choice] = command
                && verb == "permission"
            {
                let state = client::snapshot(&socket).await?;
                if state.owner.is_some() && !take_control {
                    return Err("Host already has a controller; use --take-control".into());
                }
                let id: serde_json::Value = serde_json::from_str(id)?;
                let pending = state
                    .pending_requests
                    .iter()
                    .find(|p| p.get("id") == Some(&id))
                    .ok_or("Permission is no longer pending")?
                    .clone();
                let connection = Connection::claim(socket, &state).await?;
                connection.answer_permission(&pending, choice).await?;
                println!("{{\"answered\":true}}");
                return Ok(());
            }
            // Read structured answers from stdin so secrets never enter argv.
            if let [verb, request_id] = command
                && verb == "answer"
            {
                let state = client::snapshot(&socket).await?;
                if state.owner.is_some() && !take_control {
                    return Err("Host already has a controller; use --take-control".into());
                }
                let id: serde_json::Value = serde_json::from_str(request_id)?;
                let pending = state
                    .pending_requests
                    .iter()
                    .find(|request| request.get("id") == Some(&id))
                    .ok_or("request is no longer pending")?;
                let question = fleetpane_session::questions::Request::decode(pending)?
                    .ok_or("not a question request")?;
                let mut input = String::new();
                io::stdin()
                    .take((fleetpane_runtime::MAX_FRAME + 1) as u64)
                    .read_to_string(&mut input)?;
                if input.len() > fleetpane_runtime::MAX_FRAME {
                    return Err("answer exceeds size limit".into());
                }
                let response = serde_json::from_str(&input)?;
                question.validate_response(&response)?;
                let connection = Connection::claim(socket, &state).await?;
                connection.answer_questions(&question, &response).await?;
                println!("{{\"answered\":true}}");
                return Ok(());
            }
            let mut requested_model = None;
            let mut requested_effort = None;
            let command = match command {
                [verb, id, option, value] if verb == "config" => Command::Config {
                    thread_id: id.clone(),
                    config_id: option.clone(),
                    value: value.clone(),
                },
                [verb, id] if verb == "tasks" => Command::Tasks {
                    thread_id: id.clone(),
                },
                [verb] if verb == "models" => Command::Models { cursor: None },
                [verb, flag, cursor] if verb == "models" && flag == "--cursor" => Command::Models {
                    cursor: Some(cursor.clone()),
                },
                [verb, flags @ ..] if verb == "list" => {
                    let mut page = fleetpane_session::directory::PageRequest::default();
                    for pair in flags.chunks(2) {
                        let [flag, value] = pair else {
                            return Err("list flags require a value".into());
                        };
                        let target = match flag.as_str() {
                            "--search" => &mut page.query.search,
                            "--cwd" => &mut page.query.cwd,
                            "--cursor" => &mut page.cursor,
                            _ => return Err(format!("unsupported list flag {flag}").into()),
                        };
                        if target.is_some() {
                            return Err(format!("duplicate list flag {flag}").into());
                        }
                        *target = Some(value.clone());
                    }
                    page.params()?;
                    Command::List(page)
                }
                [verb, id, name] if verb == "rename" => Command::Rename {
                    thread_id: id.clone(),
                    name: name.clone(),
                },
                [verb, cwd] if verb == "create" => Command::Create { cwd: cwd.clone() },
                [verb, id] if verb == "open" => Command::Open {
                    thread_id: id.clone(),
                    resume: false,
                },
                [verb, id] if verb == "inspect" => Command::Inspect {
                    thread_id: id.clone(),
                    resume: false,
                },
                [verb, id, flag] if verb == "inspect" && flag == "--resume" => Command::Inspect {
                    thread_id: id.clone(),
                    resume: true,
                },
                [verb, id, flags @ ..] if verb == "history-turns" => {
                    let cursor = match flags {
                        [] => None,
                        [flag, cursor] if flag == "--cursor" => Some(cursor.clone()),
                        _ => return Err("history-turns accepts --cursor CURSOR".into()),
                    };
                    Command::History(fleetpane_session::history::Request::Turns {
                        thread_id: id.clone(),
                        cursor,
                    })
                }
                [verb, id, turn_id, flags @ ..] if verb == "history-items" => {
                    let cursor = match flags {
                        [] => None,
                        [flag, cursor] if flag == "--cursor" => Some(cursor.clone()),
                        _ => return Err("history-items accepts --cursor CURSOR".into()),
                    };
                    Command::History(fleetpane_session::history::Request::Items {
                        thread_id: id.clone(),
                        turn_id: turn_id.clone(),
                        cursor,
                    })
                }
                [verb, id, text, flags @ ..] if verb == "send" => {
                    for pair in flags.chunks(2) {
                        let [flag, value] = pair else {
                            return Err("send flags require a value".into());
                        };
                        let target = match flag.as_str() {
                            "--model" => &mut requested_model,
                            "--effort" => &mut requested_effort,
                            _ => return Err(format!("unsupported send flag {flag}").into()),
                        };
                        if target.is_some() {
                            return Err(format!("duplicate send flag {flag}").into());
                        }
                        *target = Some(value.clone());
                    }
                    if requested_model.is_none() && requested_effort.is_some() {
                        return Err("--effort requires --model".into());
                    }
                    Command::Send {
                        thread_id: id.clone(),
                        text: text.clone(),
                        options: TurnOptions::default(),
                    }
                }
                [verb, id, turn] if verb == "stop" => Command::Stop {
                    thread_id: id.clone(),
                    turn_id: turn.clone(),
                },
                _ => return Err("invalid session command; use --help".into()),
            };
            let state = client::snapshot(&socket).await?;
            if matches!(command, Command::Tasks { .. }) {
                let lease = state
                    .owner
                    .ok_or("Host has no established controller; no task read lease is available")?;
                let result = Connection { socket, lease }
                    .execute(&command, uuid::Uuid::new_v4())
                    .await?;
                println!("{}", serde_json::to_string_pretty(&result.result)?);
                return Ok(());
            }
            if state.owner.is_some() && !take_control {
                return Err("Host already has a controller; --take-control explicitly transfers ownership and fences its writes".into());
            }
            let connection = Connection::claim(socket, &state).await?;
            let mut command = command;
            if requested_model.is_some() {
                let mut catalog = Catalog::default();
                loop {
                    let cursor = catalog.next_cursor.clone();
                    let result = connection
                        .execute(
                            &Command::Models {
                                cursor: cursor.clone(),
                            },
                            uuid::Uuid::new_v4(),
                        )
                        .await?;
                    catalog.apply(&cursor, serde_json::from_value::<Page>(result.result)?)?;
                    if catalog.complete {
                        break;
                    }
                }
                if let Command::Send { options, .. } = &mut command {
                    *options = TurnOptions::new(&catalog, requested_model, requested_effort)?;
                }
            }
            let id = uuid::Uuid::new_v4();
            let reply = connection.execute(&command, id).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"operationId":id,"sequence":reply.sequence,"result":reply.result})
                )?
            );
            Ok(())
        }
        "serve" | "serve-acp" if args.len() >= 3 => {
            let acp = args[0] == "serve-acp";
            let provider = args.get(3).map_or_else(
                || PathBuf::from(if acp { "dsh" } else { "codex" }),
                PathBuf::from,
            );
            let provider_args = if args.len() > 3 {
                args[4..].to_vec()
            } else {
                if acp {
                    vec!["acp".into()]
                } else {
                    vec!["app-server".into(), "--stdio".into()]
                }
            };
            run(Config {
                socket: PathBuf::from(&args[1]),
                cwd: PathBuf::from(&args[2]),
                provider,
                provider_args,
                acp,
            })
            .await
        }
        "request" if args.len() == 2 => {
            let mut input = String::new();
            io::stdin()
                .take((fleetpane_runtime::MAX_FRAME + 1) as u64)
                .read_to_string(&mut input)?;
            if input.len() > fleetpane_runtime::MAX_FRAME {
                return Err("request exceeds size limit".into());
            }
            let command = serde_json::from_str(&input)?;
            let response = request(std::path::Path::new(&args[1]), &command).await?;
            println!("{}", serde_json::to_string_pretty(&response)?);
            if matches!(response, fleetpane_runtime::Response::Error { .. }) {
                return Err("host rejected request (see JSON above)".into());
            }
            Ok(())
        }
        _ => Err("invalid arguments; use --help".into()),
    }
}

#[cfg(unix)]
#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fleetpane-host: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(unix))]
fn main() -> ExitCode {
    eprintln!("fleetpane-host: Unix transport prototype; Windows transport is not implemented");
    ExitCode::FAILURE
}
