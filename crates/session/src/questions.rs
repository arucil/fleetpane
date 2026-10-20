//! Codex's experimental requestUserInput contract, independent of the UI.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const METHOD: &str = "item/tool/requestUserInput";
pub const OTHER_LABEL: &str = "None of the above";

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Request {
    pub id: Value,
    pub params: Params,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Params {
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub questions: Vec<Question>,
    pub is_blocking: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub id: String,
    pub header: String,
    pub question: String,
    #[serde(default)]
    pub is_other: bool,
    #[serde(default)]
    pub is_secret: bool,
    pub options: Option<Vec<OptionItem>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct OptionItem {
    pub label: String,
    pub description: String,
}

// Do not derive Debug: responses can contain secrets.
#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub answers: BTreeMap<String, Answer>,
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Answer {
    pub answers: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid user input request or answer: {0}")]
pub struct Error(String);

impl Request {
    /// Unknown methods remain inspectable; malformed known requests are errors.
    pub fn decode(value: &Value) -> Result<Option<Self>, Error> {
        if value.get("method").and_then(Value::as_str) != Some(METHOD) {
            return Ok(None);
        }
        let request: Self =
            serde_json::from_value(value.clone()).map_err(|error| Error(error.to_string()))?;
        if !(request.id.is_string() || request.id.is_i64())
            || request.params.thread_id.is_empty()
            || request.params.turn_id.is_empty()
            || request.params.item_id.is_empty()
            || request.params.questions.is_empty()
        {
            return Err(Error("missing request identity or questions".into()));
        }
        let mut ids = BTreeSet::new();
        for question in &request.params.questions {
            if question.id.is_empty() || !ids.insert(&question.id) {
                return Err(Error("question IDs must be nonempty and unique".into()));
            }
        }
        Ok(Some(request))
    }

    pub fn validate_response(&self, response: &Response) -> Result<(), Error> {
        if serde_json::to_vec(response)
            .map_err(|error| Error(error.to_string()))?
            .len()
            > crate::MAX_PROMPT_BYTES
        {
            return Err(Error("answers must fit within 64 KiB".into()));
        }
        if response.answers.len() != self.params.questions.len() {
            return Err(Error("answer every question before submitting".into()));
        }
        for question in &self.params.questions {
            let answer = response
                .answers
                .get(&question.id)
                .ok_or_else(|| Error("answer belongs to a different question".into()))?;
            let options = question.options.as_deref().unwrap_or_default();
            let valid = match answer.answers.as_slice() {
                [note] if options.is_empty() => valid_note(note),
                [label] => question.is_label(label),
                [label, note] => question.is_label(label) && valid_note(note),
                _ => false,
            };
            if !valid {
                return Err(Error(
                    "choose one option or enter an answer; notes must contain text".into(),
                ));
            }
        }
        Ok(())
    }
}

fn valid_note(note: &str) -> bool {
    note.strip_prefix("user_note: ")
        .is_some_and(|text| !text.trim().is_empty())
}

impl Question {
    pub fn labels(&self) -> Vec<String> {
        let mut labels: Vec<_> = self
            .options
            .iter()
            .flatten()
            .map(|option| option.label.clone())
            .collect();
        if self.is_other && !labels.is_empty() {
            labels.push(OTHER_LABEL.into());
        }
        labels
    }

    fn is_label(&self, value: &str) -> bool {
        self.labels().iter().any(|label| label == value)
    }

    /// Match Codex TUI semantics: a single selected label, then optional user_note.
    /// Free text is also sent as user_note, preserving embedded newlines/Unicode.
    pub fn answer(&self, selected: Option<usize>, text: &str) -> Option<Answer> {
        let labels = self.labels();
        let mut answers = if labels.is_empty() {
            Vec::new()
        } else {
            vec![labels.get(selected?)?.clone()]
        };
        if !text.trim().is_empty() {
            answers.push(format!("user_note: {}", text.trim()));
        }
        (!answers.is_empty()).then_some(Answer { answers })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request() -> Request {
        Request::decode(&json!({"id":7,"method":METHOD,"params":{
            "threadId":"a","turnId":"b","itemId":"c","isBlocking":true,
            "questions":[{"id":"choice","header":"Language","question":"Choose?","isOther":true,
                "options":[{"label":"Rust (Recommended)","description":"Native"}]},
                {"id":"text","header":"Details","question":"Why?"}]}}))
        .unwrap()
        .unwrap()
    }

    #[test]
    fn no_default_answer_and_unicode_notes_match_codex_tui() {
        let request = request();
        let choice = &request.params.questions[0];
        assert!(choice.answer(None, "").is_none());
        let response = Response {
            answers: BTreeMap::from([
                ("choice".into(), choice.answer(Some(1), "自定义").unwrap()),
                (
                    "text".into(),
                    request.params.questions[1]
                        .answer(None, "中文\n第二行")
                        .unwrap(),
                ),
            ]),
        };
        request.validate_response(&response).unwrap();
        assert_eq!(
            serde_json::to_value(&response).unwrap(),
            json!({"answers":{
            "choice":{"answers":[OTHER_LABEL,"user_note: 自定义"]},
            "text":{"answers":["user_note: 中文\n第二行"]}}})
        );
    }

    #[test]
    fn invalid_identity_missing_answers_and_multi_selection_are_rejected() {
        let request = request();
        for value in [
            json!({"answers":{}}),
            json!({"answers":{"choice":{"answers":["Rust (Recommended)",OTHER_LABEL]},"text":{"answers":["user_note: ok"]}}}),
            json!({"answers":{"wrong":{"answers":["Rust (Recommended)"]},"text":{"answers":["user_note: ok"]}}}),
        ] {
            assert!(
                request
                    .validate_response(&serde_json::from_value(value).unwrap())
                    .is_err()
            );
        }
        assert!(Request::decode(&json!({"method":METHOD})).is_err());
        assert!(
            Request::decode(&json!({"method":"future/request"}))
                .unwrap()
                .is_none()
        );
        let mut duplicate = json!({"id":1,"method":METHOD,"params":{"threadId":"a","turnId":"b","itemId":"c","isBlocking":false,"questions":[{"id":"x","header":"","question":""},{"id":"x","header":"","question":""}]}});
        assert!(Request::decode(&duplicate).is_err());
        duplicate["id"] = Value::Null;
        assert!(Request::decode(&duplicate).is_err());
    }

    #[test]
    fn oversized_answer_is_rejected_before_transport() {
        let request = request();
        let mut response = Response {
            answers: BTreeMap::from([
                (
                    "choice".into(),
                    request.params.questions[0].answer(Some(0), "").unwrap(),
                ),
                (
                    "text".into(),
                    request.params.questions[1]
                        .answer(None, &"x".repeat(crate::MAX_PROMPT_BYTES))
                        .unwrap(),
                ),
            ]),
        };
        assert!(
            request
                .validate_response(&response)
                .unwrap_err()
                .to_string()
                .contains("64 KiB")
        );
        response.answers.get_mut("text").unwrap().answers = vec!["user_note: short".into()];
        request.validate_response(&response).unwrap();
    }
}
