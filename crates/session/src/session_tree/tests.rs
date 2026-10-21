use super::*;
use fleetpane_runtime::{Lease, Request, Response};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn value(id: &str, parent: Option<&str>) -> Value {
    json!({"id":id,"sessionId":"tree","parentThreadId":parent,"forkedFromId":null,"cwd":"/tmp","name":null,"preview":id,"ephemeral":false,"status":{"type":"idle"},"turns":[]})
}
fn metadata(id: &str, parent: Option<&str>) -> Metadata {
    serde_json::from_value(value(id, parent)).unwrap()
}
fn state() -> Snapshot {
    serde_json::from_value(json!({"protocol":fleetpane_runtime::PROTOCOL_VERSION,"host_id":Uuid::new_v4(),"host_pid":1,"provider_pid":2,"provider_info":{},"owner":null,"sequence":8,"operation_ids":[],"journal_total":0,"admission":{"generation":0,"mode":"ready"},"activity":{"terminal_ids":[],"turns":[],"in_flight_operations":0,"unknown_operations":0},"pending_requests":[],"fault":null})).unwrap()
}
fn index(rows: Vec<Metadata>) -> Index {
    rows.into_iter()
        .map(|m| (m.id.clone(), (m, Some(false))))
        .collect()
}

#[test]
fn metadata_requires_known_parent_and_empty_history() {
    let mut v = value("root", None);
    assert!(serde_json::from_value::<Metadata>(v.clone()).is_ok());
    v.as_object_mut().unwrap().remove("parentThreadId");
    assert!(serde_json::from_value::<Metadata>(v).is_err());
    let mut v = value("root", None);
    v["turns"] = json!([{"id":"must-not-import"}]);
    assert!(serde_json::from_value::<Metadata>(v).is_err());
    let mut m = metadata("root", None);
    m.cwd = "relative".into();
    assert!(m.validate().is_err());
}

#[test]
fn parent_edges_include_archived_and_loaded_children_but_not_forks() {
    let mut fork = metadata("fork", None);
    fork.forked_from_id = Some("root".into());
    let mut idx = index(vec![
        metadata("root", None),
        metadata("child", Some("root")),
        metadata("grandchild", Some("child")),
        fork,
    ]);
    idx.get_mut("child").unwrap().1 = Some(true);
    idx.get_mut("grandchild").unwrap().1 = None;
    let mut snapshot = state();
    snapshot.activity.turns.push(ActiveTurn {
        thread_id: "child".into(),
        turn_id: Some("turn-original".into()),
    });
    snapshot.pending_requests = vec![
        json!({"params":{"threadId":"child"}}),
        json!({"params":{"threadId":"missing"}}),
        json!({"params":{}}),
    ];
    let view = project(
        &idx,
        &BTreeSet::from(["grandchild".into(), "other".into()]),
        &snapshot,
        "root",
        2,
    )
    .unwrap();
    assert_eq!(
        view.rows
            .iter()
            .map(|r| (r.metadata.id.as_str(), r.depth))
            .collect::<Vec<_>>(),
        vec![("root", 0), ("child", 1), ("grandchild", 2)]
    );
    assert_eq!(view.rows[1].archived, Some(true));
    assert_eq!(view.rows[2].archived, None);
    assert_eq!(
        view.rows[1]
            .active_turn
            .as_ref()
            .unwrap()
            .turn_id
            .as_deref(),
        Some("turn-original")
    );
    assert_eq!(view.rows[1].pending_requests, 1);
    assert!(view.rows[2].loaded);
    assert_eq!(view.unassigned_requests, 2);
    assert_eq!(view.other_loaded_contexts, 1);
    assert_eq!((view.sequence_before, view.sequence_after), (2, 8));
    assert!(view.unresolved_same_session.is_empty());
    assert_eq!(view.warnings.len(), 2);
}

#[test]
fn incomplete_parent_chain_is_visible_and_cycles_or_inconsistent_session_ids_fail() {
    let mut idx = index(vec![
        metadata("root", None),
        metadata("orphan", Some("unknown")),
    ]);
    let view = project(&idx, &BTreeSet::new(), &state(), "root", 0).unwrap();
    assert_eq!(view.unresolved_same_session, vec!["orphan"]);
    assert_eq!(view.warnings.len(), 3);
    idx.get_mut("orphan").unwrap().0.parent_thread_id = Some("orphan".into());
    assert!(project(&idx, &BTreeSet::new(), &state(), "root", 0).is_err());
    idx.get_mut("orphan").unwrap().0.parent_thread_id = Some("root".into());
    idx.get_mut("orphan").unwrap().0.session_id = "other-tree".into();
    assert!(project(&idx, &BTreeSet::new(), &state(), "root", 0).is_err());
    idx.get_mut("orphan").unwrap().0.session_id = "tree".into();
    idx.get_mut("root").unwrap().0.parent_thread_id = Some("orphan".into());
    assert!(project(&idx, &BTreeSet::new(), &state(), "root", 0).is_err());
}

#[test]
fn depth_and_inventory_limits_fail_without_partial_adoption() {
    let rows = (0..=65)
        .map(|n| {
            metadata(
                &n.to_string(),
                (n > 0).then(|| (n - 1).to_string()).as_deref(),
            )
        })
        .collect();
    assert!(project(&index(rows), &BTreeSet::new(), &state(), "0", 0).is_err());
    let mut idx = Index::new();
    let mut bytes = MAX_BYTES - 1;
    assert!(insert(&mut idx, &mut bytes, metadata("a", None), Some(false)).is_err());
    assert!(idx.is_empty());
    assert_eq!(bytes, MAX_BYTES - 1);
    bytes = 2;
    insert(&mut idx, &mut bytes, metadata("a", None), Some(false)).unwrap();
    assert!(insert(&mut idx, &mut bytes, metadata("a", None), Some(true)).is_err());
    let mut cursor = None;
    let mut seen = BTreeSet::new();
    advance(&mut cursor, &mut seen, Some("next".into())).unwrap();
    assert!(advance(&mut cursor, &mut seen, Some("next".into())).is_err());
    assert!(advance(&mut cursor, &mut seen, Some(String::new())).is_err());
    assert!(fleetpane_runtime::is_query_method("thread/loaded/list"));
}

fn query(value: Value) -> Response {
    Response::Query {
        message: json!({"id":"observational","result":value}),
        sequence: 8,
    }
}
fn page(data: Vec<Value>) -> Response {
    query(json!({"data":data,"nextCursor":null}))
}

async fn scripted(
    first: Snapshot,
    responses: Vec<Response>,
) -> (Result<View, Error>, Vec<Request>) {
    let root = std::env::temp_dir().join(format!("fp-tree-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let socket = root.join("control.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let host = first.host_id;
    let task = tokio::spawn(async move {
        let mut requests = vec![];
        for response in std::iter::once(Response::Snapshot(first)).chain(responses) {
            let (stream, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            requests.push(serde_json::from_str(&line).unwrap());
            let mut bytes = serde_json::to_vec(&response).unwrap();
            bytes.push(b'\n');
            reader.get_mut().write_all(&bytes).await.unwrap();
        }
        requests
    });
    let result = inspect(&socket, host, "root").await;
    let requests = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
    (result, requests)
}
fn owned() -> Snapshot {
    let mut s = state();
    s.owner = Some(Lease {
        host_id: s.host_id,
        controller_id: Uuid::new_v4(),
        generation: 1,
    });
    s
}

#[tokio::test]
async fn collection_borrows_one_lease_and_only_dispatches_bounded_metadata_queries() {
    let first = owned();
    let mut last = first.clone();
    last.sequence = 12;
    let (result, requests) = scripted(
        first.clone(),
        vec![
            page(vec![json!("root"), json!("ephemeral")]),
            page(vec![value("root", None)]),
            page(vec![]),
            query(json!({"thread":value("ephemeral",Some("root"))})),
            page(vec![json!("root"), json!("ephemeral")]),
            Response::Snapshot(last),
        ],
    )
    .await;
    let view = result.unwrap();
    assert_eq!(view.rows.len(), 2);
    assert_eq!(view.rows[1].archived, None);
    assert_eq!((view.sequence_before, view.sequence_after), (8, 12));
    let mut methods = vec![];
    for request in requests {
        match request {
            Request::Snapshot => {}
            Request::Query {
                lease,
                method,
                params,
            } => {
                assert_eq!(Some(lease), first.owner);
                assert!(fleetpane_runtime::is_query_method(&method));
                if method == "thread/list" {
                    assert_eq!(params["limit"], 100);
                    assert_eq!(params["sourceKinds"], json!(SOURCES));
                    assert_eq!(params["modelProviders"], json!([]));
                }
                if method == "thread/read" {
                    assert_eq!(params["includeTurns"], false);
                    assert_eq!(params["threadId"], "ephemeral");
                }
                methods.push(method);
            }
            _ => panic!("Inspection must never claim or mutate"),
        }
    }
    assert_eq!(
        methods,
        vec![
            "thread/loaded/list",
            "thread/list",
            "thread/list",
            "thread/read",
            "thread/loaded/list"
        ]
    );
}

#[tokio::test]
async fn collection_rejects_owner_host_or_loaded_inventory_changes_without_partial_view() {
    for fault in ["owner", "host", "loaded"] {
        let first = owned();
        let mut last = first.clone();
        if fault == "owner" {
            last.owner.as_mut().unwrap().generation += 1;
        }
        if fault == "host" {
            last.host_id = Uuid::new_v4();
        }
        let mut responses = vec![
            page(vec![json!("root")]),
            page(vec![value("root", None)]),
            page(vec![]),
            page(if fault == "loaded" {
                vec![]
            } else {
                vec![json!("root")]
            }),
        ];
        if fault != "loaded" {
            responses.push(Response::Snapshot(last));
        }
        let (result, requests) = scripted(first, responses).await;
        assert!(result.is_err(), "{fault}");
        assert!(
            requests
                .iter()
                .all(|r| matches!(r, Request::Query { .. } | Request::Snapshot))
        );
    }
}

#[tokio::test]
async fn overlapping_archive_membership_is_an_error_instead_of_a_guessed_tree() {
    let (result, _) = scripted(
        owned(),
        vec![
            page(vec![]),
            page(vec![value("root", None)]),
            page(vec![value("root", None)]),
        ],
    )
    .await;
    assert!(result.unwrap_err().to_string().contains("overlaps"));
}
