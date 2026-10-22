use super::*;
use serde_json::{Value, json};

struct Fixture {
    root: PathBuf,
    home: PathBuf,
    cwd: PathBuf,
    store: Store,
    roots: Roots,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
impl Fixture {
    async fn new() -> Self {
        let root = std::env::temp_dir().join(format!("fp-migration-{}", Uuid::new_v4()));
        let home = root.join("paseo");
        let cwd = root.join("工作区 with space");
        for p in [
            home.join("projects"),
            home.join("agents/project"),
            cwd.clone(),
            root.join("codex"),
            root.join("dsh"),
        ] {
            std::fs::create_dir_all(p).unwrap();
        }
        let store = Store::open(root.join("controller")).await.unwrap();
        let roots = Roots {
            codex: root.join("codex"),
            dsh: root.join("dsh"),
        };
        let fixture = Self {
            root,
            home,
            cwd,
            store,
            roots,
        };
        fixture.workspaces(false);
        fixture.agent("one", "codex", Some("w"));
        fixture
    }
    fn workspaces(&self, missing: bool) {
        let cwd = if missing {
            self.root.join("deleted")
        } else {
            self.cwd.clone()
        };
        self.write(self.home.join("projects/workspaces.json"),json!([{"workspaceId":"w","projectId":"p","cwd":cwd,"kind":"directory","displayName":"Workspace","title":"中文 title","archivedAt":"2026-01-01","pinnedAt":"2026-01-02"}]));
    }
    fn write(&self, path: PathBuf, value: Value) {
        std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    }
    fn agent(&self, id: &str, provider: &str, workspace: Option<&str>) {
        self.write(self.home.join(format!("agents/project/{id}.json")),json!({"id":id,"provider":provider,"cwd":self.cwd,"workspaceId":workspace,"title":format!("Session {id}"),"lastStatus":"idle","persistence":{"provider":provider,"sessionId":id,"metadata":{"secret":"DO_NOT_COPY"}},"config":{"apiKey":"DO_NOT_COPY"}}));
    }
    fn endpoint(&self) -> Endpoint {
        Endpoint {
            name: "local".into(),
            socket: self.root.join("absent.sock"),
        }
    }
    async fn imported(&self) -> ImportedWorkspace {
        self.store
            .import_paseo_workspace(
                &discover(self.home.clone()).await.unwrap(),
                "w",
                self.endpoint(),
            )
            .await
            .unwrap()
    }
    fn codex_log(&self, id: &str, rows: &[Value], suffix: &str) -> PathBuf {
        let path = self.roots.codex.join(format!("{id}.jsonl"));
        let header = json!({"type":"session_meta","payload":{"id":id,"cwd":self.cwd}});
        let mut text = serde_json::to_string(&header).unwrap() + "\n";
        for row in rows {
            text.push_str(&serde_json::to_string(row).unwrap());
            text.push('\n');
        }
        text.push_str(suffix);
        std::fs::write(&path, text).unwrap();
        let db = rusqlite::Connection::open(self.roots.codex.join("state_5.sqlite")).unwrap();
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS threads(id TEXT PRIMARY KEY,rollout_path TEXT)",
        )
        .unwrap();
        db.execute(
            "INSERT OR REPLACE INTO threads VALUES(?1,?2)",
            params![id, path.to_str().unwrap()],
        )
        .unwrap();
        path
    }
    async fn session(&self) -> Session {
        let imported = self.imported().await;
        self.store
            .paseo_sessions(imported.id, None)
            .await
            .unwrap()
            .remove(0)
    }
}
fn message(n: usize) -> Value {
    json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":format!("## Answer {n}\n\n中文 **bold**")} ]}})
}

#[tokio::test]
async fn fork_export_survives_source_removal_preserves_raw_records_and_refuses_tampering() {
    let f = Fixture::new().await;
    let log = f.codex_log(
        "one",
        &[
            message(1),
            json!({"type":"unknown-future","opaque":{"keep":"exact 中文"}}),
        ],
        "",
    );
    let original = std::fs::read(&log).unwrap();
    let session = f.session().await;
    f.store
        .snapshot_paseo_history(session.id, f.roots.clone())
        .await
        .unwrap();
    std::fs::remove_file(&log).unwrap();
    let exported = history::export_codex(f.store.root(), &session).unwrap();
    assert_eq!(std::fs::read(&exported).unwrap(), original);
    assert_eq!(
        history::export_codex(f.store.root(), &session).unwrap(),
        exported
    );
    std::fs::write(&exported, b"changed").unwrap();
    assert!(
        history::export_codex(f.store.root(), &session)
            .unwrap_err()
            .to_string()
            .contains("changed")
    );
    assert!(
        std::fs::read_dir(exported.parent().unwrap())
            .unwrap()
            .all(|p| p.unwrap().path().extension().unwrap() == "jsonl")
    );
}

#[tokio::test]
async fn fork_refuses_partial_and_non_self_contained_history() {
    for mode in [json!("paginated"), json!("future"), json!(null), json!(1)] {
        let f = Fixture::new().await;
        let log = f.codex_log("one", &[message(1)], "");
        let text = std::fs::read_to_string(&log).unwrap();
        let mut records: Vec<Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        records[0]["payload"]["history_mode"] = mode;
        std::fs::write(
            &log,
            records.iter().map(|r| format!("{r}\n")).collect::<String>(),
        )
        .unwrap();
        let session = f.session().await;
        f.store
            .snapshot_paseo_history(session.id, f.roots.clone())
            .await
            .unwrap();
        assert!(history::export_codex(f.store.root(), &session).is_err());
        assert!(!f.store.root().join("paseo-forks").exists());
    }
    let f = Fixture::new().await;
    f.codex_log("one", &[message(1)], "{incomplete");
    let session = f.session().await;
    f.store
        .snapshot_paseo_history(session.id, f.roots.clone())
        .await
        .unwrap();
    assert!(
        history::export_codex(f.store.root(), &session)
            .unwrap_err()
            .to_string()
            .contains("complete")
    );
}

#[tokio::test]
async fn schema_eight_upgrade_preserves_archives_and_requires_the_new_binding_table() {
    let f = Fixture::new().await;
    f.codex_log("one", &[message(1)], "");
    let session = f.session().await;
    f.store
        .snapshot_paseo_history(session.id, f.roots.clone())
        .await
        .unwrap();
    f.store
        .job(|db| {
            db.execute_batch("DROP TABLE project_names; DROP TABLE workspace_names; DROP TABLE pending_alert_cursors; DROP TABLE completion_alert_cursors; DROP TABLE notification_preferences; DROP TABLE workspace_layout; DROP TABLE session_selection; DROP TABLE attention_reviews; DROP TABLE host_selection; DROP TABLE paseo_batch_results; DROP TABLE paseo_batches; DROP TABLE paseo_continuations; PRAGMA user_version=8;")?;
            Ok(())
        })
        .await
        .unwrap();
    let reopened = Store::open(f.store.root().into()).await.unwrap();
    assert_eq!(
        reopened.paseo_session(session.id).await.unwrap().agent.id,
        "one"
    );
    assert_eq!(
        reopened
            .paseo_history(session.id, 0)
            .await
            .unwrap()
            .status
            .rows,
        1
    );
    assert!(
        reopened
            .paseo_continuation(session.id)
            .await
            .unwrap()
            .is_none()
    );
    reopened
        .job(|db| {
            db.execute_batch("DROP TABLE paseo_continuations;")?;
            Ok(())
        })
        .await
        .unwrap();
    assert!(Store::open(f.store.root().into()).await.is_err());
}

#[tokio::test]
async fn batch_partial_failures_stale_selection_and_idempotent_resume() {
    let f = Fixture::new().await;
    f.agent("unsupported", "claude", Some("w"));
    f.codex_log("one", &[message(1)], "");
    let path = f.home.join("projects/workspaces.json");
    let mut workspaces: Vec<Value> =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for id in ["changed", "gone", "missing"] {
        let mut workspace = workspaces[0].clone();
        workspace["workspaceId"] = json!(id);
        if id == "missing" {
            workspace["cwd"] = json!(f.root.join("absent-directory"));
        }
        workspaces.push(workspace);
    }
    f.write(path.clone(), json!(workspaces));
    f.agent("missing-history", "codex", Some("missing"));
    let catalog = discover(f.home.clone()).await.unwrap();
    let plan = BatchPlan::selected(
        &catalog,
        &catalog
            .candidates
            .iter()
            .map(|c| c.source.workspace_id.clone())
            .collect(),
        f.endpoint(),
        f.roots.clone(),
    )
    .unwrap();
    workspaces[1]["title"] = json!("changed after preview");
    workspaces.remove(2);
    f.write(path, json!(workspaces));
    // A session outside the selection does not invalidate the plan.
    f.agent("orphan", "codex", Some("unselected"));
    let report = f
        .store
        .import_paseo_batch(plan.clone(), BatchControl::default())
        .await
        .unwrap();
    assert!(!report.cancelled);
    assert_eq!(report.progress.workspaces_done, 4);
    assert_eq!(report.progress.histories_available, 1);
    assert_eq!(report.progress.histories_failed, 2);
    assert_eq!(
        report.results.iter().filter(|r| r.error.is_some()).count(),
        2
    );
    let saved = f.store.paseo_workspaces().await.unwrap();
    assert_eq!(saved.len(), 2);
    assert!(
        report
            .results
            .iter()
            .find(|r| r.source_id == "missing")
            .unwrap()
            .registration_error
            .is_some()
    );
    let first = report
        .results
        .iter()
        .find(|r| r.source_id == "w")
        .unwrap()
        .imported_id
        .unwrap();
    // Resume needs neither already-copied provider logs nor a new workspace ID.
    std::fs::remove_file(f.roots.codex.join("one.jsonl")).unwrap();
    let again = f
        .store
        .import_paseo_batch(plan, BatchControl::default())
        .await
        .unwrap();
    assert_eq!(again.progress.histories_available, 1);
    assert_eq!(
        again
            .results
            .iter()
            .find(|r| r.source_id == "w")
            .unwrap()
            .imported_id,
        Some(first)
    );
    assert!(
        again
            .results
            .iter()
            .filter(|r| r.imported_id.is_some())
            .all(|r| r.reused)
    );
    assert_eq!(f.store.paseo_workspaces().await.unwrap().len(), 2);
}

#[tokio::test]
async fn batch_plan_rejects_invalid_selections_before_writing() {
    let f = Fixture::new().await;
    let catalog = discover(f.home.clone()).await.unwrap();
    assert!(
        BatchPlan::selected(&catalog, &BTreeSet::new(), f.endpoint(), f.roots.clone()).is_err()
    );
    assert!(
        BatchPlan::selected(
            &catalog,
            &BTreeSet::from(["absent".into()]),
            f.endpoint(),
            f.roots.clone()
        )
        .is_err()
    );
    let plan = BatchPlan::selected(
        &catalog,
        &BTreeSet::from(["w".into()]),
        f.endpoint(),
        f.roots.clone(),
    )
    .unwrap();
    let mut duplicate = plan.clone();
    duplicate.workspaces.push(duplicate.workspaces[0].clone());
    assert!(
        f.store
            .import_paseo_batch(duplicate, BatchControl::default())
            .await
            .is_err()
    );
    let mut relative = plan.clone();
    relative.roots.codex = "relative".into();
    assert!(
        f.store
            .import_paseo_batch(relative, BatchControl::default())
            .await
            .is_err()
    );
    let control = BatchControl::default();
    control.cancel();
    let report = f.store.import_paseo_batch(plan, control).await.unwrap();
    assert!(report.cancelled);
    assert!(report.results.is_empty());
    assert!(f.store.paseo_workspaces().await.unwrap().is_empty());
    assert_eq!(
        f.store
            .paseo_batch(report.batch_id)
            .await
            .unwrap()
            .summary
            .state,
        BatchState::Stopped
    );
}

#[tokio::test]
async fn batch_receipt_reopens_exact_diagnostics_and_retry_revalidates_saved_selection() {
    let f = Fixture::new().await;
    f.codex_log("one", &[message(1)], "");
    f.agent("unsupported", "claude", Some("w"));
    let catalog = discover(f.home.clone()).await.unwrap();
    let plan = BatchPlan::selected(
        &catalog,
        &BTreeSet::from(["w".into()]),
        f.endpoint(),
        f.roots.clone(),
    )
    .unwrap();
    let report = f
        .store
        .import_paseo_batch(plan, BatchControl::default())
        .await
        .unwrap();
    let reopened = Store::open(f.store.root().into()).await.unwrap();
    let page = reopened
        .paseo_batch_results(report.batch_id, None)
        .await
        .unwrap();
    assert_eq!(page.summary.state, BatchState::Completed);
    assert_eq!(
        serde_json::to_value(&page.results).unwrap(),
        serde_json::to_value(&report.results).unwrap()
    );
    let old = serde_json::to_value(&page).unwrap();
    // A retry cannot silently switch to newly discovered metadata.
    f.workspaces(true);
    let retried = reopened
        .retry_paseo_batch(report.batch_id, BatchControl::default())
        .await
        .unwrap();
    assert_ne!(report.batch_id, retried.batch_id);
    assert!(
        retried.results[0]
            .error
            .as_ref()
            .unwrap()
            .contains("changed")
    );
    assert!(retried.results[0].imported_id.is_none());
    assert_eq!(reopened.paseo_workspaces().await.unwrap().len(), 1);
    assert_eq!(
        old,
        serde_json::to_value(
            reopened
                .paseo_batch_results(report.batch_id, None)
                .await
                .unwrap()
        )
        .unwrap()
    );
}

#[tokio::test]
async fn batch_history_paging_and_incomplete_tail_warning() {
    let f = Fixture::new().await;
    for i in 0..70 {
        f.agent(&format!("session-{i}"), "codex", Some("w"));
    }
    f.codex_log("one", &[message(1)], "{\"incomplete\":");
    let catalog = discover(f.home.clone()).await.unwrap();
    let plan = BatchPlan::selected(
        &catalog,
        &BTreeSet::from(["w".into()]),
        f.endpoint(),
        f.roots.clone(),
    )
    .unwrap();
    let report = f
        .store
        .import_paseo_batch(plan, BatchControl::default())
        .await
        .unwrap();
    assert_eq!(report.progress.histories_available, 1);
    assert_eq!(report.progress.histories_failed, 70);
    assert_eq!(report.results[0].session_count, 71);
    assert_eq!(report.results[0].history_warnings.len(), 1);
    assert!(report.results[0].histories_finished);
}

#[tokio::test]
async fn batch_cancellation_finishes_current_snapshot_and_resume_keeps_it() {
    let f = Fixture::new().await;
    // The last registered session is visited first; a substantial copy lets
    // cancellation arrive after it starts, without timing-dependent sleeps.
    f.agent("two", "codex", Some("w"));
    f.codex_log("two", &(0..10_000).map(message).collect::<Vec<_>>(), "");
    f.codex_log("one", &[message(1)], "");
    let catalog = discover(f.home.clone()).await.unwrap();
    let plan = BatchPlan::selected(
        &catalog,
        &BTreeSet::from(["w".into()]),
        f.endpoint(),
        f.roots.clone(),
    )
    .unwrap();
    let control = BatchControl::default();
    let watch = control.clone();
    let cancel = async move {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if watch.progress().current_session.as_deref() == Some("two") {
                    watch.cancel();
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("observed in-flight copy");
    };
    let (report, ()) = tokio::join!(f.store.import_paseo_batch(plan.clone(), control), cancel);
    let report = report.unwrap();
    assert!(report.cancelled);
    assert_eq!(report.progress.histories_available, 1);
    assert_eq!(report.progress.workspaces_done, 0);
    assert!(!report.results[0].histories_finished);
    let saved = report.results[0].imported_id.unwrap();
    let sessions = f.store.paseo_sessions(saved, None).await.unwrap();
    let two = sessions.iter().find(|s| s.agent.id == "two").unwrap();
    let history = f.store.paseo_history(two.id, 0).await.unwrap();
    assert_eq!(history.status.rows, 10_000);
    std::fs::remove_file(f.roots.codex.join("two.jsonl")).unwrap();
    let resumed = f
        .store
        .import_paseo_batch(plan, BatchControl::default())
        .await
        .unwrap();
    assert!(!resumed.cancelled);
    assert!(resumed.results[0].reused);
    assert_eq!(resumed.progress.histories_available, 2);
    assert_eq!(resumed.progress.histories_failed, 0);
    assert!(resumed.results[0].histories_finished);
}

#[tokio::test]
async fn batch_cancelled_awaiter_keeps_worker_lock_and_publishes_stopped_receipt() {
    let f = Fixture::new().await;
    f.codex_log("one", &(0..60_000).map(message).collect::<Vec<_>>(), "");
    let catalog = discover(f.home.clone()).await.unwrap();
    let plan = BatchPlan::selected(
        &catalog,
        &BTreeSet::from(["w".into()]),
        f.endpoint(),
        f.roots.clone(),
    )
    .unwrap();
    let control = BatchControl::default();
    let watch = control.clone();
    let store = f.store.clone();
    let task = tokio::spawn(async move { store.import_paseo_batch(plan, control).await });
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while watch.progress().current_session.is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    task.abort();
    let _cancelled = task.await;
    let active = f.store.paseo_batches(None).await.unwrap().remove(0);
    assert!(
        active.writer_active,
        "the owner survives its cancelled awaiter"
    );
    assert!(
        f.store
            .retry_paseo_batch(active.id, BatchControl::default())
            .await
            .is_err()
    );
    watch.cancel();
    let receipt = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let receipt = f.store.paseo_batch_results(active.id, None).await.unwrap();
            if receipt.summary.state != BatchState::Running {
                break receipt;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(receipt.summary.state, BatchState::Stopped);
    assert_eq!(receipt.summary.progress.histories_available, 1);
    assert!(!receipt.results[0].histories_finished);
    let session = f
        .store
        .paseo_sessions(receipt.results[0].imported_id.unwrap(), None)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(
        f.store
            .paseo_history(session.id, 0)
            .await
            .unwrap()
            .status
            .rows,
        60_000
    );
}

#[tokio::test]
async fn preview_import_identity_paging_and_no_secret_copy() {
    let f = Fixture::new().await;
    for n in 0..70 {
        f.agent(&format!("agent-{n}"), "codex", None);
    }
    f.agent("orphan", "codex", Some("unknown"));
    let preview = discover(f.home.clone()).await.unwrap();
    assert_eq!(preview.candidates[0].sessions.len(), 71);
    assert_eq!(preview.unassociated_sessions, 1);
    assert!(
        !serde_json::to_string(&preview)
            .unwrap()
            .contains("DO_NOT_COPY")
    );
    let (a, b) = tokio::join!(
        f.store.import_paseo_workspace(&preview, "w", f.endpoint()),
        f.store.import_paseo_workspace(&preview, "w", f.endpoint())
    );
    let a = a.unwrap();
    assert_eq!(a.id, b.unwrap().id);
    assert!(a.workspace_id.is_some());
    assert_eq!(a.source.name(), "中文 title");
    assert_eq!(a.session_count, 71);
    assert_eq!(f.store.paseo_workspaces().await.unwrap().len(), 1);
    let mut before = None;
    let mut ids = BTreeSet::new();
    loop {
        let sessions = f.store.paseo_sessions(a.id, before).await.unwrap();
        if sessions.is_empty() {
            break;
        }
        before = sessions.last().map(|s| s.position);
        for s in sessions {
            assert!(ids.insert(s.agent.id));
        }
    }
    assert_eq!(ids.len(), 71);
    assert!(!f.endpoint().socket.exists());
}
#[tokio::test]
async fn missing_workspace_retains_history_bindings_and_stale_preview_refuses_writes() {
    let f = Fixture::new().await;
    let preview = discover(f.home.clone()).await.unwrap();
    f.workspaces(true);
    assert!(
        f.store
            .import_paseo_workspace(&preview, "w", f.endpoint())
            .await
            .unwrap_err()
            .to_string()
            .contains("changed")
    );
    assert!(f.store.paseo_workspaces().await.unwrap().is_empty());
    let imported = f.imported().await;
    assert!(imported.workspace_id.is_none());
    assert!(imported.registration_error.is_some());
    assert_eq!(
        f.store
            .paseo_sessions(imported.id, None)
            .await
            .unwrap()
            .len(),
        1
    );
}
#[tokio::test]
async fn malformed_duplicate_and_symlink_sources_fail_explicitly() {
    let f = Fixture::new().await;
    std::fs::copy(
        f.home.join("agents/project/one.json"),
        f.home.join("agents/project/copy.json"),
    )
    .unwrap();
    assert!(
        discover(f.home.clone())
            .await
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
    std::fs::remove_file(f.home.join("agents/project/copy.json")).unwrap();
    std::os::unix::fs::symlink(
        f.home.join("agents/project/one.json"),
        f.home.join("agents/project/link.json"),
    )
    .unwrap();
    assert!(
        discover(f.home.clone())
            .await
            .unwrap_err()
            .to_string()
            .contains("symlink")
    );
    std::fs::remove_file(f.home.join("agents/project/link.json")).unwrap();
    std::fs::write(f.home.join("agents/project/one.json"), "{").unwrap();
    assert!(discover(f.home.clone()).await.is_err());
}

#[tokio::test]
async fn unrelated_session_changes_do_not_invalidate_workspace_preview() {
    let f = Fixture::new().await;
    f.agent("outside", "codex", Some("unregistered"));
    let preview = discover(f.home.clone()).await.unwrap();
    let path = f.home.join("agents/project/outside.json");
    let mut value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["lastStatus"] = json!("running");
    f.write(path, value);
    f.store
        .import_paseo_workspace(&preview, "w", f.endpoint())
        .await
        .unwrap();
    let session = f
        .store
        .paseo_sessions(f.store.paseo_workspaces().await.unwrap()[0].id, None)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(session.agent.id, "one");
}

#[tokio::test]
async fn transaction_failure_rolls_back_registration_and_all_session_metadata() {
    let f = Fixture::new().await;
    f.store.job(|db| {db.execute_batch("CREATE TRIGGER reject_import BEFORE INSERT ON paseo_sessions BEGIN SELECT RAISE(FAIL,'injected copy failure'); END;")?;Ok(())}).await.unwrap();
    let error = f
        .store
        .import_paseo_workspace(&discover(f.home.clone()).await.unwrap(), "w", f.endpoint())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("injected copy failure"));
    assert!(f.store.paseo_workspaces().await.unwrap().is_empty());
    assert!(f.store.workspaces(None).await.unwrap().is_empty());
}
#[tokio::test]
async fn codex_complete_snapshot_is_paged_durable_and_preserves_raw_records() {
    let f = Fixture::new().await;
    let mut rows: Vec<_> = (0..70).map(message).collect();
    rows.push(json!({"type":"response_item","payload":{"type":"function_call","name":"exec","arguments":"{\"command\":\"ls\"}","call_id":"c"}}));
    rows.push(json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"c","output":"hello\nworld"}}));
    rows.push(json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_image","image_url":"data:image/png;base64,aGVsbG8="}]}}));
    rows.push(json!({"type":"future_record","payload":{"keep":"opaque"}}));
    let log = f.codex_log("one", &rows, "");
    let original = std::fs::read(&log).unwrap();
    let session = f.session().await;
    let status = f
        .store
        .snapshot_paseo_history(session.id, f.roots.clone())
        .await
        .unwrap();
    assert_eq!(status.rows, 73);
    assert_eq!(status.source_records, 75);
    let mut after = 0;
    let mut count = 0;
    loop {
        let page = f.store.paseo_history(session.id, after).await.unwrap();
        assert!(page.items.len() <= 32);
        for i in &page.items {
            i.content().unwrap();
        }
        count += page.items.len();
        if let Some(next) = page.next {
            assert!(next > after);
            after = next;
        } else {
            break;
        }
    }
    assert_eq!(count, 73);
    assert_eq!(std::fs::read(&log).unwrap(), original);
    assert_eq!(
        f.store.paseo_source_record(session.id, 75).await.unwrap()["payload"]["keep"],
        "opaque"
    );
    std::fs::remove_file(log).unwrap();
    assert_eq!(
        f.store
            .snapshot_paseo_history(session.id, f.roots.clone())
            .await
            .unwrap()
            .rows,
        73
    );
    let path = f
        .store
        .root()
        .join("paseo-history")
        .join(format!("{}.sqlite", session.id));
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}
#[tokio::test]
async fn incomplete_final_line_is_visible_and_corrupt_interior_never_publishes() {
    let f = Fixture::new().await;
    f.codex_log("one", &[message(0)], "{\"type\":");
    let session = f.session().await;
    let status = f
        .store
        .snapshot_paseo_history(session.id, f.roots.clone())
        .await
        .unwrap();
    assert!(status.warning.is_some());
    assert_eq!(status.rows, 1);
    f.agent("two", "codex", Some("w"));
    // Separate store: repeated imports intentionally retain the original snapshot.
    let other = Store::open(f.root.join("other-controller")).await.unwrap();
    let imported = other
        .import_paseo_workspace(&discover(f.home.clone()).await.unwrap(), "w", f.endpoint())
        .await
        .unwrap();
    let two = other
        .paseo_sessions(imported.id, None)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.agent.id == "two")
        .unwrap();
    let log = f.codex_log("two", &[], "bad json\n");
    assert!(
        other
            .snapshot_paseo_history(two.id, f.roots.clone())
            .await
            .is_err()
    );
    assert!(
        !other
            .root()
            .join("paseo-history")
            .join(format!("{}.sqlite", two.id))
            .exists()
    );
    assert_eq!(
        std::fs::read_dir(other.root().join("paseo-history"))
            .unwrap()
            .count(),
        0
    );
    std::fs::write(
        log,
        format!(
            "{}\n",
            json!({"type":"session_meta","payload":{"id":"WRONG","cwd":f.cwd}})
        ),
    )
    .unwrap();
    assert!(
        other
            .snapshot_paseo_history(two.id, f.roots.clone())
            .await
            .unwrap_err()
            .to_string()
            .contains("binding")
    );
}
#[tokio::test]
async fn dsh_concatenated_zstd_frames_and_workspace_failure_report() {
    let f = Fixture::new().await;
    f.agent("one", "deepseek-harness", Some("w"));
    f.agent("unsupported", "claude", Some("w"));
    let dir = f
        .roots
        .dsh
        .join("sessions")
        .join(history::project_key(f.cwd.to_str().unwrap()))
        .join("one");
    std::fs::create_dir_all(&dir).unwrap();
    let records = [
        json!({"type":"session","version":3,"id":"one","cwd":f.cwd}),
        json!({"type":"user/message","data":{"content":[{"type":"text","text":"迁移测试"}]}}),
        json!({"type":"assistant/message","data":{"message":{"content":[{"type":"text","text":"## DSH migrated"},{"type":"reasoning","text":"think"}]}}}),
        json!({"type":"tool/call","data":{"name":"bash","callId":"c","arguments":"{}"}}),
        json!({"type":"tool/result","data":{"message":{"content":[{"type":"tool-result","toolCallId":"c","content":[{"type":"text","text":"result"}]}]}}}),
    ];
    let mut compressed = Vec::new();
    for record in records {
        compressed
            .extend(zstd::stream::encode_all((record.to_string() + "\n").as_bytes(), 1).unwrap());
    }
    std::fs::write(dir.join("session.v3.jsonl.zstd"), compressed).unwrap();
    let imported = f.imported().await;
    let report = f
        .store
        .copy_paseo_workspace_history(imported.id, f.roots.clone())
        .await
        .unwrap();
    assert_eq!(report.copied, 1);
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].1.contains("supports"));
    let session = f
        .store
        .paseo_sessions(imported.id, None)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.agent.id == "one")
        .unwrap();
    let page = f.store.paseo_history(session.id, 0).await.unwrap();
    assert_eq!(page.items.len(), 5);
    for i in page.items {
        i.content().unwrap();
    }
}

#[test]
fn dsh_paths_match_utf16_and_truncation_contract() {
    assert_eq!(
        history::encoded("你好😀/~"),
        "~4F60~597D~D83D~DE00~002F~007E"
    );
    assert_eq!(history::encoded(".."), "~002E~002E");
    assert_eq!(history::project_key("/a//b:中文"), "--a-b-~4E2D~6587--");
    assert_eq!(
        history::project_key(&format!("/{}", "中".repeat(100))).len(),
        255
    );
}

fn dsh_log(f: &Fixture, name: &str, version: u64, text: &str) -> PathBuf {
    let directory = f
        .roots
        .dsh
        .join("sessions")
        .join(history::project_key(f.cwd.to_str().unwrap()))
        .join("one");
    std::fs::create_dir_all(&directory).unwrap();
    let records = [
        json!({"type":"session","version":version,"id":"one","cwd":f.cwd,"createdAt":1,"isSeeded":false,"delegationDepth":0}),
        json!({"seq":0,"time":1,"type":"user/message","data":{"role":"user","source":{"kind":"user"},"content":[{"type":"text","text":"历史问题"}]}}),
        json!({"seq":1,"time":2,"type":"assistant/message","data":{"message":{"role":"assistant","source":{"kind":"model"},"content":[{"type":"text","text":text}]}}}),
        json!({"seq":2,"time":3,"type":"tool/call","data":{"name":"read","callId":"v4-call","arguments":{"path":"sample.txt"}}}),
        json!({"seq":3,"time":4,"type":"tool/result","data":{"message":{"id":"v4-result","role":"tool","toolCallId":"v4-call","source":{"kind":"tool","callId":"v4-call"},"content":[{"type":"text","text":"原始工具结果"}]}}}),
    ];
    let mut bytes = Vec::new();
    for record in records {
        let line = record.to_string() + "\n";
        if name.ends_with(".zstd") {
            bytes.extend(zstd::stream::encode_all(line.as_bytes(), 1).unwrap());
        } else {
            bytes.extend(line.as_bytes());
        }
    }
    let path = directory.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

#[tokio::test]
async fn dsh_v4_selects_latest_generation_and_preserves_native_tool_result() {
    for suffix in ["", ".zstd"] {
        let f = Fixture::new().await;
        f.agent("one", "deepseek-harness", Some("w"));
        let old = dsh_log(&f, &format!("session.v3.jsonl{suffix}"), 3, "OLD SNAPSHOT");
        let path = dsh_log(
            &f,
            &format!("session.v4.jsonl{suffix}"),
            4,
            "## 最新中文 **answer**",
        );
        let path = std::fs::canonicalize(path).unwrap();
        let original = std::fs::read(&path).unwrap();
        let old_bytes = std::fs::read(&old).unwrap();
        let session = f.session().await;
        let status = f
            .store
            .snapshot_paseo_history(session.id, f.roots.clone())
            .await
            .unwrap();
        assert_eq!(status.source, std::fs::canonicalize(&path).unwrap());
        assert_eq!(status.source_records, 5);
        assert!(status.warning.is_none());
        let page = f.store.paseo_history(session.id, 0).await.unwrap();
        assert_eq!(page.items.len(), 4);
        assert_eq!(page.items[1].text().unwrap(), "## 最新中文 **answer**");
        for item in &page.items {
            item.content().unwrap();
        }
        let raw = f.store.paseo_source_record(session.id, 5).await.unwrap();
        assert_eq!(raw["data"]["message"]["role"], "tool");
        assert_eq!(raw["data"]["message"]["content"][0]["text"], "原始工具结果");
        assert_eq!(std::fs::read(&path).unwrap(), original);
        std::fs::remove_file(&path).unwrap();
        let reopened = Store::open(f.store.root().to_path_buf()).await.unwrap();
        assert_eq!(
            reopened
                .paseo_history(session.id, 0)
                .await
                .unwrap()
                .items
                .len(),
            4
        );
        assert_eq!(
            reopened
                .snapshot_paseo_history(session.id, f.roots.clone())
                .await
                .unwrap()
                .source,
            path
        );
        assert_eq!(std::fs::read(old).unwrap(), old_bytes);
    }
}

#[tokio::test]
async fn dsh_refuses_unsupported_or_corrupt_latest_generation_instead_of_old_snapshot() {
    for (name, version, corrupt, expected) in [
        ("session.v12.jsonl", 12, false, "unsupported"),
        ("session.v4.jsonl", 3, false, "version"),
        ("session.v4.jsonl", 4, true, "JSON"),
        ("session.v4.jsonl.zstd", 4, false, "encodings"),
    ] {
        let f = Fixture::new().await;
        f.agent("one", "deepseek-harness", Some("w"));
        dsh_log(&f, "session.v3.jsonl", 3, "OLD SNAPSHOT");
        let latest = dsh_log(&f, name, version, "LATEST");
        if corrupt {
            std::fs::write(&latest, b"not JSON\n").unwrap();
        }
        let session = f.session().await;
        let error = f
            .store
            .snapshot_paseo_history(session.id, f.roots.clone())
            .await
            .unwrap_err();
        assert!(error.to_string().contains(expected), "{name}: {error}");
        assert_eq!(
            std::fs::read_dir(f.store.root().join("paseo-history"))
                .unwrap()
                .count(),
            0
        );
    }
}

#[tokio::test]
async fn linked_git_workspace_is_registered_in_place_without_changing_branch_or_files() {
    let f = Fixture::new().await;
    fn git(cwd: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    git(&f.cwd, &["init", "-q"]);
    git(
        &f.cwd,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-qm",
            "fixture",
        ],
    );
    let linked = f.root.join("linked");
    git(
        &f.cwd,
        &[
            "worktree",
            "add",
            "-qb",
            "imported-branch",
            linked.to_str().unwrap(),
        ],
    );
    std::fs::write(linked.join("untracked.txt"), "exact uncommitted bytes").unwrap();
    f.write(f.home.join("projects/workspaces.json"),json!([{"workspaceId":"w","projectId":"p","cwd":linked,"kind":"worktree","displayName":"Linked worktree","branch":"imported-branch"}]));
    let imported = f.imported().await;
    let registered = f
        .store
        .workspace(imported.workspace_id.unwrap())
        .await
        .unwrap();
    let overview = registered.inspect().await.unwrap();
    let git = overview.git.unwrap();
    assert!(git.linked_worktree);
    assert_eq!(git.branch.as_deref(), Some("imported-branch"));
    assert_eq!(
        std::fs::read_to_string(linked.join("untracked.txt")).unwrap(),
        "exact uncommitted bytes"
    );
    assert_eq!(imported.source.branch.as_deref(), Some("imported-branch"));
}

#[tokio::test]
async fn binding_reopens_and_only_one_dispatch_attempt_wins() {
    let f = Fixture::new().await;
    let session = f.session().await;
    let intent = fork_intent(&f, &session).await;
    f.store
        .save_paseo_continuation(intent.clone(), None)
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        f.store.attempt_paseo_continuation(session.id, intent.id),
        f.store.attempt_paseo_continuation(session.id, intent.id)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    let reopened = Store::open(f.store.root().into()).await.unwrap();
    let saved = reopened
        .prepare_paseo_continuation(session.id)
        .await
        .unwrap();
    assert_eq!(saved.id, intent.id);
    assert!(saved.attempted);
    assert!(
        reopened
            .attempt_paseo_continuation(session.id, intent.id)
            .await
            .is_err()
    );
    assert!(
        reopened
            .reprepare_paseo_continuation(session.id, intent.id)
            .await
            .unwrap_err()
            .to_string()
            .contains("already attempted")
    );
}

async fn fork_intent(f: &Fixture, session: &Session) -> continuation::Intent {
    let parent = f.store.paseo_workspace(session.parent).await.unwrap();
    continuation::Intent {
        id: Uuid::new_v4(),
        session_id: session.id,
        workspace_id: parent.workspace_id.unwrap(),
        thread_id: Uuid::new_v4().to_string(),
        cwd: f.cwd.to_str().unwrap().into(),
        path: f.root.join("saved.jsonl"),
        target: crate::routing::Target {
            endpoint: f.endpoint(),
            host_id: Uuid::new_v4(),
            admission_generation: 0,
        },
        attempted: false,
    }
}

#[tokio::test]
async fn replacing_unattempted_binding_requires_current_id_and_fences_old_dispatch() {
    let f = Fixture::new().await;
    let session = f.session().await;
    let original = fork_intent(&f, &session).await;
    f.store
        .save_paseo_continuation(original.clone(), None)
        .await
        .unwrap();
    let mut a = original.clone();
    a.id = Uuid::new_v4();
    a.target.admission_generation += 1;
    let mut b = a.clone();
    b.id = Uuid::new_v4();
    let (ra, rb) = tokio::join!(
        f.store.save_paseo_continuation(a, Some(original.id)),
        f.store.save_paseo_continuation(b, Some(original.id))
    );
    assert_ne!(ra.is_ok(), rb.is_ok());
    let winner = ra.or(rb).unwrap();
    let reopened = Store::open(f.store.root().into()).await.unwrap();
    let saved = reopened
        .prepare_paseo_continuation(session.id)
        .await
        .unwrap();
    assert_eq!(saved.id, winner.id);
    assert!(!saved.attempted);
    let connection = crate::client::Connection {
        socket: saved.target.endpoint.socket.clone(),
        lease: fleetpane_runtime::Lease {
            host_id: saved.target.host_id,
            generation: 0,
            controller_id: Uuid::new_v4(),
        },
    };
    assert!(
        reopened
            .dispatch_paseo_continuation(session.id, original.id, &connection)
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("binding changed")
    );
    assert!(
        reopened
            .reprepare_paseo_continuation(session.id, original.id)
            .await
            .is_err()
    );
    assert!(
        !reopened
            .paseo_continuation(session.id)
            .await
            .unwrap()
            .unwrap()
            .attempted
    );
    // A failed fresh observation must leave the previously valid saved binding.
    assert!(
        reopened
            .reprepare_paseo_continuation(session.id, saved.id)
            .await
            .is_err()
    );
    assert_eq!(
        reopened
            .paseo_continuation(session.id)
            .await
            .unwrap()
            .unwrap()
            .id,
        saved.id
    );
}

#[tokio::test]
async fn reprepare_and_dispatch_attempt_cannot_both_win() {
    let f = Fixture::new().await;
    let session = f.session().await;
    let original = fork_intent(&f, &session).await;
    f.store
        .save_paseo_continuation(original.clone(), None)
        .await
        .unwrap();
    let mut replacement = original.clone();
    replacement.id = Uuid::new_v4();
    let replacement_id = replacement.id;
    let (reprepare, attempt) = tokio::join!(
        f.store
            .save_paseo_continuation(replacement, Some(original.id)),
        f.store.attempt_paseo_continuation(session.id, original.id)
    );
    assert_ne!(reprepare.is_ok(), attempt.is_ok());
    let saved = f
        .store
        .paseo_continuation(session.id)
        .await
        .unwrap()
        .unwrap();
    if reprepare.is_ok() {
        assert_eq!(saved.id, replacement_id);
        assert!(!saved.attempted);
    } else {
        assert_eq!(saved.id, original.id);
        assert!(saved.attempted);
    }
}
