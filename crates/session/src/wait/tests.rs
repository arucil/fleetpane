use super::*;
use fleetpane_runtime::{Event, Lease, Request, Response};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn turn(id: &str, status: &str) -> Turn {
    serde_json::from_value(json!({"id":id,"status":status,"items":[{"id":"message","type":"agentMessage","text":"large content is not the wait result"}],"error":null,"stopReason":"refusal"})).unwrap()
}

fn lease(host_id: Uuid) -> Lease {
    serde_json::from_value(json!({"host_id":host_id,"controller_id":Uuid::new_v4(),"generation":1}))
        .unwrap()
}

fn snapshot(owner: &Lease, sequence: u64) -> Snapshot {
    serde_json::from_value(json!({"protocol":fleetpane_runtime::PROTOCOL_VERSION,"host_id":owner.host_id,"host_pid":1,"provider_pid":2,"provider_info":{},"owner":owner,"sequence":sequence,"operation_ids":[],"journal_total":0,"admission":{"generation":0,"mode":"ready"},"activity":{"turns":[],"in_flight_operations":0,"unknown_operations":0},"pending_requests":[],"fault":null})).unwrap()
}

#[test]
fn outcomes_preserve_failure_refusal_unknown_and_omit_transcript() {
    for status in ["completed", "failed", "interrupted", "unknown"] {
        let result = terminal(turn("target", status)).unwrap().unwrap();
        let encoded = serde_json::to_value(result).unwrap();
        assert_eq!(encoded["turn"]["status"], status);
        assert_eq!(encoded["turn"]["stopReason"], "refusal");
        assert_eq!(encoded["turn"]["items"], json!([]));
        assert_eq!(
            encoded["outcome"],
            if status == "unknown" {
                "unknown"
            } else {
                "finished"
            }
        );
    }
    assert!(terminal(turn("target", "inProgress")).unwrap().is_none());
    assert!(terminal(turn("target", "future-status")).is_err());
}

#[test]
fn attention_binds_acp_requests_to_active_turn_and_codex_turn_ids_exactly() {
    let mut state = snapshot(&lease(Uuid::new_v4()), 3);
    state.activity.turns = vec![fleetpane_runtime::lifecycle::ActiveTurn {
        thread_id: "s".into(),
        turn_id: Some("target".into()),
    }];
    state.pending_requests = vec![
        json!({"id":1,"method":"session/request_permission","params":{"threadId":"s"}}),
        json!({"id":2,"method":"item/tool/requestUserInput","params":{"threadId":"s","turnId":"target"}}),
        json!({"id":3,"params":{"threadId":"s","turnId":"old"}}),
        json!({"id":4,"params":{"threadId":"other","turnId":"target"}}),
        json!({"id":5,"params":{"threadId":"s","turnId":null}}),
    ];
    assert_eq!(
        attention(&state, "s", "target"),
        state.pending_requests[..2]
    );
    assert!(attention(&state, "s", "old").is_empty());
    state.activity.turns.clear();
    assert!(attention(&state, "s", "target").is_empty());
}

async fn scripted(
    responses: Vec<Response>,
) -> (
    Connection,
    tokio::task::JoinHandle<Vec<Request>>,
    std::path::PathBuf,
) {
    // Keep the socket below Darwin's sockaddr_un path limit.
    let root = std::path::PathBuf::from("/tmp").join(format!("fleetpane-wait-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let socket = root.join("control.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let owner = lease(Uuid::new_v4());
    let task = tokio::spawn(async move {
        let mut requests = Vec::new();
        for response in responses {
            let (stream, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            requests.push(serde_json::from_str(&line).unwrap());
            let mut frame = serde_json::to_vec(&response).unwrap();
            frame.push(b'\n');
            reader.get_mut().write_all(&frame).await.unwrap();
        }
        requests
    });
    (
        Connection {
            socket,
            lease: owner,
        },
        task,
        root,
    )
}

fn query(data: Value, sequence: u64) -> Response {
    Response::Query {
        message: json!({"id":"query:synthetic","result":data}),
        sequence,
    }
}

#[tokio::test]
async fn pages_find_the_exact_turn_and_backlog_uses_delivered_cursor_across_owner_change() {
    let owner = lease(Uuid::new_v4());
    let mut state = snapshot(&owner, 9);
    let newer_owner = lease(owner.host_id);
    state.owner = Some(newer_owner);
    let completed = Event {
        sequence: 9,
        message: json!({"method":"turn/completed","params":{"threadId":"s","turn":turn("target","failed")}}),
    };
    let (mut connection, task, root) = scripted(vec![
        Response::Snapshot(snapshot(&owner, 1)),
        query(json!({"data":[turn("newer","completed")],"nextCursor":"older"}),2),
        query(json!({"data":[turn("target","inProgress")],"nextCursor":null}),4),
        Response::Snapshot(state.clone()),
        Response::Events(vec![Event { sequence:5, message:json!({"method":"turn/completed","params":{"threadId":"other","turn":turn("target","completed")}}) }]),
        Response::Snapshot(state),
        Response::Events(vec![completed]),
    ]).await;
    connection.lease = owner;
    let report = connection.wait_turn("s", "target", 1000).await.unwrap();
    assert!(matches!(report.outcome,Outcome::Finished { turn } if turn.status=="failed"));
    assert_eq!(report.sequence, Some(9));
    let requests = task.await.unwrap();
    assert!(
        matches!(&requests[1],Request::Query {method,params,..} if method=="thread/turns/list" && params["threadId"]=="s" && params["itemsView"]=="notLoaded" && params["cursor"].is_null())
    );
    assert!(matches!(&requests[2],Request::Query {params,..} if params["cursor"]=="older"));
    assert!(matches!(&requests[4], Request::Events { after: 1, .. }));
    assert!(matches!(&requests[6], Request::Events { after: 5, .. }));
    assert!(requests.iter().all(|request| matches!(
        request,
        Request::Query { .. } | Request::Snapshot | Request::Events { .. }
    )));
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn missing_and_repeating_history_are_errors_not_completion() {
    for mut responses in [
        vec![query(json!({"data":[],"nextCursor":null}), 1)],
        vec![
            query(json!({"data":[],"nextCursor":"repeat"}), 1),
            query(json!({"data":[],"nextCursor":"repeat"}), 2),
        ],
    ] {
        let owner = lease(Uuid::new_v4());
        responses.insert(0, Response::Snapshot(snapshot(&owner, 0)));
        let (mut connection, task, root) = scripted(responses).await;
        connection.lease = owner;
        assert!(connection.wait_turn("s", "missing", 1000).await.is_err());
        task.await.unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn completion_before_a_stale_read_reply_is_still_observed() {
    let root =
        std::path::PathBuf::from("/tmp").join(format!("fleetpane-wait-race-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let socket = root.join("control.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let owner = lease(Uuid::new_v4());
    let state = snapshot(&owner, 1);
    let server = tokio::spawn(async move {
        let mut queried = false;
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            let response = match serde_json::from_str::<Request>(&line).unwrap() {
                Request::Query { .. } => {
                    queried = true;
                    // The provider captured an active turn, then emitted its
                    // completion (sequence 3) before this stale reply (4).
                    query(
                        json!({"data":[turn("target","inProgress")],"nextCursor":null}),
                        4,
                    )
                }
                Request::Snapshot => {
                    let mut s = state.clone();
                    if queried {
                        s.sequence = 4;
                    }
                    Response::Snapshot(s)
                }
                Request::Events { after, .. } => Response::Events(if after < 3 {
                    vec![Event {
                        sequence: 3,
                        message: json!({"method":"turn/completed","params":{"threadId":"s","turn":turn("target","completed")}}),
                    }]
                } else {
                    Vec::new()
                }),
                other => panic!("Observer sent mutation: {other:?}"),
            };
            let mut frame = serde_json::to_vec(&response).unwrap();
            frame.push(b'\n');
            reader.get_mut().write_all(&frame).await.unwrap();
        }
    });
    let connection = Connection {
        socket,
        lease: owner,
    };
    let report = connection.wait_turn("s", "target", 100).await.unwrap();
    server.abort();
    assert!(server.await.unwrap_err().is_cancelled());
    std::fs::remove_dir_all(root).unwrap();
    assert!(
        matches!(report.outcome, Outcome::Finished { .. }),
        "Missed completion: {report:?}"
    );
    assert_eq!(report.sequence, Some(3));
}

#[tokio::test]
async fn deadline_includes_a_stalled_initial_read_and_never_sends_cancellation() {
    let (connection, task, root) = scripted(Vec::new()).await;
    // Own an endpoint that accepts a read and never responds.
    task.await.unwrap();
    std::fs::remove_file(&connection.socket).unwrap();
    let listener = tokio::net::UnixListener::bind(&connection.socket).unwrap();
    let initial = Response::Snapshot(snapshot(&connection.lease, 0));
    let blocked = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        assert!(matches!(
            serde_json::from_str::<Request>(&line).unwrap(),
            Request::Snapshot
        ));
        let mut frame = serde_json::to_vec(&initial).unwrap();
        frame.push(b'\n');
        reader.get_mut().write_all(&frame).await.unwrap();
        drop(reader);
        let (stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        assert!(matches!(
            serde_json::from_str::<Request>(&line).unwrap(),
            Request::Query { .. }
        ));
        line.clear();
        // Timeout drops this observer's connection; it sends no Stop/answer.
        assert_eq!(reader.read_line(&mut line).await.unwrap(), 0);
    });
    let report = connection.wait_turn("s", "target", 50).await.unwrap();
    assert!(matches!(report.outcome, Outcome::TimedOut));
    assert_eq!(report.sequence, None);
    blocked.await.unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
