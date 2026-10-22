//! Codex approval choices supplied by the provider, independent of presentation.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", try_from = "String")]
pub enum Decision {
    Accept,
    AcceptForSession,
    Decline,
    Cancel,
}

impl TryFrom<String> for Decision {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "accept" => Ok(Self::Accept),
            "acceptForSession" => Ok(Self::AcceptForSession),
            "decline" => Ok(Self::Decline),
            "cancel" => Ok(Self::Cancel),
            _ => Err("unsupported Codex approval decision"),
        }
    }
}

impl Decision {
    pub fn label(self) -> &'static str {
        match self {
            Self::Accept => "Allow once",
            Self::AcceptForSession => "Allow for session",
            Self::Decline => "Decline · continue turn",
            Self::Cancel => "Cancel · stop turn",
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Choices {
    pub decisions: Vec<Decision>,
    /// Policy amendments and future variants stay inspectable in the raw request.
    pub unsupported: usize,
}

pub fn choices(request: &Value) -> Result<Option<Choices>, String> {
    if !matches!(
        request.get("method").and_then(Value::as_str),
        Some("item/commandExecution/requestApproval" | "item/fileChange/requestApproval")
    ) {
        return Ok(None);
    }
    let params = request
        .get("params")
        .and_then(Value::as_object)
        .ok_or("Codex approval requires an object params payload")?;
    let advertised = params.get("availableDecisions");
    let Some(advertised) = advertised.filter(|value| !value.is_null()) else {
        // Older Codex requests omit the optional list. Preserve the established
        // one-shot accept/decline contract rather than inferring persistent grants.
        return Ok(Some(Choices {
            decisions: vec![Decision::Accept, Decision::Decline],
            unsupported: 0,
        }));
    };
    let advertised = advertised
        .as_array()
        .ok_or("Codex approval availableDecisions must be an array or null")?;
    if advertised.len() > 16 {
        return Err("Codex approval exceeds the 16-decision display limit".into());
    }
    let mut choices = Choices {
        decisions: Vec::new(),
        unsupported: 0,
    };
    for value in advertised {
        match serde_json::from_value::<Decision>(value.clone()) {
            Ok(decision) => {
                if choices.decisions.contains(&decision) {
                    return Err("Codex approval contains a duplicate decision".into());
                }
                choices.decisions.push(decision);
            }
            Err(_) if value.is_object() || value.is_string() => choices.unsupported += 1,
            Err(_) => return Err("Codex approval decision must be a string or object".into()),
        }
    }
    Ok(Some(choices))
}

/// Validate before writing to provider stdin or removing a pending request.
/// Other provider request methods retain their own response contracts.
pub fn validate_response(request: &Value, response: &Value) -> Result<(), String> {
    let Some(choices) = choices(request)? else {
        return Ok(());
    };
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Answer {
        decision: Decision,
    }
    let answer: Answer = serde_json::from_value(response.clone())
        .map_err(|error| format!("invalid Codex approval response: {error}"))?;
    if !choices.decisions.contains(&answer.decision) {
        return Err("This decision is not offered by the pending Codex approval".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(decisions: Value) -> Value {
        json!({"id":0,"method":"item/commandExecution/requestApproval", "params":{
            "threadId":"synthetic", "availableDecisions":decisions}})
    }

    #[test]
    fn installed_codex_choices_preserve_order_and_refuse_unoffered_decline() {
        let request = request(json!(["accept", {"acceptWithExecpolicyAmendment":{
            "execpolicy_amendment":["printf", "synthetic"]}}, "cancel"]));
        assert_eq!(
            choices(&request).unwrap().unwrap(),
            Choices {
                decisions: vec![Decision::Accept, Decision::Cancel],
                unsupported: 1,
            }
        );
        assert!(validate_response(&request, &json!({"decision":"decline"})).is_err());
        for decision in ["accept", "cancel"] {
            validate_response(&request, &json!({"decision":decision})).unwrap();
        }
        assert_eq!(Decision::Cancel.label(), "Cancel · stop turn");
    }

    #[test]
    fn absent_null_and_empty_lists_have_distinct_contracts() {
        let mut legacy = request(Value::Null);
        for _ in 0..2 {
            let options = choices(&legacy).unwrap().unwrap();
            assert_eq!(options.decisions, vec![Decision::Accept, Decision::Decline]);
            validate_response(&legacy, &json!({"decision":"accept"})).unwrap();
            validate_response(&legacy, &json!({"decision":"decline"})).unwrap();
            assert!(validate_response(&legacy, &json!({"decision":"acceptForSession"})).is_err());
            legacy["params"]
                .as_object_mut()
                .unwrap()
                .remove("availableDecisions");
        }
        let empty = request(json!([]));
        assert!(choices(&empty).unwrap().unwrap().decisions.is_empty());
        assert!(validate_response(&empty, &json!({"decision":"accept"})).is_err());
    }

    #[test]
    fn future_choices_are_inspectable_and_invalid_choices_never_become_defaults() {
        assert_eq!(
            choices(&request(json!([{"accept":null}, "cancel"])))
                .unwrap()
                .unwrap(),
            Choices {
                decisions: vec![Decision::Cancel],
                unsupported: 1
            },
        );
        let future = request(json!(["future", {"futureDecision":{"unknown":[1,null]}}, "cancel"]));
        for params in [Value::Null, json!(false), json!([]), json!("invalid")] {
            let mut invalid = future.clone();
            invalid["params"] = params;
            assert!(choices(&invalid).is_err());
        }
        assert_eq!(
            choices(&future).unwrap().unwrap(),
            Choices {
                decisions: vec![Decision::Cancel],
                unsupported: 2,
            }
        );
        for invalid in [
            json!(false),
            json!("accept"),
            json!([null]),
            json!([4]),
            json!(["accept", "accept"]),
            json!(vec!["future"; 17]),
        ] {
            assert!(choices(&request(invalid)).is_err());
        }
        let offered = request(json!(["cancel", "decline", "acceptForSession", "accept"]));
        assert_eq!(
            choices(&offered).unwrap().unwrap().decisions,
            vec![
                Decision::Cancel,
                Decision::Decline,
                Decision::AcceptForSession,
                Decision::Accept
            ]
        );
        for invalid in [
            json!({}),
            json!({"decision":true}),
            json!({"decision":"future"}),
            json!({"decision":"accept","extra":true}),
            json!({"decision":{"acceptWithExecpolicyAmendment":{}}}),
            json!({"decision":{"accept":null}}),
        ] {
            assert!(validate_response(&offered, &invalid).is_err());
        }
        let mut file = offered;
        file["method"] = json!("item/fileChange/requestApproval");
        validate_response(&file, &json!({"decision":"acceptForSession"})).unwrap();
        assert!(
            choices(&json!({"method":"session/request_permission"}))
                .unwrap()
                .is_none()
        );
    }
}
