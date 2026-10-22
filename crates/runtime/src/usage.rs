//! Atomic provider context observations. Session totals are retained only as raw telemetry.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    time::{SystemTime, UNIX_EPOCH},
};

pub const READ_METHOD: &str = "fleetpane/contextUsage/read";
pub const UPDATED_METHOD: &str = "thread/contextUsage/updated";
pub const MAX_OBSERVATION_BYTES: usize = 64 * 1024;
const MAX_THREADS: usize = 64;
const MAX_CACHE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    CodexLast,
    AcpUsageUpdate,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Observation {
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub source: Source,
    /// Host receipt time, not a provider measurement timestamp.
    pub observed_at_unix_ms: u64,
    pub used_tokens: u64,
    pub capacity_tokens: Option<u64>,
    /// Complete original provider notification, including totals, cost and extensions.
    pub raw: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct View {
    pub sequence: u64,
    pub observation: Option<Observation>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Read {
    pub thread_id: String,
}

fn now() -> Result<u64, String> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis(),
    )
    .map_err(|e| e.to_string())
}
fn identity(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 4096 {
        return Err("context usage identity must be 1..4096 bytes".into());
    }
    Ok(())
}
fn codex_count(value: &Value, key: &str) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| format!("invalid context usage {key}: expected nonnegative int64"))
}
fn breakdown(value: &Value) -> Result<u64, String> {
    for field in [
        "cachedInputTokens",
        "inputTokens",
        "outputTokens",
        "reasoningOutputTokens",
    ] {
        codex_count(value, field)?;
    }
    if value.get("cacheWriteInputTokens").is_some() {
        codex_count(value, "cacheWriteInputTokens")?;
    }
    codex_count(value, "totalTokens")
}

impl Observation {
    fn codex(raw: &Value, observed_at_unix_ms: u64) -> Result<Self, String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Params {
            thread_id: String,
            turn_id: String,
            token_usage: Value,
        }
        let p: Params = serde_json::from_value(
            raw.get("params")
                .cloned()
                .ok_or("context usage lacks params")?,
        )
        .map_err(|e| format!("invalid Codex context usage: {e}"))?;
        let used_tokens = breakdown(&p.token_usage["last"])?;
        breakdown(&p.token_usage["total"])?;
        let capacity_tokens = match p.token_usage.get("modelContextWindow") {
            None | Some(Value::Null) => None,
            Some(_) => Some(codex_count(&p.token_usage, "modelContextWindow")?),
        };
        let result = Self {
            thread_id: p.thread_id,
            turn_id: Some(p.turn_id),
            source: Source::CodexLast,
            observed_at_unix_ms,
            used_tokens,
            capacity_tokens,
            raw: raw.clone(),
        };
        result.bounds()?;
        Ok(result)
    }

    pub fn from_acp(raw: &Value, turn_id: Option<String>) -> Result<Self, String> {
        Self::acp(raw, turn_id, now()?)
    }
    fn acp(raw: &Value, turn_id: Option<String>, observed_at_unix_ms: u64) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct Cost {
            #[serde(rename = "amount")]
            _amount: f64,
            #[serde(rename = "currency")]
            _currency: String,
        }
        #[derive(Deserialize)]
        struct Update {
            #[serde(rename = "sessionUpdate")]
            kind: String,
            used: u64,
            size: u64,
            #[serde(rename = "cost")]
            _cost: Option<Cost>,
            #[serde(rename = "_meta")]
            _meta: Option<serde_json::Map<String, Value>>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Params {
            session_id: String,
            update: Update,
        }
        if raw.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
            || raw.get("method").and_then(Value::as_str) != Some("session/update")
        {
            return Err("context usage raw telemetry is not an ACP session/update".into());
        }
        let p: Params = serde_json::from_value(
            raw.get("params")
                .cloned()
                .ok_or("ACP context usage lacks params")?,
        )
        .map_err(|e| format!("invalid ACP context usage: {e}"))?;
        if p.update.kind != "usage_update" {
            return Err("ACP context usage has wrong discriminator".into());
        }
        let result = Self {
            thread_id: p.session_id,
            turn_id,
            source: Source::AcpUsageUpdate,
            observed_at_unix_ms,
            used_tokens: p.update.used,
            capacity_tokens: Some(p.update.size),
            raw: raw.clone(),
        };
        result.bounds()?;
        Ok(result)
    }

    pub fn from_event(message: &Value) -> Result<Option<Self>, String> {
        match message.get("method").and_then(Value::as_str) {
            Some("thread/tokenUsage/updated") => Self::codex(message, now()?).map(Some),
            Some(UPDATED_METHOD) => {
                let observation: Self = serde_json::from_value(
                    message
                        .pointer("/params/observation")
                        .cloned()
                        .ok_or("context usage notification lacks observation")?,
                )
                .map_err(|e| e.to_string())?;
                observation.validate()?;
                if message.pointer("/params/threadId").and_then(Value::as_str)
                    != Some(&observation.thread_id)
                {
                    return Err(
                        "context usage notification has conflicting thread identities".into(),
                    );
                }
                Ok(Some(observation))
            }
            _ => Ok(None),
        }
    }
    fn bounds(&self) -> Result<(), String> {
        identity(&self.thread_id)?;
        if let Some(id) = &self.turn_id {
            identity(id)?;
        }
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > MAX_OBSERVATION_BYTES {
            return Err("context usage exceeds 64 KiB observation limit".into());
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        self.bounds()?;
        let decoded = match self.source {
            Source::CodexLast => {
                if self.raw.get("method").and_then(Value::as_str)
                    != Some("thread/tokenUsage/updated")
                {
                    return Err("context usage raw telemetry has wrong Codex method".into());
                }
                Self::codex(&self.raw, self.observed_at_unix_ms)?
            }
            Source::AcpUsageUpdate => {
                Self::acp(&self.raw, self.turn_id.clone(), self.observed_at_unix_ms)?
            }
        };
        if decoded != *self {
            return Err("context usage differs from its raw provider snapshot".into());
        }
        Ok(())
    }
    /// A zero or unknown capacity has no defined percentage. Never cap overcapacity.
    pub fn percent(&self) -> Option<f64> {
        self.capacity_tokens
            .filter(|c| *c != 0)
            .map(|c| self.used_tokens as f64 / c as f64 * 100.)
    }
    pub fn notification(&self) -> Value {
        json!({"method":UPDATED_METHOD,"params":{"threadId":self.thread_id,"observation":self}})
    }
    /// Copy identity and receipt metadata alongside the untouched parsed provider payload.
    pub fn copy_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("observation JSON is serializable")
    }
}

impl View {
    pub fn validate(&self, thread: &str, sequence: u64) -> Result<(), String> {
        if self.sequence != sequence {
            return Err("context usage read boundary mismatch".into());
        }
        if let Some(observation) = &self.observation {
            observation.validate()?;
            if observation.thread_id != thread {
                return Err("context usage read belongs to another thread".into());
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct Cache {
    observations: VecDeque<(Observation, usize)>,
    bytes: usize,
}
impl Cache {
    pub fn record(&mut self, observation: Observation) {
        if let Some(i) = self
            .observations
            .iter()
            .position(|(old, _)| old.thread_id == observation.thread_id)
        {
            self.bytes -= self.observations.remove(i).expect("existing observation").1;
        }
        let bytes = serde_json::to_vec(&observation)
            .expect("serializable observation")
            .len();
        self.bytes += bytes;
        self.observations.push_back((observation, bytes));
        while self.observations.len() > MAX_THREADS || self.bytes > MAX_CACHE_BYTES {
            self.bytes -= self
                .observations
                .pop_front()
                .expect("nonempty usage cache")
                .1;
        }
    }
    #[cfg(any(unix, test))]
    pub fn read(&self, thread: &str, sequence: u64) -> View {
        View {
            sequence,
            observation: self
                .observations
                .iter()
                .find(|(o, _)| o.thread_id == thread)
                .map(|(o, _)| o.clone()),
        }
    }
}

#[cfg(test)]
mod tests;
