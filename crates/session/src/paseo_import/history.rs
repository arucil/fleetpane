//! Private snapshots, streamed from provider-owned logs. No provider is started.
mod dsh;
use super::{Session, invalid};
use crate::{Item, client::Error, storage::Store};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const MAX_LINE: u64 = 64 * 1024 * 1024;
const MAX_LOG: u64 = 1024 * 1024 * 1024;
const MAX_ITEM: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Roots {
    pub codex: PathBuf,
    pub dsh: PathBuf,
}
impl Roots {
    pub fn environment() -> Result<Self, Error> {
        let home = std::env::var_os("HOME")
            .ok_or_else(|| invalid("HOME unavailable; provide provider roots explicitly"))?;
        Ok(Self {
            codex: std::env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(&home).join(".codex")),
            dsh: std::env::var_os("DSH_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(home).join(".dsh")),
        })
    }
}
#[derive(Debug, Deserialize, Serialize)]
pub struct HistoryStatus {
    pub session_id: Uuid,
    pub rows: u64,
    pub source_records: u64,
    pub source: PathBuf,
    pub source_bytes: u64,
    pub warning: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct MigrationReport {
    pub copied: usize,
    pub failures: Vec<(Uuid, String)>,
}
#[derive(Debug, Serialize)]
pub struct HistoryPage {
    pub session_id: Uuid,
    pub items: Vec<Item>,
    pub next: Option<u64>,
    pub status: HistoryStatus,
}

pub(super) fn encoded(raw: &str) -> String {
    if raw == "." {
        return "~002E".into();
    }
    if raw == ".." {
        return "~002E~002E".into();
    }
    raw.encode_utf16()
        .map(|n| {
            if n < 128
                && (u8::try_from(n).expect("ASCII").is_ascii_alphanumeric()
                    || b"._-".contains(&u8::try_from(n).expect("ASCII")))
            {
                char::from(u8::try_from(n).expect("ASCII")).to_string()
            } else {
                format!("~{n:04X}")
            }
        })
        .collect()
}
pub(super) fn project_key(cwd: &str) -> String {
    let mut body = String::new();
    let mut separator = false;
    for c in cwd.encode_utf16() {
        if matches!(c, 47 | 92 | 58) {
            if !separator {
                body.push('-');
            }
            separator = true;
        } else {
            separator = false;
            if c < 128
                && (u8::try_from(c).expect("ASCII").is_ascii_alphanumeric()
                    || b"._-".contains(&u8::try_from(c).expect("ASCII")))
            {
                body.push(char::from(u8::try_from(c).expect("ASCII")));
            } else {
                body.push_str(&format!("~{c:04X}"));
            }
        }
    }
    let body = body.trim_start_matches('-');
    let body = if body.is_empty() { "root" } else { body };
    format!("--{}--", &body[..body.len().min(251)])
}
fn source(session: &Session, roots: &Roots) -> Result<(PathBuf, Option<u64>), Error> {
    let handle = session
        .agent
        .persistence
        .as_ref()
        .ok_or_else(|| invalid("Paseo session has no durable provider handle"))?;
    if handle.provider != session.agent.provider || handle.session_id.is_empty() {
        return Err(invalid("provider handle does not match session"));
    }
    let root = match session.agent.provider.as_str() {
        "codex" => &roots.codex,
        "deepseek-harness" => &roots.dsh,
        _ => {
            return Err(invalid(
                "history import currently supports Codex and DeepSeek Harness",
            ));
        }
    };
    if !root.is_absolute() {
        return Err(invalid("provider root must be absolute"));
    }
    let root = std::fs::canonicalize(root).map_err(invalid)?;
    if session.agent.provider == "codex" {
        // Codex's authoritative thread index; open read-only, never create it.
        let db = root.join("state_5.sqlite");
        let db =
            Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(invalid)?;
        let path: Option<String> = db
            .query_row(
                "SELECT rollout_path FROM threads WHERE id=?1",
                [&handle.session_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(invalid)?;
        let path = PathBuf::from(path.ok_or_else(|| {
            invalid("Codex thread absent from state_5.sqlite; this index version is required")
        })?);
        let path = std::fs::canonicalize(path).map_err(invalid)?;
        if !path.starts_with(&root) {
            return Err(invalid(
                "Codex history path escapes configured provider root",
            ));
        }
        return Ok((path, None));
    }
    let cwd = session
        .agent
        .cwd
        .to_str()
        .ok_or_else(|| invalid("non-UTF-8 cwd"))?;
    let dir = root
        .join("sessions")
        .join(project_key(cwd))
        .join(encoded(&handle.session_id));
    dsh::source(&dir, &root).map(|(path, version)| (path, Some(version)))
}

fn item(value: Value) -> Result<Item, Error> {
    serde_json::from_value(value).map_err(invalid)
}
fn text_item(id: &str, kind: &str, text: &str) -> Result<Item, Error> {
    item(json!({"id":id,"type":kind,"text":text}))
}
fn blocks(values: &[Value]) -> Vec<Value> {
    values
        .iter()
        .map(|v| match v["type"].as_str() {
            Some("input_text" | "output_text") => json!({"type":"text","text":v["text"]}),
            Some("input_image") => json!({"type":"image","url":v["image_url"]}),
            _ => v.clone(),
        })
        .collect()
}
fn project(v: &Value, provider: &str, index: u64) -> Result<Vec<Item>, Error> {
    let id = format!("paseo-{index}");
    let mut result = Vec::new();
    if provider == "codex" {
        if v["type"] != "response_item" {
            return Ok(result);
        }
        let p = &v["payload"];
        match p["type"].as_str(){
            Some("message") if matches!(p["role"].as_str(),Some("user"|"assistant"))=>{
                let content=p["content"].as_array().ok_or_else(||invalid("Codex message lacks content array"))?;
                result.push(item(json!({"id":id,"type":if p["role"]=="user"{"userMessage"}else{"acpMessage"},"content":blocks(content)}))?);
            }
            Some("function_call"|"custom_tool_call")=>result.push(item(json!({"id":id,"type":"acpToolCall","call":{"title":p["name"],"status":"historical","rawInput":if p["type"]=="function_call"{&p["arguments"]}else{&p["input"]},"rawOutput":null},"source":p}))?),
            Some("function_call_output"|"custom_tool_call_output")=>result.push(item(json!({"id":id,"type":"acpToolCall","call":{"title":"Tool result","status":"historical","rawOutput":p["output"]},"source":p}))?),
            Some("reasoning")=>{
                if let Some(summary)=p["summary"].as_array(){for (i,part) in summary.iter().enumerate(){if let Some(text)=part["text"].as_str(){result.push(item(json!({"id":format!("{id}-{i}"),"type":"reasoning","summary":[text],"content":[]}))?);}}}
            }
            _=>{}
        }
    } else {
        let d = &v["data"];
        match v["type"].as_str(){
            Some("user/message")=>{let content=d["content"].as_array().ok_or_else(||invalid("DSH user message lacks content array"))?;result.push(item(json!({"id":id,"type":"userMessage","content":content}))?);}
            Some("assistant/message")=>{
                let content=d["message"]["content"].as_array().ok_or_else(||invalid("DSH assistant message lacks content array"))?;
                for (i,b) in content.iter().enumerate(){match b["type"].as_str(){
                    Some("text")=>result.push(text_item(&format!("{id}-{i}"),"agentMessage",b["text"].as_str().ok_or_else(||invalid("DSH assistant text is not a string"))?)?),
                    Some("reasoning")=>result.push(item(json!({"id":format!("{id}-{i}"),"type":"reasoning","summary":[b["text"]],"content":[]}))?),
                    Some("tool-call")=>{},
                    _=>result.push(item(json!({"id":format!("{id}-{i}"),"type":"paseoContent","source":b}))?),
                }}
            }
            Some("tool/call")=>result.push(item(json!({"id":id,"type":"acpToolCall","call":{"title":d["name"],"status":"historical","rawInput":d["arguments"]},"source":d}))?),
            Some("tool/result")=>result.push(item(json!({"id":id,"type":"acpToolCall","call":{"title":"Tool result","status":"historical","rawOutput":d["message"]["content"]},"source":d}))?),
            Some("todo/write")=>result.push(item(json!({"id":id,"type":"paseoTodo","todos":d["todos"]}))?),
            _=>{}
        }
    }
    Ok(result)
}

impl Store {
    /// Sequential per-session copies are individually atomic and resumable.
    /// A missing/unsupported history stays an explicit failure, never an empty success.
    pub async fn copy_paseo_workspace_history(
        &self,
        parent: Uuid,
        roots: Roots,
    ) -> Result<MigrationReport, Error> {
        let mut report = MigrationReport {
            copied: 0,
            failures: Vec::new(),
        };
        let mut before = None;
        loop {
            let sessions = self.paseo_sessions(parent, before).await?;
            if sessions.is_empty() {
                break;
            }
            before = sessions.last().map(|s| s.position);
            for session in sessions {
                match self.snapshot_paseo_history(session.id, roots.clone()).await {
                    Ok(_) => report.copied += 1,
                    Err(e) => report.failures.push((session.id, e.to_string())),
                }
            }
        }
        Ok(report)
    }
    /// The archive is published only after the whole immutable-prefix copy is
    /// validated. Repeated calls keep the existing snapshot; no automatic refresh.
    pub async fn snapshot_paseo_history(
        &self,
        id: Uuid,
        roots: Roots,
    ) -> Result<HistoryStatus, Error> {
        let session = self.paseo_session(id).await?;
        let directory = self.root().join("paseo-history");
        let root = self.root().to_owned();
        tokio::task::spawn_blocking(move || {
            fleetpane_runtime::unix::prepare_directory(&directory).map_err(invalid)?;
            let target = directory.join(format!("{id}.sqlite"));
            if target.exists() {
                let saved = status(&target)?;
                if saved.session_id != id {
                    return Err(invalid("history belongs to another session"));
                }
                return Ok(saved);
            }
            let (source, version) = source(&session, &roots)?;
            let file = File::open(&source).map_err(invalid)?;
            let size = file.metadata().map_err(invalid)?.len();
            if !file.metadata().map_err(invalid)?.is_file() || size > MAX_LOG {
                return Err(invalid("history must be a regular file within 1 GiB"));
            }
            let scratch = super::scratch::Writer::open(&root)?;
            let temporary = scratch.path();
            let result = build(&temporary, id, &source, file, size, &session, version);
            match result {
                Ok(summary) => {
                    // Hard-link publication never replaces another successful writer.
                    match std::fs::hard_link(&temporary, &target) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(e) => {
                            std::fs::remove_file(&temporary).map_err(invalid)?;
                            return Err(invalid(e));
                        }
                    }
                    std::fs::remove_file(&temporary).map_err(invalid)?;
                    File::open(directory)
                        .and_then(|f| f.sync_all())
                        .map_err(invalid)?;
                    let saved = status(&target)?;
                    if saved.session_id != summary.session_id {
                        return Err(invalid("published history identity mismatch"));
                    }
                    Ok(saved)
                }
                Err(e) => {
                    if temporary.exists() {
                        std::fs::remove_file(temporary).map_err(invalid)?;
                    }
                    Err(e)
                }
            }
        })
        .await
        .map_err(invalid)?
    }
    pub async fn paseo_history(&self, id: Uuid, after: u64) -> Result<HistoryPage, Error> {
        self.paseo_session(id).await?;
        let path = self
            .root()
            .join("paseo-history")
            .join(format!("{id}.sqlite"));
        tokio::task::spawn_blocking(move || {
            let db = open(&path)?;
            let status = status(&path)?;
            if status.session_id != id {
                return Err(invalid("history belongs to another session"));
            }
            let mut q = db
                .prepare("SELECT seq,data FROM items WHERE seq>?1 ORDER BY seq LIMIT 32")
                .map_err(invalid)?;
            let mut items = Vec::new();
            let mut last = after;
            let mut bytes = 0;
            for row in q
                .query_map([i64::try_from(after).map_err(invalid)?], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(invalid)?
            {
                let (seq, text) = row.map_err(invalid)?;
                if bytes + text.len() > 256 * 1024 && !items.is_empty() {
                    break;
                }
                bytes += text.len();
                items.push(serde_json::from_str(&text).map_err(invalid)?);
                last = seq.try_into().map_err(invalid)?;
            }
            Ok(HistoryPage {
                session_id: id,
                items,
                next: (last < status.rows).then_some(last),
                status,
            })
        })
        .await
        .map_err(invalid)?
    }
    pub async fn paseo_source_record(&self, id: Uuid, record: u64) -> Result<Value, Error> {
        self.paseo_session(id).await?;
        let path = self
            .root()
            .join("paseo-history")
            .join(format!("{id}.sqlite"));
        tokio::task::spawn_blocking(move || {
            let db = open(&path)?;
            if status(&path)?.session_id != id {
                return Err(invalid("history belongs to another session"));
            }
            let text: String = db
                .query_row(
                    "SELECT data FROM records WHERE seq=?1",
                    [i64::try_from(record).map_err(invalid)?],
                    |r| r.get(0),
                )
                .map_err(invalid)?;
            serde_json::from_str(&text).map_err(invalid)
        })
        .await
        .map_err(invalid)?
    }
}
fn open(path: &Path) -> Result<Connection, Error> {
    let meta = std::fs::symlink_metadata(path).map_err(invalid)?;
    if !meta.is_file() || meta.permissions().mode() & 0o077 != 0 {
        return Err(invalid("history must be a private regular file (0600)"));
    }
    let parent = std::fs::canonicalize(
        path.parent()
            .ok_or_else(|| invalid("history path has no parent"))?,
    )
    .map_err(invalid)?;
    Connection::open_with_flags(
        parent.join(
            path.file_name()
                .ok_or_else(|| invalid("history path has no name"))?,
        ),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(invalid)
}
fn status(path: &Path) -> Result<HistoryStatus, Error> {
    let db = open(path)?;
    let data: String = db
        .query_row("SELECT data FROM metadata", [], |r| r.get(0))
        .map_err(invalid)?;
    serde_json::from_str(&data).map_err(invalid)
}

/// Materialize the immutable raw archive, never the provider-owned source log.
/// Paginated Codex rollouts need a separate thread store and are not self-contained.
pub(super) fn export_codex(root: &Path, session: &Session) -> Result<PathBuf, Error> {
    let id = session.id;
    let archive = root.join("paseo-history").join(format!("{id}.sqlite"));
    let db = open(&archive)?;
    let saved = status(&archive)?;
    if saved.session_id != id || saved.warning.is_some() {
        return Err(invalid(
            "Fork requires a complete history snapshot without an incomplete-tail warning",
        ));
    }
    let header: String = db
        .query_row("SELECT data FROM records WHERE seq=1", [], |r| r.get(0))
        .map_err(invalid)?;
    let header: Value = serde_json::from_str(&header).map_err(invalid)?;
    let handle = session
        .agent
        .persistence
        .as_ref()
        .ok_or_else(|| invalid("session has no provider binding"))?;
    if header["type"] != "session_meta"
        || header["payload"]["id"] != handle.session_id
        || header["payload"]["cwd"] != session.agent.cwd.to_string_lossy().as_ref()
    {
        return Err(invalid("Codex snapshot identity/directory mismatch"));
    }
    match header["payload"].get("history_mode") {
        None => {}
        Some(Value::String(mode)) if mode == "legacy" => {}
        Some(_) => {
            return Err(invalid(
                "This Codex history requires its separate thread database or has an unsupported history mode; a JSONL snapshot cannot restore its complete context",
            ));
        }
    }
    let directory = root.join("paseo-forks");
    fleetpane_runtime::unix::prepare_directory(&directory).map_err(invalid)?;
    let target = directory.join(format!("{id}.jsonl"));
    let scratch = super::scratch::Writer::open(root)?;
    let temporary = scratch.path();
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(invalid)?;
        let mut statement = db
            .prepare("SELECT data FROM records ORDER BY seq")
            .map_err(invalid)?;
        let mut records = 0_u64;
        let mut bytes = 0_u64;
        for row in statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(invalid)?
        {
            let text = row.map_err(invalid)?;
            bytes += text.len() as u64;
            if bytes > MAX_LOG {
                return Err(invalid("fork export exceeds 1 GiB"));
            }
            file.write_all(text.as_bytes()).map_err(invalid)?;
            if !text.ends_with('\n') {
                file.write_all(b"\n").map_err(invalid)?;
            }
            records += 1;
        }
        if records != saved.source_records {
            return Err(invalid("snapshot raw record count mismatch"));
        }
        file.sync_all().map_err(invalid)?;
        match std::fs::hard_link(&temporary, &target) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = std::fs::symlink_metadata(&target).map_err(invalid)?;
                if !metadata.is_file()
                    || metadata.len() > MAX_LOG
                    || metadata.permissions().mode() & 0o077 != 0
                {
                    return Err(invalid(
                        "fork export must be a private regular file within 1 GiB",
                    ));
                }
                let digest = |path: &Path| -> Result<Vec<u8>, Error> {
                    use sha2::{Digest, Sha256};
                    let mut digest = Sha256::new();
                    let mut input = OpenOptions::new()
                        .read(true)
                        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
                        .open(path)
                        .map_err(invalid)?;
                    let mut buffer = [0u8; 65536];
                    loop {
                        let n = input.read(&mut buffer).map_err(invalid)?;
                        if n == 0 {
                            break;
                        }
                        digest.update(&buffer[..n]);
                    }
                    Ok(digest.finalize().to_vec())
                };
                if digest(&temporary)? != digest(&target)? {
                    return Err(invalid("saved fork export changed; refusing to replace it"));
                }
            }
            Err(e) => return Err(invalid(e)),
        }
        File::open(&directory)
            .and_then(|f| f.sync_all())
            .map_err(invalid)?;
        std::fs::canonicalize(&target).map_err(invalid)
    })();
    if temporary.exists() {
        std::fs::remove_file(&temporary).map_err(invalid)?;
    }
    result
}
fn build(
    path: &Path,
    id: Uuid,
    source: &Path,
    file: File,
    size: u64,
    session: &Session,
    version: Option<u64>,
) -> Result<HistoryStatus, Error> {
    let created = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(invalid)?;
    drop(created);
    let mut db = Connection::open(path).map_err(invalid)?;
    db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=EXTRA; CREATE TABLE items(seq INTEGER PRIMARY KEY,data TEXT NOT NULL);CREATE TABLE records(seq INTEGER PRIMARY KEY,data TEXT NOT NULL);CREATE TABLE metadata(data TEXT NOT NULL);").map_err(invalid)?;
    let reader: Box<dyn Read> = if source.extension().is_some_and(|x| x == "zstd") {
        Box::new(zstd::stream::read::Decoder::new(file.take(size)).map_err(invalid)?)
    } else {
        Box::new(file.take(size))
    };
    let mut reader = BufReader::new(reader.take(MAX_LOG + 1));
    let tx = db.transaction().map_err(invalid)?;
    let mut total = 0;
    let mut record = 0;
    let mut rows = 0;
    let mut source_records = 0_u64;
    let mut warning = None;
    let mut header = false;
    loop {
        let mut line = Vec::new();
        let n = reader
            .by_ref()
            .take(MAX_LINE + 1)
            .read_until(b'\n', &mut line)
            .map_err(invalid)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        record += 1;
        if n as u64 > MAX_LINE || total > MAX_LOG {
            return Err(invalid(
                "history exceeds 64 MiB per record or 1 GiB decoded",
            ));
        }
        if !line.ends_with(b"\n") {
            warning =
                Some("Final record is incomplete; only complete source records were copied".into());
            break;
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let v: Value = serde_json::from_slice(&line)
            .map_err(|e| invalid(format!("invalid history JSON at record {record}: {e}")))?;
        source_records += 1;
        tx.execute(
            "INSERT INTO records(seq,data) VALUES(?1,?2)",
            params![
                i64::try_from(source_records).map_err(invalid)?,
                std::str::from_utf8(&line).map_err(invalid)?
            ],
        )
        .map_err(invalid)?;
        if !header {
            let handle = session
                .agent
                .persistence
                .as_ref()
                .expect("source verified handle");
            let valid = if session.agent.provider == "codex" {
                v["type"] == "session_meta"
                    && v["payload"]["id"] == handle.session_id
                    && v["payload"]["cwd"].as_str() == session.agent.cwd.to_str()
            } else {
                v["type"] == "session"
                    && matches!(v["version"].as_u64(), Some(3 | 4))
                    && v["version"].as_u64() == version
                    && v["id"] == handle.session_id
                    && v["cwd"].as_str() == session.agent.cwd.to_str()
            };
            if !valid {
                return Err(invalid(
                    "history header session ID/cwd/version differs from Paseo binding",
                ));
            }
            header = true;
            continue;
        }
        for item in project(&v, &session.agent.provider, record)? {
            let data = serde_json::to_string(&item).map_err(invalid)?;
            if data.len() > MAX_ITEM {
                return Err(invalid(
                    "one conversation item exceeds 4 MiB; snapshot was not published",
                ));
            }
            rows += 1;
            tx.execute(
                "INSERT INTO items(seq,data) VALUES(?1,?2)",
                params![i64::try_from(rows).map_err(invalid)?, data],
            )
            .map_err(invalid)?;
        }
    }
    if !header {
        return Err(invalid("history has no complete session header"));
    }
    let result = HistoryStatus {
        session_id: id,
        rows,
        source_records,
        source: source.into(),
        source_bytes: size,
        warning,
    };
    tx.execute(
        "INSERT INTO metadata(data) VALUES(?1)",
        [serde_json::to_string(&result).map_err(invalid)?],
    )
    .map_err(invalid)?;
    tx.commit().map_err(invalid)?;
    drop(db);
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(invalid)?;
    Ok(result)
}
