//! Presentation text for the same atomic snapshot used by every client.
use crate::{Error, Thread};
use fleetpane_runtime::usage::{Observation, Source, View};

impl Thread {
    pub fn apply_context_usage(&mut self, view: View, sequence: u64) -> Result<(), Error> {
        view.validate(&self.id, sequence)
            .map_err(Error::Projection)?;
        if self
            .context_usage
            .as_ref()
            .is_none_or(|old| view.sequence >= old.sequence)
        {
            self.context_usage = Some(view);
        }
        Ok(())
    }
}

pub fn summary(observation: Option<&Observation>) -> String {
    let Some(observation) = observation else {
        return "Context unavailable".into();
    };
    let capacity = observation
        .capacity_tokens
        .map(|n| n.to_string())
        .unwrap_or_else(|| "unavailable".into());
    let percent = observation
        .percent()
        .map(|n| format!(" · {n:.1}%"))
        .unwrap_or_default();
    format!(
        "Context {} / {} tokens{}",
        observation.used_tokens, capacity, percent
    )
}

pub fn source(observation: &Observation) -> &'static str {
    match observation.source {
        Source::CodexLast => "Codex · last token usage",
        Source::AcpUsageUpdate => "ACP · usage update",
    }
}

#[cfg(test)]
mod tests;
