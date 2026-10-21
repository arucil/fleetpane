use super::{Image, Prompt, Store, validate};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};

// Held until dispatch (or refusal), so queued clients cannot accumulate unbounded
// base64 payloads while another provider write is pending.
static PROMPTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
pub(crate) struct Prepared {
    pub prompt: Prompt,
    _permit: tokio::sync::SemaphorePermit<'static>,
}

#[derive(Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
enum Input {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "fleetpaneImage")]
    Image { image: Image },
}

/// Runs on a connection task, before the serial Host state loop. Rejected input
/// has not been admitted or written to the provider. Receipt lookup still wins
/// over preflight errors when the UUID was already recorded.
pub(crate) async fn prepare(
    store: &Store,
    method: &str,
    params: &Value,
    acp: bool,
) -> Result<Option<Prepared>, String> {
    if method != "turn/start" {
        return Ok(None);
    }
    let Some(input) = params.get("input").and_then(Value::as_array) else {
        return Ok(None);
    };
    if !input.iter().any(|v| v["type"] == "fleetpaneImage") {
        return Ok(None);
    }
    if input.len() > super::MAX_IMAGES + 1 {
        return Err("Too many image prompt input blocks".into());
    }
    let input: Vec<Input> =
        serde_json::from_value(json!(input)).map_err(|e| format!("Image prompt: {e}"))?;
    let images: Vec<_> = input
        .iter()
        .filter_map(|i| match i {
            Input::Image { image } => Some(image.clone()),
            _ => None,
        })
        .collect();
    validate(&images)?;
    let text_bytes: usize = input
        .iter()
        .filter_map(|i| match i {
            Input::Text { text } => Some(text.len()),
            _ => None,
        })
        .sum();
    if text_bytes > 64 * 1024 {
        return Err("Image prompt text exceeds 64 KiB".into());
    }
    let permit = PROMPTS
        .try_acquire()
        .map_err(|_| "Image sends are busy; wait for the pending sends to finish")?;
    let mut prompt = Prompt {
        wire: Vec::new(),
        display: Vec::new(),
    };
    for part in input {
        match part {
            Input::Text { text } => {
                if text.trim().is_empty() {
                    return Err("Empty text blocks must be omitted from image-only prompts".into());
                }
                let part = json!({"type":"text", "text":text});
                prompt.wire.push(part.clone());
                prompt.display.push(part);
            }
            Input::Image { image } => {
                let loaded = store.load(image.clone()).await?;
                let display = json!({"type":"localImage", "path":loaded.path});
                let wire = if acp {
                    json!({"type":"image", "data":STANDARD.encode(&loaded.bytes), "mimeType":image.mime_type})
                } else {
                    display.clone()
                };
                prompt.wire.push(wire);
                prompt.display.push(display);
            }
        }
    }
    Ok(Some(Prepared {
        prompt,
        _permit: permit,
    }))
}
