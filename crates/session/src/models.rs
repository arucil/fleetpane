//! Provider-owned model capabilities and explicit options for the next turn.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const PAGE_SIZE: usize = 64;
const MAX_MODELS: usize = 256;
const MAX_BYTES: usize = 1024 * 1024;
const MAX_CURSOR_BYTES: usize = 4096;

pub fn params(cursor: &Option<String>) -> Result<serde_json::Value, Error> {
    validate_cursor(cursor)?;
    Ok(serde_json::json!({"limit":PAGE_SIZE,"cursor":cursor,"includeHidden":false}))
}

fn validate_cursor(cursor: &Option<String>) -> Result<(), Error> {
    if cursor
        .as_ref()
        .is_some_and(|value| value.is_empty() || value.len() > MAX_CURSOR_BYTES)
    {
        return Err(Error(
            "Model cursor must contain text and fit within 4 KiB.".into(),
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    pub model: String,
    pub display_name: String,
    pub description: String,
    pub hidden: bool,
    pub supported_reasoning_efforts: Vec<Effort>,
    pub default_reasoning_effort: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Effort {
    pub reasoning_effort: String,
    pub description: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub data: Vec<Model>,
    pub next_cursor: Option<String>,
}

#[derive(Default)]
pub struct Catalog {
    pub models: Vec<Model>,
    pub next_cursor: Option<String>,
    pub complete: bool,
    seen: BTreeSet<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("model catalog: {0}")]
pub struct Error(String);

impl Catalog {
    /// Validate the whole page before changing the retained catalog.
    pub fn apply(&mut self, cursor: &Option<String>, page: Page) -> Result<(), Error> {
        validate_cursor(cursor)?;
        validate_cursor(&page.next_cursor)?;
        if cursor.is_some() && (self.complete || cursor != &self.next_cursor) {
            return Err(Error(
                "Page does not continue this catalog; reload models.".into(),
            ));
        }
        let mut seen = if cursor.is_some() {
            self.seen.clone()
        } else {
            BTreeSet::new()
        };
        if let Some(cursor) = cursor {
            seen.insert(cursor.clone());
        }
        if page.data.len() > PAGE_SIZE
            || (page.data.is_empty() && page.next_cursor.is_some())
            || page
                .next_cursor
                .as_ref()
                .is_some_and(|next| next.is_empty() || seen.contains(next))
        {
            return Err(Error(
                "Invalid page size or repeated pagination cursor.".into(),
            ));
        }
        let mut models = if cursor.is_some() {
            self.models.clone()
        } else {
            vec![]
        };
        models.extend(page.data);
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for model in &models {
            let mut efforts = BTreeSet::new();
            if model.id.trim().is_empty()
                || model.model.trim().is_empty()
                || model.display_name.trim().is_empty()
                || !ids.insert(&model.id)
                || !names.insert(&model.model)
                || model.supported_reasoning_efforts.iter().any(|effort| {
                    effort.reasoning_effort.trim().is_empty()
                        || !efforts.insert(&effort.reasoning_effort)
                })
                || (!model.supported_reasoning_efforts.is_empty()
                    && !efforts.contains(&model.default_reasoning_effort))
            {
                return Err(Error(
                    "Empty or duplicate model/effort identity, or unsupported default effort."
                        .into(),
                ));
            }
        }
        if models.len() > MAX_MODELS
            || serde_json::to_vec(&models)
                .map_err(|error| Error(error.to_string()))?
                .len()
                > MAX_BYTES
        {
            return Err(Error("Catalog exceeds 256 models / 1 MiB.".into()));
        }
        self.models = models;
        self.complete = page.next_cursor.is_none();
        self.next_cursor = page.next_cursor;
        self.seen = seen;
        Ok(())
    }

    pub fn find(&self, name: &str) -> Option<&Model> {
        self.models
            .iter()
            .find(|model| model.model == name && !model.hidden)
    }
}

/// Omitted options retain the provider's current thread configuration. Explicit
/// values must originate in a complete catalog; future effort strings are valid.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TurnOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    effort: Option<String>,
}

impl TurnOptions {
    pub fn new(
        catalog: &Catalog,
        model: Option<String>,
        effort: Option<String>,
    ) -> Result<Self, Error> {
        let options = Self { model, effort };
        options.validate(catalog)?;
        Ok(options)
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }
    pub fn effort(&self) -> Option<&str> {
        self.effort.as_deref()
    }

    pub fn validate(&self, catalog: &Catalog) -> Result<(), Error> {
        let Some(name) = &self.model else {
            return if self.effort.is_none() {
                Ok(())
            } else {
                Err(Error(
                    "Choose an explicit model before setting reasoning effort.".into(),
                ))
            };
        };
        if !catalog.complete {
            return Err(Error(
                "Load the complete catalog before choosing a model.".into(),
            ));
        }
        let model = catalog.find(name).ok_or_else(|| {
            Error(format!(
                "Model {name:?} is unavailable; reload models or use the current session settings."
            ))
        })?;
        if let Some(effort) = &self.effort
            && !model
                .supported_reasoning_efforts
                .iter()
                .any(|option| &option.reasoning_effort == effort)
        {
            return Err(Error(format!(
                "Reasoning effort {effort:?} is not supported by {}.",
                model.display_name
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn model(name: &str) -> Model {
        serde_json::from_value(json!({"id":name,"model":name,"displayName":name,"description":"Synthetic", "hidden":false,"isDefault":true,"defaultReasoningEffort":"future-effort","supportedReasoningEfforts":[{"reasoningEffort":"future-effort","description":"Provider option"}]})).unwrap()
    }

    #[test]
    fn options_follow_provider_capabilities_and_omit_defaults() {
        let mut catalog = Catalog::default();
        assert_eq!(
            serde_json::to_value(TurnOptions::default()).unwrap(),
            json!({})
        );
        assert!(TurnOptions::new(&catalog, Some("a".into()), None).is_err());
        catalog
            .apply(
                &None,
                Page {
                    data: vec![model("a")],
                    next_cursor: None,
                },
            )
            .unwrap();
        let options =
            TurnOptions::new(&catalog, Some("a".into()), Some("future-effort".into())).unwrap();
        assert_eq!(
            serde_json::to_value(&options).unwrap(),
            json!({"model":"a","effort":"future-effort"})
        );
        assert!(TurnOptions::new(&catalog, None, Some("future-effort".into())).is_err());
        assert!(TurnOptions::new(&catalog, Some("a".into()), Some("unsupported".into())).is_err());
        catalog.models[0].hidden = true;
        assert!(options.validate(&catalog).is_err());
        assert!(TurnOptions::default().validate(&catalog).is_ok());
    }

    #[test]
    fn invalid_pages_preserve_catalog_and_cursor() {
        let mut catalog = Catalog::default();
        catalog
            .apply(
                &None,
                Page {
                    data: vec![model("a")],
                    next_cursor: Some("one".into()),
                },
            )
            .unwrap();
        for page in [
            Page {
                data: vec![model("b")],
                next_cursor: Some("one".into()),
            },
            Page {
                data: vec![model("a")],
                next_cursor: None,
            },
            Page {
                data: vec![],
                next_cursor: Some("two".into()),
            },
        ] {
            assert!(catalog.apply(&Some("one".into()), page).is_err());
            assert_eq!(catalog.models.len(), 1);
            assert_eq!(catalog.next_cursor.as_deref(), Some("one"));
        }
        catalog
            .apply(
                &Some("one".into()),
                Page {
                    data: vec![model("b")],
                    next_cursor: None,
                },
            )
            .unwrap();
        assert!(catalog.complete);
        assert!(
            catalog
                .apply(
                    &Some("one".into()),
                    Page {
                        data: vec![model("c")],
                        next_cursor: None
                    }
                )
                .is_err()
        );
        catalog
            .apply(
                &None,
                Page {
                    data: vec![],
                    next_cursor: None,
                },
            )
            .unwrap();
        assert!(catalog.complete && catalog.models.is_empty());
    }

    #[test]
    fn catalog_bounds_and_invalid_efforts_are_rejected_atomically() {
        let mut catalog = Catalog::default();
        assert!(params(&Some(String::new())).is_err());
        assert!(params(&Some("x".repeat(MAX_CURSOR_BYTES + 1))).is_err());
        assert!(
            catalog
                .apply(
                    &None,
                    Page {
                        data: vec![model("a")],
                        next_cursor: Some("x".repeat(MAX_CURSOR_BYTES + 1))
                    }
                )
                .is_err()
        );
        assert!(
            catalog
                .apply(
                    &None,
                    Page {
                        data: (0..=PAGE_SIZE).map(|i| model(&i.to_string())).collect(),
                        next_cursor: None
                    }
                )
                .is_err()
        );
        for offset in (0..256).step_by(PAGE_SIZE) {
            let cursor = catalog.next_cursor.clone();
            catalog
                .apply(
                    &cursor,
                    Page {
                        data: (offset..offset + PAGE_SIZE)
                            .map(|i| model(&i.to_string()))
                            .collect(),
                        next_cursor: Some((offset + PAGE_SIZE).to_string()),
                    },
                )
                .unwrap();
        }
        let cursor = catalog.next_cursor.clone();
        assert!(
            catalog
                .apply(
                    &cursor,
                    Page {
                        data: vec![model("extra")],
                        next_cursor: None
                    }
                )
                .is_err()
        );
        assert_eq!(catalog.models.len(), 256);
        let mut invalid = model("invalid");
        invalid
            .supported_reasoning_efforts
            .push(invalid.supported_reasoning_efforts[0].clone());
        assert!(
            catalog
                .apply(
                    &None,
                    Page {
                        data: vec![invalid],
                        next_cursor: None
                    }
                )
                .is_err()
        );
        let mut huge = model("huge");
        huge.description = "x".repeat(MAX_BYTES);
        assert!(
            catalog
                .apply(
                    &None,
                    Page {
                        data: vec![huge],
                        next_cursor: None
                    }
                )
                .is_err()
        );
        assert_eq!(catalog.models.len(), 256);
    }
}
