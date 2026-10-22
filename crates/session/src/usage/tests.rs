use super::*;
use fleetpane_runtime::{Event, usage::Observation};
use serde_json::{Value, json};

fn thread() -> Thread {
    serde_json::from_value(
        json!({"id":"one","cwd":"/synthetic","preview":"","status":{"type":"idle"},"turns":[]}),
    )
    .unwrap()
}
fn event(sequence: u64, used: u64, capacity: Option<u64>) -> Event {
    let breakdown = |total| json!({"totalTokens":total,"inputTokens":0,"cachedInputTokens":0,"outputTokens":0,"reasoningOutputTokens":0});
    Event {
        sequence,
        message: json!({"method":"thread/tokenUsage/updated","params":{"threadId":"one","turnId":"last-turn","tokenUsage":{
        "last":breakdown(used),"total":breakdown(88888),"modelContextWindow":capacity}}}),
    }
}
#[test]
fn reload_sequence_boundary_atomic_replacement_foreign_thread_and_interturn() {
    let mut thread = thread();
    assert_eq!(summary(None), "Context unavailable");
    let first = event(5, 125, Some(100));
    assert!(thread.apply(&first).unwrap());
    let original = thread
        .context_usage
        .as_ref()
        .unwrap()
        .observation
        .as_ref()
        .unwrap();
    assert_eq!(summary(Some(original)), "Context 125 / 100 tokens · 125.0%");
    let mut foreign = event(6, 1, Some(1));
    foreign.message["params"]["threadId"] = json!("other");
    assert!(!thread.apply(&foreign).unwrap());
    let read = View {
        sequence: 10,
        observation: Observation::from_event(&event(9, 0, None).message).unwrap(),
    };
    thread.apply_context_usage(read, 10).unwrap();
    thread.apply(&event(8, 50, Some(100))).unwrap();
    let current = thread
        .context_usage
        .as_ref()
        .unwrap()
        .observation
        .as_ref()
        .unwrap();
    assert_eq!(summary(Some(current)), "Context 0 / unavailable tokens");
    assert_eq!(current.capacity_tokens, None);
    let before = serde_json::to_value(&thread.context_usage).unwrap();
    let started = Event {
        sequence: 11,
        message: json!({"method":"turn/started","params":{"threadId":"one","turn":{"id":"new-turn","status":"inProgress","items":[]}}}),
    };
    assert!(thread.apply(&started).unwrap());
    assert_eq!(serde_json::to_value(&thread.context_usage).unwrap(), before);
    assert_eq!(thread.active_turn().unwrap().id, "new-turn");
    thread.apply(&event(12, 1, Some(0))).unwrap();
    assert_eq!(thread.active_turn().unwrap().status, "inProgress");
    assert_eq!(
        summary(thread.context_usage.as_ref().unwrap().observation.as_ref()),
        "Context 1 / 0 tokens"
    );
}
#[test]
fn read_rejects_wrong_boundary_identity_and_invalid_payload_without_mutation() {
    let mut thread = thread();
    thread.apply(&event(1, 20, Some(100))).unwrap();
    let saved = serde_json::to_value(&thread).unwrap();
    let mut bad = event(2, 10, Some(30));
    bad.message["params"]["tokenUsage"]["last"]["totalTokens"] = Value::Null;
    assert!(thread.apply(&bad).is_err());
    assert_eq!(serde_json::to_value(&thread).unwrap(), saved);
    let observation = Observation::from_event(&event(2, 10, Some(30)).message).unwrap();
    assert!(
        thread
            .apply_context_usage(
                View {
                    sequence: 2,
                    observation: observation.clone()
                },
                3
            )
            .is_err()
    );
    let mut wrong = observation.unwrap();
    wrong.thread_id = "other".into();
    assert!(
        thread
            .apply_context_usage(
                View {
                    sequence: 2,
                    observation: Some(wrong)
                },
                2
            )
            .is_err()
    );
    assert_eq!(serde_json::to_value(&thread).unwrap(), saved);
    thread
        .apply_context_usage(
            View {
                sequence: 4,
                observation: None,
            },
            4,
        )
        .unwrap();
    assert!(thread.context_usage.as_ref().unwrap().observation.is_none());
    thread.apply(&event(3, 10, Some(30))).unwrap();
    assert!(thread.context_usage.as_ref().unwrap().observation.is_none());
}

#[test]
fn host_receipt_timestamp_never_needs_arithmetic_and_copy_keeps_full_u64() {
    let raw = json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"one","update":{
        "sessionUpdate":"usage_update","used":u64::MAX,"size":u64::MAX,"_meta":{"providerTime":"opaque"}}}});
    let mut observation = Observation::from_acp(&raw, None).unwrap();
    observation.observed_at_unix_ms = u64::MAX;
    observation.validate().unwrap();
    assert!(summary(Some(&observation)).contains("18446744073709551615 / 18446744073709551615"));
    let copy: Value = serde_json::from_str(&observation.copy_json()).unwrap();
    assert_eq!(copy["raw"], raw);
    assert_eq!(copy["threadId"], "one");
    assert_eq!(copy["observedAtUnixMs"], u64::MAX);
}
