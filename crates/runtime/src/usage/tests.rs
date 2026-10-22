use super::*;
use crate::{PROTOCOL_VERSION, Response, State};
use uuid::Uuid;

pub(super) fn codex(used: Value, capacity: Value) -> Value {
    let breakdown = |total| json!({"totalTokens":total,"inputTokens":1,"cachedInputTokens":0,"outputTokens":0,"reasoningOutputTokens":0});
    json!({"method":"thread/tokenUsage/updated","params":{"threadId":"one","turnId":"turn","tokenUsage":{
        "last":breakdown(used),"total":breakdown(json!(99999)),"modelContextWindow":capacity,"future":{"exact":"中文\n","falsy":false}}}})
}
fn parse(raw: &Value) -> Observation {
    Observation::from_event(raw).unwrap().unwrap()
}
fn acp(used: Value, size: Value) -> Value {
    json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"one","update":{
        "sessionUpdate":"usage_update","used":used,"size":size,"cost":{"amount":0.125,"currency":"USD"},"_meta":{"opaque":[null,false,"中文"]}}}})
}

#[test]
fn unknown_zero_full_and_overcapacity_keep_atomic_raw_snapshots() {
    for capacity in [Value::Null, json!(0), json!(100)] {
        let raw = codex(json!(0), capacity.clone());
        let o = parse(&raw);
        assert_eq!(o.used_tokens, 0);
        assert_eq!(o.capacity_tokens, capacity.as_u64());
        assert_eq!(
            o.percent(),
            capacity.as_u64().filter(|c| *c > 0).map(|_| 0.)
        );
        assert_eq!(
            serde_json::from_str::<Value>(&o.copy_json()).unwrap()["raw"],
            raw
        );
        let copied: Value = serde_json::from_str(&o.copy_json()).unwrap();
        assert_eq!(copied["threadId"], o.thread_id);
        assert_eq!(copied["observedAtUnixMs"], o.observed_at_unix_ms);
        assert_eq!(parse(&o.notification()), o);
    }
    let mut missing = codex(json!(125), json!(100));
    assert_eq!(parse(&missing).percent(), Some(125.));
    missing["params"]["tokenUsage"]
        .as_object_mut()
        .unwrap()
        .remove("modelContextWindow");
    assert_eq!(parse(&missing).capacity_tokens, None);
    let max = Observation::from_acp(&acp(json!(u64::MAX), json!(u64::MAX)), None).unwrap();
    assert_eq!(max.used_tokens, u64::MAX);
    assert_eq!(max.percent(), Some(100.));
    assert_eq!(max.raw, acp(json!(u64::MAX), json!(u64::MAX)));
}

#[test]
fn missing_null_wrong_type_negative_overflow_and_tampering_are_errors() {
    let valid = codex(json!(15), json!(100));
    for pointer in [
        "/params/threadId",
        "/params/turnId",
        "/params/tokenUsage/last",
        "/params/tokenUsage/total",
        "/params/tokenUsage/last/totalTokens",
    ] {
        let mut raw = valid.clone();
        *raw.pointer_mut(pointer).unwrap() = Value::Null;
        assert!(Observation::from_event(&raw).is_err(), "{pointer}");
    }
    for n in [
        json!(-1),
        json!(1.5),
        json!("3"),
        json!(u64::MAX),
        Value::Null,
    ] {
        assert!(Observation::from_event(&codex(n.clone(), json!(100))).is_err());
        if !n.is_null() {
            assert!(Observation::from_event(&codex(json!(1), n.clone())).is_err());
        }
    }
    for n in [
        json!(-1),
        json!(1.5),
        Value::Null,
        json!("3"),
        serde_json::from_str("18446744073709551616").unwrap(),
    ] {
        assert!(Observation::from_acp(&acp(n.clone(), json!(100)), None).is_err());
        assert!(Observation::from_acp(&acp(json!(1), n), None).is_err());
    }
    let mut o = parse(&valid);
    o.capacity_tokens = None;
    assert!(o.validate().is_err());
    let mut raw = acp(json!(1), json!(100));
    raw["params"]["update"]["_meta"] = json!(false);
    assert!(Observation::from_acp(&raw, None).is_err());
    raw["params"]["update"]["_meta"] = Value::Null;
    raw["params"]["update"]["cost"] = json!({"amount":"bad","currency":"USD"});
    assert!(Observation::from_acp(&raw, None).is_err());
    let mut message = parse(&valid).notification();
    message["params"]["threadId"] = json!("other");
    assert!(Observation::from_event(&message).is_err());
}

#[test]
fn host_retains_latest_between_turns_event_expiry_and_takeover_without_inferred_success() {
    let mut state = State::new(1, json!({}));
    assert!(crate::is_query_method(READ_METHOD));
    let owner = match state.claim(PROTOCOL_VERSION, state.snapshot.host_id, 0, Uuid::new_v4()) {
        Response::Lease(lease) => lease,
        other => panic!("{other:?}"),
    };
    state.record(codex(json!(20), json!(100))).unwrap();
    let expected = state
        .usage
        .read("one", state.snapshot.sequence)
        .observation
        .unwrap();
    state.record(json!({"method":"turn/started","params":{"threadId":"one","turn":{"id":"next","status":"inProgress","items":[]}}})).unwrap();
    for _ in 0..140 {
        state
            .record(json!({"method":"fixture/heartbeat","params":{}}))
            .unwrap();
    }
    assert!(state.events.len() < 140);
    assert_eq!(
        state.usage.read("one", state.snapshot.sequence).observation,
        Some(expected)
    );
    assert_eq!(state.snapshot().activity.turns.len(), 1);
    let _next = state.claim(
        PROTOCOL_VERSION,
        state.snapshot.host_id,
        owner.generation,
        Uuid::new_v4(),
    );
    assert!(state.usage.read("other", 0).observation.is_none());
    state.record(codex(json!(0), Value::Null)).unwrap();
    let latest = state
        .usage
        .read("one", state.snapshot.sequence)
        .observation
        .unwrap();
    assert_eq!(latest.used_tokens, 0);
    assert_eq!(latest.capacity_tokens, None);
    assert_eq!(state.snapshot().activity.turns.len(), 1);
    let before = state.snapshot.sequence;
    assert!(state.record(codex(Value::Null, json!(10))).is_err());
    assert_eq!(state.snapshot.sequence, before);
    assert_eq!(state.usage.read("one", before).observation, Some(latest));
}

#[test]
fn cache_eviction_is_bounded_and_never_mixes_threads_or_fields() {
    let mut cache = Cache::default();
    for i in 0..70 {
        let mut raw = codex(json!(i), json!(100));
        raw["params"]["threadId"] = json!(i.to_string());
        cache.record(parse(&raw));
    }
    assert_eq!(cache.observations.len(), 64);
    let unknown = cache.read("0", 42);
    assert!(unknown.observation.is_none());
    assert_eq!(unknown.sequence, 42);
    unknown.validate("0", 42).unwrap();
    assert!(unknown.validate("0", 41).is_err());
    assert_eq!(cache.read("69", 0).observation.unwrap().used_tokens, 69);
    let mut big = codex(json!(1), json!(2));
    big["params"]["opaque"] = json!("x".repeat(MAX_OBSERVATION_BYTES));
    assert!(Observation::from_event(&big).is_err());
}

#[test]
fn byte_budget_evicts_whole_snapshots_and_missing_required_fields_never_default() {
    let mut cache = Cache::default();
    for i in 0..30 {
        let mut raw = codex(json!(i), json!(100));
        raw["params"]["threadId"] = json!(i.to_string());
        raw["params"]["opaque"] = json!("x".repeat(50000));
        cache.record(parse(&raw));
    }
    assert!(cache.bytes <= MAX_CACHE_BYTES);
    assert!(cache.observations.len() < 30);
    assert!(cache.read("0", 10).observation.is_none());
    assert_eq!(cache.read("29", 10).observation.unwrap().used_tokens, 29);
    for key in [
        "cachedInputTokens",
        "inputTokens",
        "outputTokens",
        "reasoningOutputTokens",
        "totalTokens",
    ] {
        let mut raw = codex(json!(1), json!(100));
        raw["params"]["tokenUsage"]["last"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(Observation::from_event(&raw).is_err(), "{key}");
    }
    for key in ["used", "size"] {
        let mut raw = acp(json!(1), json!(100));
        raw["params"]["update"].as_object_mut().unwrap().remove(key);
        assert!(Observation::from_acp(&raw, None).is_err(), "{key}");
    }
    let mut raw = acp(json!(1), json!(100));
    raw["params"]["update"]["cost"] = Value::Null;
    raw["params"]["update"]
        .as_object_mut()
        .unwrap()
        .remove("_meta");
    assert!(Observation::from_acp(&raw, None).is_ok());
}
