use super::*;
use fleetpane_runtime::{State, lifecycle::ActiveTurn, stopping::Phase};
use serde_json::json;
use uuid::Uuid;

fn thread() -> Thread {
    serde_json::from_value(json!({
        "id":"a", "cwd":"/synthetic", "preview":"", "status":{"type":"idle"}, "turns":[]
    }))
    .unwrap()
}

fn active(snapshot: &mut Snapshot) {
    snapshot.activity.turns.push(ActiveTurn {
        thread_id: "a".into(),
        turn_id: Some("one".into()),
    });
}

#[test]
fn working_uses_host_activity_even_without_a_loaded_turn() {
    let thread = thread();
    let mut snapshot = State::new(1, json!({})).snapshot();
    active(&mut snapshot);
    assert_eq!(
        Progress::observed(&thread, &snapshot),
        Progress::Working {
            turn_id: Some("one")
        }
    );
    snapshot.activity.turns[0].turn_id = None;
    assert_eq!(
        Progress::observed(&thread, &snapshot),
        Progress::Working { turn_id: None }
    );
    snapshot.activity.turns[0].thread_id = "other".into();
    assert_eq!(Progress::observed(&thread, &snapshot), Progress::Idle);

    let mut archived = thread.clone();
    archived.status.kind = "archived".into();
    snapshot.activity.turns[0].thread_id = "a".into();
    assert!(matches!(
        Progress::observed(&archived, &snapshot),
        Progress::Working { .. }
    ));
}

#[test]
fn requests_match_session_and_exact_known_turn_and_accept_acp_without_turn_id() {
    let thread = thread();
    let mut snapshot = State::new(1, json!({})).snapshot();
    active(&mut snapshot);
    snapshot.pending_requests = vec![
        json!({"params":{"threadId":"other","turnId":"one"}}),
        json!({"params":{"threadId":"a","turnId":"old"}}),
    ];
    assert!(matches!(
        Progress::observed(&thread, &snapshot),
        Progress::Working { .. }
    ));
    snapshot
        .pending_requests
        .push(json!({"params":{"threadId":"a","turnId":"one"}}));
    snapshot
        .pending_requests
        .push(json!({"params":{"threadId":"a"}}));
    assert_eq!(
        Progress::observed(&thread, &snapshot),
        Progress::Waiting { requests: 2 }
    );
    snapshot.pending_requests.clear();
    assert!(matches!(
        Progress::observed(&thread, &snapshot),
        Progress::Working { .. }
    ));
}

#[test]
fn stop_wait_survives_terminal_history_and_missing_active_turn() {
    let thread = thread();
    let mut snapshot = State::new(1, json!({})).snapshot();
    let stop = Stop {
        thread_id: "a".into(),
        turn_id: "one".into(),
        operation_id: Uuid::new_v4(),
        phase: Phase::Dispatching,
        terminal_status: Some("interrupted".into()),
        error: None,
    };
    snapshot.activity.stopping = Some(vec![stop.clone()]);
    assert_eq!(
        Progress::observed(&thread, &snapshot),
        Progress::Stopping(&stop)
    );
    for phase in [Phase::Rejected, Phase::Unknown] {
        snapshot.activity.stopping.as_mut().unwrap()[0].phase = phase;
        let progress = Progress::observed(&thread, &snapshot);
        assert!(!progress.label().contains("working"));
    }
    snapshot.activity.stopping = Some(vec![]);
    assert_eq!(Progress::observed(&thread, &snapshot), Progress::Idle);
}

#[test]
fn stale_history_never_claims_the_agent_is_still_working() {
    let mut thread = thread();
    thread.turns.push(
        serde_json::from_value(json!({"id":"one","status":"inProgress","items":[]})).unwrap(),
    );
    let mut snapshot = State::new(1, json!({})).snapshot();
    assert_eq!(
        Progress::observed(&thread, &snapshot),
        Progress::Synchronizing
    );
    thread.turns[0].status = "unknown".into();
    assert_eq!(Progress::observed(&thread, &snapshot), Progress::Unknown);
    thread.status.kind = "archived".into();
    assert_eq!(Progress::observed(&thread, &snapshot), Progress::Archived);
    snapshot.fault = Some("provider gone".into());
    active(&mut snapshot);
    assert_eq!(
        Progress::observed(&thread, &snapshot),
        Progress::Unavailable
    );
}
