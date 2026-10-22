use std::process::ExitCode;

#[cfg(unix)]
mod migration;

#[cfg(unix)]
mod prompt;

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
            "fleetpane-host serve SOCKET CWD [PROVIDER [ARGS...]]\nfleetpane-host request SOCKET < request.json\nfleetpane-host session SOCKET [--take-control] list [--archived] [--search TITLE] [--cwd CWD] [--cursor CURSOR]|models [--cursor CURSOR]|mcp [--cursor CURSOR]|rename THREAD NAME|restore THREAD|create CWD|open THREAD|send THREAD TEXT [--model MODEL] [--effort EFFORT]|steer THREAD TURN TEXT|stop THREAD TURN|answer JSON_REQUEST_ID < answers.json\n\nUnix prototype. Defaults to codex app-server --stdio. Keep serve running independently.\nOnly local, same-user trusted controllers. Lifecycle: session SOCKET [--take-control] drain|ready (drain pauses fresh mutations; existing output, reads, Stop and answers continue). Mutation receipts persist beside SOCKET in SOCKET.journal; interrupted admissions are never replayed."
        );
        println!(
            "History: inspect THREAD [--resume] | history-turns THREAD [--cursor CURSOR] | history-items THREAD TURN [--cursor CURSOR]. Native views use these bounded commands; open requests full history."
        );
        println!(
            "Prompt input for send THREAD and steer THREAD TURN: choose exactly one of TEXT, --prompt TEXT, --prompt-file PATH or --stdin; use -- TEXT for a literal starting with '-'. Exact UTF-8 text is limited to 64 KiB. Files must be regular files. Input is validated before contacting a Host. Only send accepts --model/--effort."
        );
        println!(
            "Wait: session SOCKET wait THREAD TURN [--timeout-ms 1..3600000] (default 30000). Read-only: returns finished, unknown, attention or timed_out; never takes control, answers, sends or stops. Requires an existing controller for the initial history read."
        );
        println!(
            "Session tree: session SOCKET tree THREAD (read-only, no --take-control). Bounded ancestry and loaded-context observation; never archive/shutdown authorization."
        );
        println!(
            "fleetpane-host operations SOCKET [--cursor CURSOR] | operation SOCKET UUID: read-only durable receipts.\nfleetpane-host hosts CONFIG.json: read-only parallel snapshots of configured local Hosts, without claiming control."
        );
        println!(
            "fleetpane-host attention CONFIG.json [CONTROLLER_STORE]: observe pending approvals/questions and up to 64 retained terminal turns per Host. Optional store includes local review receipts; no takeover, answer or resume.\nfleetpane-host attention-review CONTROLLER_STORE SELECTION.json: mark exact completion records reviewed locally, preserving failures and pending requests."
        );
        println!(
            "fleetpane-host host-diagnostics CONFIG.json [HOST_NAME]: read-only endpoint checks, protocol, process identity, admission, fault and activity counts. Omits provider initialization data and request payloads; never claims or resumes."
        );
        println!(
            "fleetpane-host sessions CONFIG.json [--archived] [--search TITLE] [--cwd ABSOLUTE_DIRECTORY] [--host NAME [--cursor CURSOR --host-id UUID]]: observe 100-session metadata pages across configured Hosts without acquiring control. Failed Hosts have explicit errors in the JSON report."
        );
        println!(
            "fleetpane-host route CONFIG.json GROUP: read-only ready-Host recommendation in an explicit route group; never creates or claims."
        );
        println!(
            "fleetpane-host creation STORE plan CONFIG GROUP CWD UUID | send UUID [--take-control] | check UUID | list [BEFORE_POSITION]: durable Host binding; send is allowed once, check never resends."
        );
        println!(
            "fleetpane-host workspace STORE add CONFIG HOST git|folder CWD | list [BEFORE_POSITION] | rename UUID EXPECTED_REVISION NAME | reset-name UUID EXPECTED_REVISION | rename-project PROJECT_UUID EXPECTED_REVISION NAME | reset-project-name PROJECT_UUID EXPECTED_REVISION | preferences UUIDS_JSON | pins | pin UUID on|off | toggle-pin UUID | reorder-pins EXPECTED_UUIDS_JSON ORDER_UUIDS_JSON | files UUID [RELATIVE_DIRECTORY] | read UUID RELATIVE_FILE | open UUID LOCAL_PATH | export UUID RELATIVE_FILE ABSOLUTE_DESTINATION | inspect UUID | create UUID [--take-control]: existing local directories, stable IDs and read-only Git observations. Export creates a new local copy up to 64 MiB without overwrite. Pins are local preferences (up to 32); they do not contact a Host. Each toggle-pin invocation changes state once; pin on|off is idempotent. Creation is an explicit provider mutation. Read a file with: workspace STORE diff UUID staged|unstaged ROOT_RELATIVE_PATH."
        );
        println!(
            "fleetpane-host worktree STORE plan SOURCE_WORKSPACE_UUID BRANCH BASE_REF UUID | plan-existing SOURCE_WORKSPACE_UUID LOCAL_BRANCH UUID | apply UUID | check UUID | list [BEFORE_POSITION]: preview a pinned baseline or an unoccupied local branch, then explicitly create once. Git mutation only; no provider/session is started."
        );
        println!(
            "DeepSeek ACP: serve-acp SOCKET CWD [PROVIDER [ARGS...]] (defaults to dsh acp). Session commands: config THREAD OPTION VALUE | mode THREAD MODE_ID | permission JSON_ID OPTION_ID. Only advertised ACP v1 modes are selectable; mode changes may run during a prompt. ACP resume does not replay old history."
        );
        println!(
            "fleetpane-host session SOCKET tasks THREAD: read the latest Host-retained agent task list without claiming control or contacting the provider."
        );
        println!(
            "fleetpane-host draft STORE list [BEFORE_POSITION] | get KEY_JSON | edit KEY_JSON REVISION TEXT | check KEY_JSON | send KEY_JSON REVISION [--take-control] | steer KEY_JSON REVISION TURN [--take-control]: private session drafts; send records one attempt before dispatch, check never resends."
        );
        println!(
            "fleetpane-host terminal STORE WORKSPACE_UUID [--take-control] create LABEL | list | output ID AFTER_BYTE | input ID EXPECTED_INPUT TEXT | key ID EXPECTED_INPUT enter|interrupt|eof|tab|escape | resize ID ROWS COLS | rename ID LABEL | close ID. Host-owned POSIX shell; input never appends Enter. Output is raw bytes with explicit loss cursors; native screen and explicit-input panel available from Workspaces."
        );
        println!(
            "fleetpane-host appearance STORE get | set EXPECTED_REVISION system|light|dark: device-local appearance preferences; no Host RPC.\nfleetpane-host notification-policy STORE get | set EXPECTED_REVISION < policy.json: device-local alert preferences; no Host RPC.\nfleetpane-host completion-alerts HOSTS_JSON PREFERENCES_STORE [REVIEW_STORE]: atomically claim new terminal-result reminders; first Host observation baselines silently. Changes local notification cursors, never a Host or review status.\nfleetpane-host pending-alerts HOSTS_JSON PREFERENCES_STORE: claim new pending-request reminders; first observation baselines silently. Consumes local notification cursors; never answers or takes control."
        );
        println!(
            "fleetpane-host recent STORE list SOCKET | remember SOCKET CWD | clear SOCKET: device-local directory suggestions; no Host RPC or session creation."
        );
        println!(
            "fleetpane-host host-choice STORE get | set EXPECTED_REVISION CONFIG HOST: remember a configuration-local Host endpoint; no Host RPC or control transfer."
        );
        println!(
            "fleetpane-host last-session STORE SOCKET get | set EXPECTED_REVISION TARGET_JSON | check: exact device-local inspected session. Get/set use no Host RPC; check explicitly observes up to 800 directory entries without takeover or resume."
        );
        println!(
            "fleetpane-host image SOCKET import ABSOLUTE_FILE: validate and durably save a local PNG/JPEG/WebP asset. Drafts: attach KEY_JSON REVISION FILE... | remove-image KEY_JSON REVISION INDEX. Images are sent only by an explicit draft send; live follow-ups remain text-only."
        );
        println!(
            "Paseo Codex continuation: paseo-import prepare-fork STORE SESSION_UUID | fork STORE SESSION_UUID --take-control | fork-status STORE SESSION_UUID | reprepare-fork STORE SESSION_UUID EXPECTED_OPERATION_UUID. Complete saved legacy history only; one independent fork, never resumes the original session. Check the receipt after any ambiguous dispatch."
        );
        return Ok(());
    }
    match args[0].as_str() {
        "paseo-import" => {
            use fleetpane_session::{
                creations::Store,
                hosts::Configuration,
                paseo_import::{self, BatchPlan, Roots},
            };
            match &args[1..] {
                [verb,directory] if verb=="scratch-status" || verb=="clean-scratch" => {
                    let store=Store::open(std::path::absolute(directory)?).await?;
                    println!("{}",serde_json::to_string(&store.paseo_scratch(verb=="clean-scratch").await?)?);
                }
                [verb,directory,id] if verb=="prepare-fork" || verb=="fork-status" => {
                    let store=Store::open(std::path::absolute(directory)?).await?;
                    let session=id.parse()?;
                    if verb=="prepare-fork" {
                        println!("{}",serde_json::to_string(&store.prepare_paseo_continuation(session).await?)?);
                    } else {
                        let intent=store.paseo_continuation(session).await?.ok_or("no saved fork")?;
                        println!("{}",serde_json::to_string(&intent.resolve().await?)?);
                    }
                }
                [verb,directory,id,expected] if verb=="reprepare-fork" => {
                    let store=Store::open(std::path::absolute(directory)?).await?;
                    println!("{}",serde_json::to_string(&store.reprepare_paseo_continuation(id.parse()?,expected.parse()?).await?)?);
                }
                [verb,directory,id,flag] if verb=="fork" && flag=="--take-control" => {
                    let store=Store::open(std::path::absolute(directory)?).await?;
                    let session=id.parse()?;
                    let intent=store.paseo_continuation(session).await?.ok_or("prepare-fork first")?;
                    if intent.attempted {return Err("fork already attempted; use fork-status".into());}
                    let state=fleetpane_session::client::snapshot(&intent.target.endpoint.socket).await?;
                    intent.target.check(&state)?;
                    let connection=fleetpane_session::client::Connection::claim(intent.target.endpoint.socket.clone(),&state).await?;
                    println!("{}",store.dispatch_paseo_continuation(session,intent.id,&connection).await?.result);
                }
                [verb,home] if verb=="preview" => println!("{}",serde_json::to_string(&paseo_import::discover(PathBuf::from(home)).await?)?),
                [verb,home,config,host,codex,dsh,ids @ ..] if verb=="plan" => {
                    let endpoint=Configuration::read(std::path::Path::new(config))?.hosts.into_iter().find(|e|e.name==*host).ok_or("configured Host name not found")?;
                    let catalog=paseo_import::discover(PathBuf::from(home)).await?;
                    let selected=if ids.is_empty(){catalog.candidates.iter().map(|c|c.source.workspace_id.clone()).collect()}else{ids.iter().cloned().collect()};
                    println!("{}",serde_json::to_string(&BatchPlan::selected(&catalog,&selected,endpoint,Roots{codex:codex.into(),dsh:dsh.into()})?)?);
                }
                [verb,directory,path] if verb=="batch" => {
                    let plan=BatchPlan::read(std::path::Path::new(path))?;
                    let store=Store::open(std::path::absolute(directory)?).await?;
                    println!("{}",serde_json::to_string(&migration::run(&store,migration::Selection::Plan(plan)).await?)?);
                }
                [verb,directory] if verb=="batches" => {
                    let store=Store::open(std::path::absolute(directory)?).await?;
                    println!("{}",serde_json::to_string(&store.paseo_batches(None).await?)?);
                }
                [verb,directory,before] if verb=="batches" => {
                    let store=Store::open(std::path::absolute(directory)?).await?;
                    println!("{}",serde_json::to_string(&store.paseo_batches(Some(before.parse()?)).await?)?);
                }
                [verb,directory,id] if verb=="batch-status" || verb=="batch-results" || verb=="retry-batch" => {
                    let store=Store::open(std::path::absolute(directory)?).await?;
                    let id=id.parse()?;
                    let value=match verb.as_str() {
                        "batch-status"=>serde_json::to_string(&store.paseo_batch(id).await?)?,
                        "batch-results"=>serde_json::to_string(&store.paseo_batch_results(id,None).await?)?,
                        "retry-batch"=>serde_json::to_string(&migration::run(&store,migration::Selection::Retry(id)).await?)?,
                        _=>unreachable!(),
                    };
                    println!("{value}");
                }
                [verb,directory,id,after] if verb=="batch-results" => {
                    let store=Store::open(std::path::absolute(directory)?).await?;
                    println!("{}",serde_json::to_string(&store.paseo_batch_results(id.parse()?,Some(after.parse()?)).await?)?);
                }
                [verb,directory,home,revision,config,host,id] if verb=="workspace" => {
                    let endpoint=Configuration::read(std::path::Path::new(config))?.hosts.into_iter().find(|e|e.name==*host).ok_or("configured Host name not found")?;
                    let catalog=paseo_import::discover(PathBuf::from(home)).await?;
                    if !catalog.candidates.iter().any(|c|c.source.workspace_id==*id && c.revision==*revision) {return Err("Paseo workspace changed; preview again".into());}
                    let store=Store::open(std::path::absolute(directory)?).await?;
                    println!("{}",serde_json::to_string(&store.import_paseo_workspace(&catalog,id,endpoint).await?)?);
                }
                [verb,directory] if verb=="list" => {
                    let store=Store::open(std::path::absolute(directory)?).await?;
                    println!("{}",serde_json::to_string(&store.paseo_workspaces().await?)?);
                }
                [verb,directory,parent] if verb=="sessions" => {
                    let store=Store::open(std::path::absolute(directory)?).await?;
                    println!("{}",serde_json::to_string(&store.paseo_sessions(parent.parse()?,None).await?)?);
                }
                [verb,directory,parent,before] if verb=="sessions" => {
                    let store=Store::open(std::path::absolute(directory)?).await?;
                    println!("{}",serde_json::to_string(&store.paseo_sessions(parent.parse()?,Some(before.parse()?)).await?)?);
                }
                [verb,directory,id,codex,dsh] if verb=="copy-history" => {
                    let store=Store::open(std::path::absolute(directory)?).await?;
                    println!("{}",serde_json::to_string(&store.snapshot_paseo_history(id.parse()?,Roots{codex:codex.into(),dsh:dsh.into()}).await?)?);
                }
                [verb,directory,id,codex,dsh] if verb=="copy-workspace-history" => {
                    let store=Store::open(std::path::absolute(directory)?).await?;
                    println!("{}",serde_json::to_string(&store.copy_paseo_workspace_history(id.parse()?,Roots{codex:codex.into(),dsh:dsh.into()}).await?)?);
                }
                [verb,directory,id,record] if verb=="source-record" => {
                    let store=Store::open(std::path::absolute(directory)?).await?;
                    println!("{}",serde_json::to_string(&store.paseo_source_record(id.parse()?,record.parse()?).await?)?);
                }
                [verb,directory,id,after] if verb=="history" => {
                    let store=Store::open(std::path::absolute(directory)?).await?;
                    println!("{}",serde_json::to_string(&store.paseo_history(id.parse()?,after.parse()?).await?)?);
                }
                _=>return Err("paseo-import preview PASEO_HOME | plan PASEO_HOME CONFIG HOST CODEX_HOME DSH_HOME [SOURCE_WORKSPACE_ID ...] | batch STORE PLAN_JSON | batches STORE [BEFORE_POSITION] | batch-status STORE BATCH_UUID | batch-results STORE BATCH_UUID [AFTER_ORDINAL] | retry-batch STORE BATCH_UUID | scratch-status STORE | clean-scratch STORE | workspace STORE PASEO_HOME WORKSPACE_PREVIEW_REVISION CONFIG HOST SOURCE_WORKSPACE_ID | list STORE | sessions STORE IMPORTED_WORKSPACE_ID [BEFORE_POSITION] | copy-history STORE SESSION_UUID CODEX_HOME DSH_HOME | copy-workspace-history STORE WORKSPACE_UUID CODEX_HOME DSH_HOME | history STORE SESSION_UUID AFTER_ROW | source-record STORE SESSION_UUID RECORD_NUMBER | prepare-fork STORE SESSION_UUID | fork STORE SESSION_UUID --take-control | fork-status STORE SESSION_UUID | reprepare-fork STORE SESSION_UUID EXPECTED_OPERATION_UUID".into()),
            }
            Ok(())
        }
        "image" if args.len() == 4 && args[2] == "import" => {
            let store =
                fleetpane_runtime::attachments::Store::for_socket(&PathBuf::from(&args[1]))?;
            println!(
                "{}",
                serde_json::to_string(&store.import(PathBuf::from(&args[3])).await?)?
            );
            Ok(())
        }
        "recent" if args.len() >= 4 => {
            let store =
                fleetpane_session::creations::Store::open(std::path::absolute(&args[1])?).await?;
            let socket = PathBuf::from(&args[3]);
            let paths = match &args[2..] {
                [verb, _] if verb == "list" => store.recent_directories(socket).await?,
                [verb, _, cwd] if verb == "remember" => {
                    store.remember_directory(socket, PathBuf::from(cwd)).await?
                }
                [verb, _] if verb == "clear" => {
                    store.clear_recent_directories(socket).await?;
                    Vec::new()
                }
                _ => {
                    return Err(
                        "recent STORE list SOCKET | remember SOCKET CWD | clear SOCKET".into(),
                    );
                }
            };
            println!("{}", serde_json::to_string(&paths)?);
            Ok(())
        }
        "notification-policy" if args.len() >= 3 => {
            use fleetpane_session::{creations::Store, notifications::Preferences};
            let input = match &args[2..] {
                [verb] if verb == "get" => None,
                [verb, revision] if verb == "set" => {
                    let mut input = Vec::new();
                    io::stdin().take(524289).read_to_end(&mut input)?;
                    if input.len() > 524288 {
                        return Err("notification policy exceeds 512 KiB".into());
                    }
                    let value: Preferences = serde_json::from_slice(&input)?;
                    value.validate()?;
                    Some((revision.parse()?, value))
                }
                _ => {
                    return Err(
                        "notification-policy STORE get | set EXPECTED_REVISION < policy.json"
                            .into(),
                    );
                }
            };
            let store = Store::open(std::path::absolute(&args[1])?).await?;
            let result = match input {
                None => store.notifications().await?,
                Some((revision, value)) => store.save_notifications(revision, value).await?,
            };
            println!("{}", serde_json::to_string(&result)?);
            Ok(())
        }
        "appearance" if args.len() >= 3 => {
            use fleetpane_session::{
                appearance::{Preferences, Theme},
                creations::Store,
            };
            let store = Store::open(std::path::absolute(&args[1])?).await?;
            let result = match &args[2..] {
                [verb] if verb == "get" => store.appearance().await?,
                [verb, revision, theme] if verb == "set" => {
                    let theme: Theme = serde_json::from_value(serde_json::json!(theme))?;
                    store
                        .save_appearance(revision.parse()?, Preferences { theme })
                        .await?
                }
                _ => {
                    return Err(
                        "appearance STORE get | set EXPECTED_REVISION system|light|dark".into(),
                    );
                }
            };
            println!("{}", serde_json::to_string(&result)?);
            Ok(())
        }
        "last-session" if args.len() >= 4 => {
            let store =
                fleetpane_session::creations::Store::open(std::path::absolute(&args[1])?).await?;
            let socket = PathBuf::from(&args[2]);
            let saved = match &args[3..] {
                [verb] if verb == "get" => store.session_selection(socket).await?,
                [verb] if verb == "check" => {
                    let saved = store.session_selection(socket.clone()).await?;
                    saved
                        .target
                        .as_ref()
                        .ok_or("no saved session")?
                        .observe(socket)
                        .await?;
                    saved
                }
                [verb, revision, target] if verb == "set" => {
                    if target.len() > 16 * 1024 {
                        return Err("last-session target exceeds 16 KiB".into());
                    }
                    store
                        .save_session_selection(
                            socket,
                            revision.parse()?,
                            serde_json::from_str(target)?,
                        )
                        .await?
                }
                _ => {
                    return Err(
                        "last-session STORE SOCKET get | set EXPECTED_REVISION TARGET_JSON | check"
                            .into(),
                    );
                }
            };
            println!("{}", serde_json::to_string(&saved)?);
            Ok(())
        }
        "host-choice" if args.len() >= 3 => {
            let store =
                fleetpane_session::creations::Store::open(std::path::absolute(&args[1])?).await?;
            let result = match &args[2..] {
                [verb] if verb == "get" => store.host_selection().await?,
                [verb, revision, config, host] if verb == "set" => {
                    let configuration = fleetpane_session::hosts::Configuration::read(
                        std::path::Path::new(config),
                    )?;
                    let endpoint = configuration
                        .hosts
                        .iter()
                        .find(|endpoint| endpoint.name == *host)
                        .ok_or("Host is absent from configuration")?;
                    store
                        .save_host_selection(revision.parse()?, endpoint.socket.clone())
                        .await?
                }
                _ => return Err("host-choice STORE get | set EXPECTED_REVISION CONFIG HOST".into()),
            };
            println!("{}", serde_json::to_string(&result)?);
            Ok(())
        }
        "terminal" if args.len() >= 4 => {
            use fleetpane_session::{
                client::{self, Connection},
                creations::Store,
                terminals::{Command, Mutation, Read},
            };
            let store = Store::open(std::path::absolute(&args[1])?).await?;
            let workspace = store.workspace(args[2].parse()?).await?;
            let workspace_id = workspace.id;
            let socket = workspace.registration.endpoint.socket.clone();
            let (take_control, tail) = if args[3] == "--take-control" {
                (true, &args[4..])
            } else {
                (false, &args[3..])
            };
            let command = match tail {
                [verb, _] if verb == "create" => None,
                [verb] if verb == "list" => Some(Command::Read(Read::List { workspace_id })),
                [verb, id, after] if verb == "output" => Some(Command::Read(Read::Output {
                    workspace_id,
                    id: id.parse()?,
                    after: after.parse()?,
                })),
                [verb, id, expected, text] if verb == "input" || verb == "key" => {
                    let bytes = if verb == "input" {
                        text.as_bytes().to_vec()
                    } else {
                        match text.as_str() {
                            "enter" => vec![13],
                            "interrupt" => vec![3],
                            "eof" => vec![4],
                            "tab" => vec![9],
                            "escape" => vec![27],
                            _ => return Err("unsupported terminal key".into()),
                        }
                    };
                    Some(Command::Mutate(Mutation::Input {
                        workspace_id,
                        id: id.parse()?,
                        expected_input: expected.parse()?,
                        bytes,
                    }))
                }
                [verb, id, rows, cols] if verb == "resize" => {
                    Some(Command::Mutate(Mutation::Resize {
                        workspace_id,
                        id: id.parse()?,
                        rows: rows.parse()?,
                        cols: cols.parse()?,
                    }))
                }
                [verb, id, label] if verb == "rename" => Some(Command::Mutate(Mutation::Rename {
                    workspace_id,
                    id: id.parse()?,
                    label: label.clone(),
                })),
                [verb, id] if verb == "close" => Some(Command::Mutate(Mutation::Close {
                    workspace_id,
                    id: id.parse()?,
                })),
                _ => return Err("invalid terminal command; see --help".into()),
            };
            let state = client::snapshot(&socket).await?;
            if state.provider_info["fleetpaneTerminalVersion"] != 1 {
                return Err("Host does not advertise terminal support".into());
            }
            let read = matches!(command, Some(Command::Read(_)));
            let connection = if read {
                Connection {
                    socket,
                    lease: state
                        .owner
                        .ok_or("Host has no established controller; create a terminal first")?,
                }
            } else {
                if state.owner.is_some() && !take_control {
                    return Err(
                        "Host has a controller; --take-control explicitly transfers ownership"
                            .into(),
                    );
                }
                Connection::claim(socket, &state).await?
            };
            let id = uuid::Uuid::new_v4();
            let result = if let Some(command) = command {
                connection.execute_terminal(&command, id).await?
            } else {
                workspace
                    .create_terminal(&connection, id, tail[1].clone())
                    .await?
            };
            println!("{}", serde_json::to_string_pretty(&result.result)?);
            Ok(())
        }
        "worktree" if args.len() >= 3 => {
            use fleetpane_session::{creations::Store, worktrees::State};
            let store = Store::open(std::path::absolute(&args[1])?).await?;
            let job = match &args[2..] {
                [verb, source, branch, base, id] if verb == "plan" => Some(
                    store
                        .plan_worktree(id.parse()?, source.parse()?, branch.clone(), base.clone())
                        .await?,
                ),
                [verb, source, branch, id] if verb == "plan-existing" => Some(
                    store
                        .plan_existing_worktree(id.parse()?, source.parse()?, branch.clone())
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
                [verb, key, revision, paths @ ..] if verb == "attach" && !paths.is_empty() => {
                    let draft = store.draft(serde_json::from_str(key)?).await?;
                    if draft.revision != revision.parse::<i64>()? {
                        return Err("draft revision changed".into());
                    }
                    if draft.images.len() + paths.len() > fleetpane_runtime::attachments::MAX_IMAGES
                    {
                        return Err("A message can contain at most four images".into());
                    }
                    let assets =
                        fleetpane_runtime::attachments::Store::for_socket(&draft.key.socket)?;
                    let mut images = draft.images.clone();
                    for path in paths {
                        images.push(assets.import(PathBuf::from(path)).await?);
                    }
                    println!(
                        "{}",
                        serde_json::to_string(
                            &store
                                .edit_content(&draft, draft.text.clone(), images)
                                .await?
                        )?
                    );
                }
                [verb, key, revision, index] if verb == "remove-image" => {
                    let draft = store.draft(serde_json::from_str(key)?).await?;
                    if draft.revision != revision.parse::<i64>()? {
                        return Err("draft revision changed".into());
                    }
                    let index = index.parse::<usize>()?;
                    if index >= draft.images.len() {
                        return Err("image index is out of range".into());
                    }
                    let mut images = draft.images.clone();
                    images.remove(index);
                    println!(
                        "{}",
                        serde_json::to_string(
                            &store
                                .edit_content(&draft, draft.text.clone(), images)
                                .await?
                        )?
                    );
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
                    if (verb == "send" && (rest.is_empty() || rest == ["--take-control"]))
                        || (verb == "steer"
                            && matches!(rest, [_] | [_, _])
                            && (rest.len() == 1 || rest[1] == "--take-control")) =>
                {
                    let draft = store.draft(serde_json::from_str(key)?).await?;
                    if draft.revision != revision.parse::<i64>()? {
                        return Err("draft revision changed".into());
                    }
                    let snapshot = client::snapshot(&draft.key.socket).await?;
                    if snapshot.host_id != draft.key.host_id {
                        return Err("saved draft belongs to an earlier Host incarnation".into());
                    }
                    if snapshot.owner.is_some() && !rest.iter().any(|s| s == "--take-control") {
                        return Err("Host has a controller; use --take-control".into());
                    }
                    let connection = Connection::claim(draft.key.socket.clone(), &snapshot).await?;
                    let command = if verb == "steer" {
                        Command::Steer {
                            thread_id: draft.key.thread_id.clone(),
                            turn_id: rest[0].clone(),
                            text: draft.text.clone(),
                        }
                    } else {
                        Command::Send {
                            thread_id: draft.key.thread_id.clone(),
                            text: draft.text.clone(),
                            images: draft.images.clone(),
                            options: Default::default(),
                        }
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
                [verb] if verb == "pins" => println!(
                    "{}",
                    serde_json::to_string_pretty(&store.pinned_workspaces().await?)?
                ),
                [verb, ids] if verb == "preferences" => {
                    if ids.len() > 4096 {
                        return Err("workspace UUID JSON exceeds 4096 bytes".into());
                    }
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &store
                                .workspace_preferences(serde_json::from_str(ids)?)
                                .await?
                        )?
                    );
                }
                [verb, expected, order] if verb == "reorder-pins" => {
                    if expected.len() > 4096 || order.len() > 4096 {
                        return Err("pin order JSON exceeds 4096 bytes".into());
                    }
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &store
                                .reorder_workspace_pins(
                                    serde_json::from_str(expected)?,
                                    serde_json::from_str(order)?
                                )
                                .await?
                        )?
                    );
                }
                [verb, id, revision, value] if verb == "rename" => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &store
                                .rename_workspace(
                                    id.parse()?,
                                    revision.parse()?,
                                    Some(value.clone())
                                )
                                .await?
                        )?
                    );
                }
                [verb, id, revision, value] if verb == "rename-project" => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &store
                                .rename_project(id.parse()?, revision.parse()?, Some(value.clone()))
                                .await?
                        )?
                    );
                }
                [verb, id, revision] if verb == "reset-project-name" => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &store
                                .rename_project(id.parse()?, revision.parse()?, None)
                                .await?
                        )?
                    );
                }
                [verb, id] if verb == "toggle-pin" => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &store.toggle_workspace_pinned(id.parse()?).await?
                        )?
                    );
                }
                [verb, id, revision] if verb == "reset-name" => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &store
                                .rename_workspace(id.parse()?, revision.parse()?, None)
                                .await?
                        )?
                    );
                }
                [verb, id, rest @ ..] if verb == "files" && rest.len() <= 1 => {
                    let path = rest.first().map_or("", String::as_str);
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &store.workspace(id.parse()?).await?.list_files(path).await?
                        )?
                    );
                }
                [verb, id, path] if verb == "read" => println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &store.workspace(id.parse()?).await?.read_file(path).await?
                    )?
                ),
                [verb, id, path] if verb == "open" => println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &store.workspace(id.parse()?).await?.open_path(path).await?
                    )?
                ),
                [verb, id, path, destination] if verb == "export" => println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &store
                            .workspace(id.parse()?)
                            .await?
                            .export_file(path, PathBuf::from(destination))
                            .await?
                    )?
                ),
                [verb, id, state] if verb == "pin" => {
                    let pinned = match state.as_str() {
                        "on" => true,
                        "off" => false,
                        _ => return Err("choose pin on or off".into()),
                    };
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &store.set_workspace_pinned(id.parse()?, pinned).await?
                        )?
                    );
                }
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
        "sessions" if args.len() >= 2 => {
            let configuration =
                fleetpane_session::hosts::Configuration::read(std::path::Path::new(&args[1]))?;
            let mut query = fleetpane_session::directory::Query::default();
            let mut host = None;
            let mut cursor = None;
            let mut host_id = None;
            let mut options = args[2..].iter();
            while let Some(flag) = options.next() {
                if flag == "--archived" && !query.archived {
                    query.archived = true;
                    continue;
                }
                let destination = match flag.as_str() {
                    "--search" if query.search.is_none() => &mut query.search,
                    "--cwd" if query.cwd.is_none() => &mut query.cwd,
                    "--host" if host.is_none() => &mut host,
                    "--cursor" if cursor.is_none() => &mut cursor,
                    "--host-id" if host_id.is_none() => {
                        host_id = Some(options.next().ok_or("--host-id needs a UUID")?.parse::<uuid::Uuid>()?);
                        continue;
                    }
                    _ => return Err("Unknown or duplicate sessions flag; use --archived, --search TITLE, --cwd ABSOLUTE_DIRECTORY, --host NAME, --cursor CURSOR or --host-id UUID".into()),
                };
                *destination = Some(
                    options
                        .next()
                        .ok_or("sessions filter flag needs a value")?
                        .clone(),
                );
            }
            if cursor.is_some() && (host.is_none() || host_id.is_none())
                || host_id.is_some() && host.is_none()
            {
                return Err(
                    "Paging requires --host NAME, --cursor CURSOR and the original --host-id UUID"
                        .into(),
                );
            }
            query.validate()?;
            let reports = if let Some(name) = host {
                let endpoint = configuration
                    .hosts
                    .into_iter()
                    .find(|endpoint| endpoint.name == name)
                    .ok_or("Unknown configured Host name")?;
                let request = fleetpane_session::directory::PageRequest { query, cursor };
                let result =
                    fleetpane_session::fleet_directory::read_page(&endpoint, host_id, &request)
                        .await;
                let (observation, error) = match result {
                    Ok(observation) => (Some(observation), None),
                    Err(error) => (None, Some(error.to_string())),
                };
                vec![fleetpane_session::fleet_directory::Report {
                    endpoint,
                    observation,
                    error,
                }]
            } else {
                fleetpane_session::fleet_directory::collect(configuration, query).await?
            };
            println!("{}", serde_json::to_string_pretty(&reports)?);
            Ok(())
        }
        "attention" if args.len() == 2 || args.len() == 3 => {
            let configuration =
                fleetpane_session::hosts::Configuration::read(std::path::Path::new(&args[1]))?;
            let report = if let Some(directory) = args.get(2) {
                let store = fleetpane_session::creations::Store::open(directory.into()).await?;
                fleetpane_session::attention::collect_reviewed(configuration, &store).await?
            } else {
                fleetpane_session::attention::collect(configuration).await?
            };
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        "pending-alerts" if args.len() == 3 => {
            let configuration =
                fleetpane_session::hosts::Configuration::read(std::path::Path::new(&args[1]))?;
            let report = fleetpane_session::attention::collect(configuration).await?;
            let observations =
                fleetpane_session::notifications::pending::Observation::from_report(&report);
            let store = fleetpane_session::creations::Store::open((&args[2]).into()).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&store.claim_pending_alerts(observations).await?)?
            );
            Ok(())
        }
        "completion-alerts" if args.len() == 3 || args.len() == 4 => {
            let configuration =
                fleetpane_session::hosts::Configuration::read(std::path::Path::new(&args[1]))?;
            let report = if let Some(directory) = args.get(3) {
                let reviews = fleetpane_session::creations::Store::open(directory.into()).await?;
                fleetpane_session::attention::collect_reviewed(configuration, &reviews).await?
            } else {
                fleetpane_session::attention::collect(configuration).await?
            };
            let observations =
                fleetpane_session::notifications::delivery::Observation::from_report(&report);
            let store = fleetpane_session::creations::Store::open((&args[2]).into()).await?;
            let batch = store.claim_completion_alerts(observations).await?;
            println!("{}", serde_json::to_string_pretty(&batch)?);
            Ok(())
        }
        "attention-review" if args.len() == 3 => {
            let path = std::path::Path::new(&args[2]);
            if std::fs::metadata(path)?.len() > 1024 * 1024 {
                return Err("completion review input exceeds 1 MiB".into());
            }
            let items: Vec<fleetpane_session::attention::review::Item> =
                serde_json::from_slice(&std::fs::read(path)?)?;
            let count = items.len();
            let store = fleetpane_session::creations::Store::open((&args[1]).into()).await?;
            store.review_completions(items).await?;
            println!("{}", serde_json::json!({"reviewed":count}));
            Ok(())
        }
        "host-diagnostics" if args.len() == 2 || args.len() == 3 => {
            let mut configuration =
                fleetpane_session::hosts::Configuration::read(std::path::Path::new(&args[1]))?;
            if let Some(name) = args.get(2) {
                configuration
                    .hosts
                    .retain(|endpoint| endpoint.name == *name);
                if configuration.hosts.is_empty() {
                    return Err("configured Host name not found".into());
                }
                configuration.routes.clear();
            }
            let reports = fleetpane_session::hosts::diagnostics::collect(configuration).await?;
            println!("{}", serde_json::to_string_pretty(&reports)?);
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
            if let [verb, thread] = command
                && verb == "tree"
            {
                if take_control {
                    return Err("Session tree inspection is read-only; omit --take-control".into());
                }
                let state = client::snapshot(&socket).await?;
                let view = fleetpane_session::session_tree::inspect(&socket, state.host_id, thread)
                    .await?;
                println!("{}", serde_json::to_string_pretty(&view)?);
                return Ok(());
            }
            if let [verb, thread, turn, flags @ ..] = command
                && verb == "wait"
            {
                if take_control {
                    return Err("Wait is read-only; omit --take-control".into());
                }
                let timeout_ms = match flags {
                    [] => fleetpane_session::wait::DEFAULT_TIMEOUT_MS,
                    [flag, value] if flag == "--timeout-ms" => value.parse()?,
                    _ => return Err("wait THREAD TURN [--timeout-ms 1..3600000]".into()),
                };
                let state = client::snapshot(&socket).await?;
                let lease = state.owner.ok_or("Wait needs an existing controller lease for the initial read; claim control explicitly with a separate session command")?;
                let connection = Connection { socket, lease };
                let report = connection.wait_turn(thread, turn, timeout_ms).await?;
                println!("{}", serde_json::to_string_pretty(&report)?);
                return Ok(());
            }
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
                [verb, id, mode] if verb == "mode" => Command::Mode {
                    thread_id: id.clone(),
                    mode_id: mode.clone(),
                },
                [verb, id, option, value] if verb == "config" => Command::Config {
                    thread_id: id.clone(),
                    config_id: option.clone(),
                    value: value.clone(),
                },
                [verb, id] if verb == "tasks" => Command::Tasks {
                    thread_id: id.clone(),
                },
                [verb] if verb == "models" => Command::Models { cursor: None },
                [verb] if verb == "mcp" => Command::Mcp { cursor: None },
                [verb, flag, cursor] if verb == "mcp" && flag == "--cursor" => Command::Mcp {
                    cursor: Some(cursor.clone()),
                },
                [verb, flag, cursor] if verb == "models" && flag == "--cursor" => Command::Models {
                    cursor: Some(cursor.clone()),
                },
                [verb, flags @ ..] if verb == "list" => {
                    let mut page = fleetpane_session::directory::PageRequest::default();
                    let mut rest = flags;
                    while let Some((flag, tail)) = rest.split_first() {
                        if flag == "--archived" {
                            if page.query.archived {
                                return Err("duplicate list flag --archived".into());
                            }
                            page.query.archived = true;
                            rest = tail;
                            continue;
                        }
                        let Some((value, tail)) = tail.split_first() else {
                            return Err("list flags require a value".into());
                        };
                        rest = tail;
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
                [verb, id] if verb == "restore" => Command::Restore {
                    thread_id: id.clone(),
                },
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
                [verb, id, flags @ ..] if verb == "send" => {
                    let input = prompt::Input::parse(flags, true)?;
                    let text = input.read(&mut io::stdin().lock())?;
                    requested_model = input.model;
                    requested_effort = input.effort;
                    Command::Send {
                        thread_id: id.clone(),
                        text,
                        images: Vec::new(),
                        options: TurnOptions::default(),
                    }
                }
                [verb, id, turn, flags @ ..] if verb == "steer" => {
                    let input = prompt::Input::parse(flags, false)?;
                    Command::Steer {
                        thread_id: id.clone(),
                        turn_id: turn.clone(),
                        text: input.read(&mut io::stdin().lock())?,
                    }
                }
                [verb, id, turn] if verb == "stop" => Command::Stop {
                    thread_id: id.clone(),
                    turn_id: turn.clone(),
                },
                _ => return Err("invalid session command; use --help".into()),
            };
            // Validate local input before observing or claiming a Host lease.
            command.validate()?;
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
