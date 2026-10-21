//! Provider-owned MCP discovery. Discovery may connect/start configured servers;
//! this is an explicit journaled command, not a Host observational query.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const PAGE_SIZE: usize = 32;
const MAX_SERVERS: usize = 128;
const MAX_TOOLS: usize = 2048;
const MAX_BYTES: usize = 1024 * 1024;

pub fn params(cursor: &Option<String>) -> Result<Value, Error> {
    validate_cursor(cursor)?;
    Ok(json!({"cursor":cursor,"limit":PAGE_SIZE,"detail":"toolsAndAuthOnly"}))
}

fn validate_cursor(cursor: &Option<String>) -> Result<(), Error> {
    if cursor
        .as_ref()
        .is_some_and(|s| s.is_empty() || s.len() > 4096)
    {
        return Err(Error(
            "Cursor must contain text and fit within 4 KiB.".into(),
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
    #[serde(flatten)]
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub name: String,
    pub auth_status: String,
    pub runtime_status: Option<String>,
    pub tools_error: Option<String>,
    pub tools: BTreeMap<String, Tool>,
    #[serde(flatten)]
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub data: Vec<Server>,
    pub next_cursor: Option<String>,
}

#[derive(Default)]
pub struct Catalog {
    pub servers: Vec<Server>,
    pub next_cursor: Option<String>,
    pub complete: bool,
    seen: BTreeSet<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("MCP catalog: {0}")]
pub struct Error(String);

impl Catalog {
    /// All validation precedes replacement; a bad page retains the last view.
    pub fn apply(&mut self, cursor: &Option<String>, page: Page) -> Result<(), Error> {
        validate_cursor(cursor)?;
        validate_cursor(&page.next_cursor)?;
        if cursor.is_some() && (self.complete || cursor != &self.next_cursor) {
            return Err(Error(
                "Page does not continue this catalog; discover again.".into(),
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
                .is_some_and(|next| seen.contains(next))
        {
            return Err(Error(
                "Invalid page size or repeated pagination cursor.".into(),
            ));
        }
        let mut servers = if cursor.is_some() {
            self.servers.clone()
        } else {
            Vec::new()
        };
        servers.extend(page.data);
        let mut names = BTreeSet::new();
        let mut tools = 0;
        for server in &servers {
            if server.name.trim().is_empty()
                || !names.insert(&server.name)
                || server.auth_status.trim().is_empty()
                || server
                    .runtime_status
                    .as_ref()
                    .is_some_and(|s| s.trim().is_empty())
                || server
                    .tools
                    .iter()
                    .any(|(key, tool)| key.trim().is_empty() || tool.name.trim().is_empty())
            {
                return Err(Error(
                    "Empty identity/status or duplicate server name.".into(),
                ));
            }
            tools += server.tools.len();
        }
        if servers.len() > MAX_SERVERS
            || tools > MAX_TOOLS
            || serde_json::to_vec(&servers)
                .map_err(|e| Error(e.to_string()))?
                .len()
                > MAX_BYTES
        {
            return Err(Error(
                "Catalog exceeds 128 servers / 2048 tools / 1 MiB.".into(),
            ));
        }
        self.servers = servers;
        self.next_cursor = page.next_cursor;
        self.complete = self.next_cursor.is_none();
        self.seen = seen;
        Ok(())
    }

    pub fn copy_text(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(
            &json!({"data":self.servers,"nextCursor":self.next_cursor,"complete":self.complete}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(name: &str, next: Option<&str>) -> Page {
        serde_json::from_value(json!({"data":[{"name":name,"authStatus":"future-auth","runtimeStatus":"future-state","toolsError":"discovery failed 中文","tools":{"qualified__tool":{"name":"bare","inputSchema":{"type":"object"},"_meta":{"future":true}}},"pluginId":"synthetic","futureField":[1,2]}],"nextCursor":next})).unwrap()
    }

    #[test]
    fn preserves_provider_identity_unknown_status_metadata_and_discovery_error() {
        let mut catalog = Catalog::default();
        catalog.apply(&None, page("first", Some("opaque"))).unwrap();
        catalog
            .apply(&Some("opaque".into()), page("中文", None))
            .unwrap();
        assert!(catalog.complete);
        let copy: Value = serde_json::from_str(&catalog.copy_text().unwrap()).unwrap();
        assert_eq!(copy["data"][0]["authStatus"], "future-auth");
        assert_eq!(copy["data"][0]["tools"]["qualified__tool"]["name"], "bare");
        assert_eq!(
            copy["data"][0]["tools"]["qualified__tool"]["_meta"]["future"],
            true
        );
        assert_eq!(copy["data"][0]["futureField"], json!([1, 2]));
        assert_eq!(copy["data"][1]["toolsError"], "discovery failed 中文");
        assert_eq!(
            params(&None).unwrap(),
            json!({"cursor":null,"limit":32,"detail":"toolsAndAuthOnly"})
        );
        assert!(params(&Some(String::new())).is_err());
    }

    #[test]
    fn invalid_pages_leave_previous_view_and_cursor_intact() {
        let mut catalog = Catalog::default();
        catalog.apply(&None, page("first", Some("next"))).unwrap();
        let before = catalog.copy_text().unwrap();
        for (cursor, invalid) in [
            (Some("wrong".into()), page("second", None)),
            (Some("next".into()), page("first", None)),
            (Some("next".into()), page("second", Some("next"))),
            (Some("next".into()), page("  ", None)),
            (
                Some("next".into()),
                Page {
                    data: vec![],
                    next_cursor: Some("new".into()),
                },
            ),
        ] {
            assert!(catalog.apply(&cursor, invalid).is_err());
            assert_eq!(catalog.copy_text().unwrap(), before);
        }
        let mut oversized = page("large", None);
        oversized.data[0]
            .metadata
            .insert("large".into(), Value::String("x".repeat(MAX_BYTES)));
        assert!(catalog.apply(&None, oversized).is_err());
        assert_eq!(catalog.copy_text().unwrap(), before);
        catalog
            .apply(
                &None,
                Page {
                    data: vec![],
                    next_cursor: None,
                },
            )
            .unwrap();
        assert!(catalog.complete && catalog.servers.is_empty());
    }
}
