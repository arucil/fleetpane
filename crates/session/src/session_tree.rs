//! Observational provider session ancestry and loaded-context inventory.
use crate::client::{Connection, Error, snapshot};
use fleetpane_runtime::{Snapshot, lifecycle::ActiveTurn};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::Duration,
};
use uuid::Uuid;

const PAGE: usize = 100;
pub const MAX_NODES: usize = 5_000;
const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_DEPTH: usize = 64;
const SOURCES: &[&str] = &[
    "cli",
    "vscode",
    "exec",
    "appServer",
    "subAgent",
    "subAgentReview",
    "subAgentCompact",
    "subAgentThreadSpawn",
    "subAgentOther",
    "unknown",
];

fn invalid(message: impl Into<String>) -> Error {
    Error::Protocol(message.into())
}
fn parent<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Option::deserialize(d)
}
/// Deserialization proof that an inventory response omitted conversation history.
#[derive(Clone, Debug)]
struct EmptyHistory;
fn empty_turns<'de, D: Deserializer<'de>>(d: D) -> Result<EmptyHistory, D::Error> {
    let turns = Vec::<serde::de::IgnoredAny>::deserialize(d)?;
    if turns.is_empty() {
        Ok(EmptyHistory)
    } else {
        Err(serde::de::Error::custom(
            "Session inventory unexpectedly included conversation turns",
        ))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub id: String,
    pub session_id: String,
    // A missing field must not silently turn an unknown ancestor into a root.
    #[serde(deserialize_with = "parent")]
    pub parent_thread_id: Option<String>,
    pub forked_from_id: Option<String>,
    pub cwd: String,
    pub name: Option<String>,
    pub preview: String,
    pub ephemeral: bool,
    pub status: crate::ThreadStatus,
    #[serde(rename = "turns", deserialize_with = "empty_turns", skip_serializing)]
    _history_omitted: EmptyHistory,
}
impl Metadata {
    fn validate(&self) -> Result<(), Error> {
        if [&self.id, &self.session_id]
            .iter()
            .any(|s| s.is_empty() || s.len() > 512)
            || [&self.parent_thread_id, &self.forked_from_id]
                .iter()
                .any(|s| s.as_ref().is_some_and(|s| s.is_empty() || s.len() > 512))
            || !Path::new(&self.cwd).is_absolute()
            || self.cwd.len() > 16 * 1024
            || self.name.as_ref().is_some_and(|s| s.len() > 4096)
            || self.preview.len() > 8192
            || self.status.kind.is_empty()
            || self.status.kind.len() > 256
        {
            return Err(invalid(
                "Session ancestry metadata exceeds its identity/text bounds",
            ));
        }
        Ok(())
    }
    pub fn title(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.preview)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Row {
    pub metadata: Metadata,
    pub depth: usize,
    /// None means it was observed outside the persistent directory.
    pub archived: Option<bool>,
    pub loaded: bool,
    pub active_turn: Option<ActiveTurn>,
    pub pending_requests: usize,
}
#[derive(Clone, Debug, Serialize)]
pub struct View {
    pub host_id: Uuid,
    pub root_id: String,
    pub sequence_before: u64,
    pub sequence_after: u64,
    pub scanned_nodes: usize,
    pub rows: Vec<Row>,
    pub unresolved_same_session: Vec<String>,
    pub other_loaded_contexts: usize,
    pub unassigned_requests: usize,
    pub in_flight_mutations: usize,
    pub unknown_operations: u64,
    pub warnings: Vec<String>,
}
type Index = BTreeMap<String, (Metadata, Option<bool>)>;

fn project(
    index: &Index,
    loaded: &BTreeSet<String>,
    state: &Snapshot,
    root: &str,
    before: u64,
) -> Result<View, Error> {
    let root_metadata = &index
        .get(root)
        .ok_or_else(|| invalid("Selected session is absent from the inspection"))?
        .0;
    let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (id, (metadata, _)) in index {
        if let Some(parent) = metadata.parent_thread_id.as_deref() {
            children.entry(parent).or_default().push(id);
        }
    }
    let mut pending = BTreeMap::<&str, usize>::new();
    let mut unassigned = 0;
    for request in &state.pending_requests {
        if let Some(id) = request
            .pointer("/params/threadId")
            .and_then(Value::as_str)
            .filter(|id| index.contains_key(*id))
        {
            *pending.entry(id).or_default() += 1;
        } else {
            unassigned += 1;
        }
    }
    let mut stack = vec![(root, 0)];
    let mut seen = BTreeSet::new();
    let mut rows = vec![];
    while let Some((id, depth)) = stack.pop() {
        if depth > MAX_DEPTH || !seen.insert(id) {
            return Err(invalid(
                "Session ancestry contains a cycle or exceeds depth 64",
            ));
        }
        let (metadata, archived) = index
            .get(id)
            .ok_or_else(|| invalid("Session child metadata disappeared"))?;
        if metadata.session_id != root_metadata.session_id {
            return Err(invalid(
                "Parent-linked sessions disagree about their provider sessionId",
            ));
        }
        rows.push(Row {
            metadata: metadata.clone(),
            depth,
            archived: *archived,
            loaded: loaded.contains(id),
            active_turn: state
                .activity
                .turns
                .iter()
                .find(|t| t.thread_id == id)
                .cloned(),
            pending_requests: pending.get(id).copied().unwrap_or(0),
        });
        for child in children.get(id).into_iter().flatten().rev() {
            stack.push((child, depth + 1));
        }
    }
    let mut unresolved = vec![];
    for (id, (metadata, _)) in index.iter().filter(|(id, (m, _))| {
        m.session_id == root_metadata.session_id && !seen.contains(id.as_str())
    }) {
        let mut ancestors = BTreeSet::new();
        let mut current = metadata;
        while let Some(parent) = &current.parent_thread_id {
            if ancestors.len() >= MAX_DEPTH || !ancestors.insert(parent) {
                return Err(invalid(
                    "Related session ancestry contains a cycle or exceeds depth 64",
                ));
            }
            let Some((next, _)) = index.get(parent) else {
                unresolved.push(id.clone());
                break;
            };
            current = next;
        }
    }
    let visible_loaded = rows.iter().filter(|r| r.loaded).count();
    let mut warnings=vec!["Multi-request observation; session state can change during and after inspection. This is not permission to archive or proof of quiescence.".into(),"Background commands and other provider resources are not enumerated; idle or unloaded metadata is not a resource-complete shutdown receipt.".into()];
    if !unresolved.is_empty() {
        warnings.push("Some sessions with the same sessionId have unresolved parent chains; subtree impact is incomplete.".into());
    }
    Ok(View {
        host_id: state.host_id,
        root_id: root.into(),
        sequence_before: before,
        sequence_after: state.sequence,
        scanned_nodes: index.len(),
        rows,
        unresolved_same_session: unresolved,
        other_loaded_contexts: loaded.len() - visible_loaded,
        unassigned_requests: unassigned,
        in_flight_mutations: state.activity.in_flight_operations,
        unknown_operations: state.activity.unknown_operations,
        warnings,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page<T> {
    data: Vec<T>,
    next_cursor: Option<String>,
}
async fn read(connection: &Connection, method: &str, params: Value) -> Result<Value, Error> {
    debug_assert!(fleetpane_runtime::is_query_method(method));
    Ok(connection
        .execute_rpc(method, params, Uuid::new_v4())
        .await?
        .result)
}
fn advance(
    cursor: &mut Option<String>,
    seen: &mut BTreeSet<String>,
    next: Option<String>,
) -> Result<(), Error> {
    if let Some(value) = &next
        && (value.is_empty() || value.len() > 4096 || !seen.insert(value.clone()))
    {
        return Err(invalid("Session inventory cursor is invalid or repeated"));
    }
    if seen.len() > 100 {
        return Err(invalid("Session inventory exceeds 100 cursor pages"));
    }
    *cursor = next;
    Ok(())
}
async fn loaded(connection: &Connection) -> Result<BTreeSet<String>, Error> {
    let mut cursor = None;
    let mut seen = BTreeSet::new();
    let mut ids = BTreeSet::new();
    loop {
        let page: Page<String> = serde_json::from_value(
            read(
                connection,
                "thread/loaded/list",
                json!({"limit":PAGE,"cursor":cursor}),
            )
            .await?,
        )
        .map_err(|e| invalid(e.to_string()))?;
        if page.data.len() > PAGE {
            return Err(invalid("Loaded session page exceeds 100 records"));
        }
        for id in page.data {
            if id.is_empty() || id.len() > 512 || !ids.insert(id) || ids.len() > MAX_NODES {
                return Err(invalid(
                    "Loaded session inventory has invalid, duplicate or more than 5000 identities",
                ));
            }
        }
        advance(&mut cursor, &mut seen, page.next_cursor)?;
        if cursor.is_none() {
            break;
        }
    }
    Ok(ids)
}
fn insert(
    index: &mut Index,
    bytes: &mut usize,
    metadata: Metadata,
    archived: Option<bool>,
) -> Result<(), Error> {
    metadata.validate()?;
    if index.contains_key(&metadata.id) || index.len() >= MAX_NODES {
        return Err(invalid(
            "Session directory overlaps, changes membership or exceeds 5000 nodes; inspect again",
        ));
    }
    // Count each record once; serializing the whole growing inventory here is quadratic.
    let encoded = serde_json::to_vec(&(&metadata.id, &metadata, archived))
        .map_err(|e| invalid(e.to_string()))?
        .len()
        + 1;
    if encoded > MAX_BYTES - *bytes {
        return Err(invalid("Session metadata inventory exceeds 8 MiB"));
    }
    *bytes += encoded;
    index.insert(metadata.id.clone(), (metadata, archived));
    Ok(())
}

/// All reads borrow the current Host lease, never claim, resume, stop or mutate.
pub async fn inspect(socket: &Path, host_id: Uuid, root: &str) -> Result<View, Error> {
    if root.is_empty() || root.len() > 512 {
        return Err(Error::Input("Invalid session-tree root identity".into()));
    }
    tokio::time::timeout(Duration::from_secs(30), collect(socket, host_id, root))
        .await
        .map_err(|_| {
            Error::Input(
                "Session tree inspection exceeded 30 seconds; no mutation was dispatched".into(),
            )
        })?
}
async fn collect(socket: &Path, host_id: Uuid, root: &str) -> Result<View, Error> {
    let first = snapshot(socket).await?;
    if first.host_id != host_id {
        return Err(invalid("Session tree belongs to a replaced Host"));
    }
    if first
        .provider_info
        .get("fleetpaneProvider")
        .and_then(Value::as_str)
        == Some("deepseek-acp")
    {
        return Err(Error::Input(
            "DeepSeek ACP does not advertise session ancestry or loaded-context inventory".into(),
        ));
    }
    let lease = first.owner.clone().ok_or_else(|| {
        Error::Input("No Host lease is available for observational session queries".into())
    })?;
    let connection = Connection {
        socket: socket.to_owned(),
        lease,
    };
    let loaded_before = loaded(&connection).await?;
    let mut index = Index::new();
    let mut bytes = 2;
    for archived in [false, true] {
        let mut cursor = None;
        let mut seen = BTreeSet::new();
        loop {
            let page:Page<Metadata>=serde_json::from_value(read(&connection,"thread/list",json!({"limit":PAGE,"cursor":cursor,"archived":archived,"sourceKinds":SOURCES,"modelProviders":[],"sortKey":"created_at","sortDirection":"asc"})).await?).map_err(|e|invalid(e.to_string()))?;
            if page.data.len() > PAGE {
                return Err(invalid("Session metadata page exceeds 100 records"));
            }
            for metadata in page.data {
                insert(&mut index, &mut bytes, metadata, Some(archived))?;
            }
            advance(&mut cursor, &mut seen, page.next_cursor)?;
            if cursor.is_none() {
                break;
            }
        }
    }
    for id in loaded_before
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(root))
    {
        if index.contains_key(id) {
            continue;
        }
        let value = read(
            &connection,
            "thread/read",
            json!({"threadId":id,"includeTurns":false}),
        )
        .await?;
        let metadata: Metadata = serde_json::from_value(
            value
                .get("thread")
                .cloned()
                .ok_or_else(|| invalid("Session read has no thread"))?,
        )
        .map_err(|e| invalid(e.to_string()))?;
        if metadata.id != id {
            return Err(invalid("Session metadata read returned another identity"));
        }
        insert(&mut index, &mut bytes, metadata, None)?;
    }
    if loaded(&connection).await? != loaded_before {
        return Err(invalid(
            "Loaded contexts changed during inspection; inspect again",
        ));
    }
    let last = snapshot(socket).await?;
    if last.host_id != host_id || last.owner.as_ref() != Some(&connection.lease) {
        return Err(invalid(
            "Host or controller changed during inspection; results were not adopted",
        ));
    }
    project(&index, &loaded_before, &last, root, first.sequence)
}

#[cfg(test)]
mod tests;
