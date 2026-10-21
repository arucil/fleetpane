use crate::{Outcome, PROTOCOL_VERSION, Response, State, stopping::Phase};
use serde_json::{Value, json};
use uuid::Uuid;

fn setup() -> (State, crate::Lease) {
    let mut state = State::new(1, json!({}));
    let Response::Lease(lease) = state.claim(
        PROTOCOL_VERSION,
        state.snapshot().host_id,
        0,
        Uuid::new_v4(),
    ) else {
        panic!("lease")
    };
    state
        .record(json!({"method":"turn/started","params":{"threadId":"a","turn":{"id":"one"}}}))
        .unwrap();
    (state, lease)
}

fn stop(state: &mut State, lease: &crate::Lease) -> Uuid {
    let id = Uuid::new_v4();
    assert!(
        state
            .admit(
                lease,
                id,
                "turn/interrupt".into(),
                json!({"threadId":"a","turnId":"one"})
            )
            .unwrap()
    );
    id
}

fn completion(state: &mut State, thread: &str, turn: &str) {
    state.record(json!({"method":"turn/completed","params":{"threadId":thread,"turn":{"id":turn,"status":"interrupted"}}})).unwrap();
}

fn blocked(state: &mut State, lease: &crate::Lease) {
    for method in [
        "turn/start",
        "turn/steer",
        "thread/archive",
        "thread/resume",
        "turn/interrupt",
    ] {
        let error = state
            .admit(
                lease,
                Uuid::new_v4(),
                method.into(),
                json!({"threadId":"a","turnId":"one"}),
            )
            .unwrap_err();
        assert_eq!(error.code, "stop_pending", "{method}");
    }
}

#[test]
fn ack_and_exact_terminal_completion_required_in_both_orders() {
    for ack_first in [false, true] {
        let (mut state, lease) = setup();
        let id = stop(&mut state, &lease);
        if ack_first {
            state.record(json!({"id":id,"result":{}})).unwrap();
        } else {
            completion(&mut state, "a", "one");
        }
        blocked(&mut state, &lease);
        state.record(json!({"method":"thread/status/changed","params":{"threadId":"a","status":{"type":"idle"}}})).unwrap();
        completion(&mut state, "a", "other");
        completion(&mut state, "other", "one");
        blocked(&mut state, &lease);
        assert!(
            state
                .admit(
                    &lease,
                    id,
                    "turn/interrupt".into(),
                    json!({"threadId":"a","turnId":"one"})
                )
                .is_ok_and(|fresh| !fresh)
        );
        assert!(state.admit_query(&lease, "thread/read").is_ok());
        assert!(
            state
                .admit(
                    &lease,
                    Uuid::new_v4(),
                    "turn/start".into(),
                    json!({"threadId":"b"})
                )
                .is_ok()
        );
        if ack_first {
            completion(&mut state, "a", "one");
        } else {
            state.record(json!({"id":id,"result":{}})).unwrap();
        }
        assert!(state.snapshot().activity.stopping.unwrap().is_empty());
        assert!(
            state
                .admit(
                    &lease,
                    Uuid::new_v4(),
                    "turn/start".into(),
                    json!({"threadId":"a"})
                )
                .is_ok()
        );
    }
}

#[test]
fn rejection_is_visible_allows_only_explicit_stop_retry_and_preserves_receipt() {
    let (mut state, lease) = setup();
    let id = stop(&mut state, &lease);
    let error = json!({"code":-1,"message":"拒绝".repeat(1000)});
    state.record(json!({"id":id,"error":error})).unwrap();
    let pending = &state.snapshot().activity.stopping.unwrap()[0];
    assert_eq!(pending.phase, Phase::Rejected);
    assert!(pending.can_retry());
    assert!(pending.error.as_ref().unwrap().len() < 700);
    let Response::Operation(operation) = state.operation(id) else {
        panic!("receipt")
    };
    assert!(matches!(operation.outcome, Outcome::Finished(ref reply) if reply["error"] == error));
    assert_eq!(
        state
            .admit(
                &lease,
                Uuid::new_v4(),
                "turn/start".into(),
                json!({"threadId":"a"})
            )
            .unwrap_err()
            .code,
        "stop_pending"
    );
    let retry = stop(&mut state, &lease);
    assert_ne!(id, retry);
    state.record(json!({"id":retry,"result":{}})).unwrap();
    completion(&mut state, "a", "one");
    assert!(state.snapshot().activity.stopping.unwrap().is_empty());
}

#[test]
fn takeover_retains_progress_fault_never_claims_settlement_and_identity_is_exact() {
    let (mut state, lease) = setup();
    for params in [
        json!({"threadId":"a","turnId":"other"}),
        json!({"threadId":"b","turnId":"one"}),
        json!({"threadId":"a","turnId":""}),
    ] {
        assert!(
            state
                .admit(&lease, Uuid::new_v4(), "turn/interrupt".into(), params)
                .is_err()
        );
    }
    let id = stop(&mut state, &lease);
    let Response::Lease(next) = state.claim(
        PROTOCOL_VERSION,
        lease.host_id,
        lease.generation,
        Uuid::new_v4(),
    ) else {
        panic!("takeover")
    };
    assert!(state.authorize(&lease).is_err());
    blocked(&mut state, &next);
    state.fail_provider("connection lost".into());
    completion(&mut state, "a", "one");
    assert!(
        state
            .record(json!({"id":id,"error":{"code":-1,"message":"late"}}))
            .is_err()
    );
    assert_eq!(
        state.snapshot().activity.stopping.unwrap()[0].phase,
        Phase::Unknown
    );
    let mut value = serde_json::to_value(state.snapshot()).unwrap();
    value["activity"]
        .as_object_mut()
        .unwrap()
        .remove("stopping");
    let legacy: crate::Snapshot = serde_json::from_value(value).unwrap();
    assert!(legacy.activity.stopping.is_none());
}

#[test]
fn tracker_fault_cannot_be_erased_by_late_ack_or_rejection() {
    for reply in [json!({"result":{}}), json!({"error":{"code":-1}})] {
        let mut tracker = super::Tracker::default();
        let id = Uuid::new_v4();
        let pending = tracker
            .prepare(
                id,
                "turn/interrupt",
                &json!({"threadId":"a","turnId":"one"}),
                &[crate::lifecycle::ActiveTurn {
                    thread_id: "a".into(),
                    turn_id: Some("one".into()),
                }],
            )
            .unwrap()
            .unwrap();
        tracker.commit(pending);
        tracker.fail("lost");
        let mut message: Value = reply;
        message["id"] = json!(id);
        tracker.record(&message).unwrap();
        tracker.record(&json!({"method":"turn/completed","params":{"threadId":"a","turn":{"id":"one","status":"interrupted"}}})).unwrap();
        assert_eq!(tracker.snapshot()[0].phase, Phase::Unknown);
    }
}

#[test]
fn unsupported_terminal_status_never_releases_progress() {
    let (mut state, lease) = setup();
    let id = stop(&mut state, &lease);
    state.record(json!({"id":id,"result":{}})).unwrap();
    for status in [json!("inProgress"), json!("future"), Value::Null] {
        assert!(state.record(json!({"method":"turn/completed","params":{"threadId":"a","turn":{"id":"one","status":status}}})).is_err());
        blocked(&mut state, &lease);
    }
}

#[test]
fn stop_progress_reserves_snapshot_budget_before_admission() {
    let mut tracker = super::Tracker::default();
    let mut refused = false;
    for n in 0..128 {
        let thread = format!("{n}{}", "x".repeat(500));
        let turn = "y".repeat(512);
        let active = [crate::lifecycle::ActiveTurn {
            thread_id: thread.clone(),
            turn_id: Some(turn.clone()),
        }];
        let params = json!({"threadId":thread,"turnId":turn});
        match tracker.prepare(Uuid::new_v4(), "turn/interrupt", &params, &active) {
            Ok(Some(stop)) => tracker.commit(stop),
            Err(error) => {
                assert_eq!(error.code, "capacity");
                refused = true;
                break;
            }
            _ => panic!("Stop plan"),
        }
    }
    assert!(refused);
    tracker.fail(&"错".repeat(10000));
    assert!(serde_json::to_vec(&tracker.snapshot()).unwrap().len() < 64 * 1024);
}
