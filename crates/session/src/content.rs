//! Typed, toolkit-independent content for conversation renderers.

use crate::{Error, Item};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    Markdown(String),
    Message(Vec<Part>),
    AgentParts(Vec<Part>),
    Tool(ToolCall),
    Command(Command),
    Mcp(McpCall),
    Files(FileChanges),
    Image(Image),
    Reasoning(String),
    Unknown(Value),
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub title: String,
    pub status: String,
    pub raw_input: Option<Value>,
    pub content: Option<Vec<Value>>,
    pub raw_output: Option<Value>,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub locations: Option<Vec<fleetpane_runtime::acp::tool_calls::Location>>,
}

impl ToolCall {
    pub fn display_title(&self) -> &str {
        if self.title.is_empty() {
            self.name
                .as_deref()
                .filter(|name| !name.is_empty())
                .unwrap_or("Tool call")
        } else {
            &self.title
        }
    }

    pub fn parts(&self) -> Result<Vec<Part>, Error> {
        self.content
            .iter()
            .flatten()
            .map(|entry| {
                if entry.get("type").and_then(Value::as_str) == Some("content") {
                    let content = entry.get("content").ok_or_else(|| {
                        Error::Projection("ACP tool content lacks its content block".into())
                    })?;
                    decode_part(content, true)
                } else {
                    // Diff and terminal references stay visible until their renderers exist.
                    Ok(Part::Unknown(entry.clone()))
                }
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    Text(String),
    Image(Image),
    Resource(Resource),
    ResourceLink(ResourceLink),
    Unknown(Value),
}

/// Inline protocol data, not permission to read the resource's URI.
/// Keep both payload fields when supplied; the raw Item retains extensions.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub uri: String,
    pub mime_type: Option<String>,
    pub text: Option<String>,
    pub blob: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLink {
    pub uri: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Image {
    Local(String),
    Url(String),
    Encoded { mime_type: String, data: String },
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Command {
    pub command: String,
    pub cwd: String,
    pub status: String,
    pub aggregated_output: Option<String>,
    pub exit_code: Option<i64>,
    pub duration_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct McpCall {
    pub server: String,
    pub tool: String,
    pub status: String,
    pub arguments: Value,
    pub result: Option<McpResult>,
    pub error: Option<Value>,
    pub duration_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct McpResult {
    pub content: Vec<Value>,
    pub structured_content: Option<Value>,
    pub is_error: Option<bool>,
}

impl McpResult {
    pub fn parts(&self) -> Result<Vec<Part>, Error> {
        self.content
            .iter()
            .map(|part| decode_part(part, true))
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[non_exhaustive]
pub struct FileChanges {
    pub status: String,
    pub changes: Vec<FileChange>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[non_exhaustive]
pub struct FileChange {
    pub path: String,
    pub kind: FileKind,
    pub diff: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FileKind {
    Add,
    Delete,
    Update { move_path: Option<String> },
}

impl FileChange {
    /// Codex sends full contents for add/delete and unified hunks for updates.
    /// Adapt that supplied data; never read or reconstruct a working-tree file.
    pub fn unified_patch(&self) -> Result<String, Error> {
        let path = serde_json::to_string(&self.path)?;
        match &self.kind {
            FileKind::Add | FileKind::Delete => {
                let count = self.diff.lines().count();
                let (old, new, hunk, sign) = match self.kind {
                    FileKind::Add => (
                        "/dev/null",
                        path.as_str(),
                        format!("@@ -0,0 +1,{count} @@"),
                        '+',
                    ),
                    FileKind::Delete => (
                        path.as_str(),
                        "/dev/null",
                        format!("@@ -1,{count} +0,0 @@"),
                        '-',
                    ),
                    FileKind::Update { .. } => unreachable!(),
                };
                let mut patch = format!("--- {old}\n+++ {new}\n");
                if count > 0 {
                    patch.push_str(&hunk);
                    patch.push('\n');
                    for line in self.diff.split_inclusive('\n') {
                        patch.push(sign);
                        patch.push_str(line);
                        if !line.ends_with('\n') {
                            patch.push_str("\n\\ No newline at end of file\n");
                        }
                    }
                }
                Ok(patch)
            }
            FileKind::Update { move_path } => {
                // Codex appends this display annotation to moved-file hunks.
                // The target is already carried by kind.move_path, not patch data.
                let diff = move_path
                    .as_ref()
                    .and_then(|target| self.diff.strip_suffix(&format!("\n\nMoved to: {target}")))
                    .unwrap_or(&self.diff);
                if diff.starts_with("diff --git ") || diff.starts_with("--- ") {
                    return Ok(diff.to_owned());
                }
                let target = serde_json::to_string(move_path.as_ref().unwrap_or(&self.path))?;
                Ok(format!("--- {path}\n+++ {target}\n{diff}"))
            }
        }
    }
}

impl Item {
    /// Empty reasoning has no visible content. Unknown item/part data stays inspectable.
    pub fn content(&self) -> Result<Option<Content>, Error> {
        let fields = serde_json::to_value(&self.fields)?;
        let content = match self.kind.as_str() {
            "agentMessage" | "plan" => Content::Markdown(self.text()?),
            "userMessage" => {
                #[derive(Deserialize)]
                struct Message {
                    content: Vec<Value>,
                }
                let message: Message = serde_json::from_value(fields)?;
                Content::Message(
                    message
                        .content
                        .iter()
                        .map(|part| decode_part(part, false))
                        .collect::<Result<_, _>>()?,
                )
            }
            "commandExecution" => Content::Command(serde_json::from_value(fields)?),
            "mcpToolCall" => Content::Mcp(serde_json::from_value(fields)?),
            "acpToolCall" => Content::Tool(serde_json::from_value(
                fields
                    .get("call")
                    .cloned()
                    .ok_or_else(|| Error::Projection("ACP tool has no call".into()))?,
            )?),
            "acpMessage" => {
                let parts: Vec<Value> = serde_json::from_value(fields["content"].clone())?;
                Content::AgentParts(
                    parts
                        .iter()
                        .map(|p| decode_part(p, true))
                        .collect::<Result<_, _>>()?,
                )
            }
            "fileChange" => Content::Files(serde_json::from_value(fields)?),
            "imageView" => {
                #[derive(Deserialize)]
                struct Viewed {
                    path: String,
                }
                Content::Image(Image::Local(serde_json::from_value::<Viewed>(fields)?.path))
            }
            "reasoning" => {
                #[derive(Deserialize)]
                struct Reasoning {
                    summary: Vec<String>,
                    content: Vec<String>,
                }
                let reasoning: Reasoning = serde_json::from_value(fields)?;
                let text = reasoning
                    .summary
                    .into_iter()
                    .chain(reasoning.content)
                    .filter(|part| !part.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                if text.is_empty() {
                    return Ok(None);
                }
                Content::Reasoning(text)
            }
            _ => Content::Unknown(fields),
        };
        Ok(Some(content))
    }
}

fn decode_part(value: &Value, mcp: bool) -> Result<Part, Error> {
    #[derive(Deserialize)]
    struct Text {
        text: String,
    }
    #[derive(Deserialize)]
    struct Local {
        path: String,
    }
    #[derive(Deserialize)]
    struct Url {
        url: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Encoded {
        mime_type: String,
        data: String,
    }
    Ok(match value.get("type").and_then(Value::as_str) {
        Some("text") => Part::Text(serde_json::from_value::<Text>(value.clone())?.text),
        Some("localImage") => Part::Image(Image::Local(
            serde_json::from_value::<Local>(value.clone())?.path,
        )),
        Some("image") if mcp => {
            let image: Encoded = serde_json::from_value(value.clone())?;
            Part::Image(Image::Encoded {
                mime_type: image.mime_type,
                data: image.data,
            })
        }
        Some("image") => Part::Image(Image::Url(
            serde_json::from_value::<Url>(value.clone())?.url,
        )),
        Some("resource") => {
            #[derive(Deserialize)]
            struct Embedded {
                resource: Resource,
            }
            let resource = serde_json::from_value::<Embedded>(value.clone())?.resource;
            if resource.text.is_none() && resource.blob.is_none() {
                return Err(Error::Projection(
                    "Embedded resource lacks text or blob contents".into(),
                ));
            }
            Part::Resource(resource)
        }
        Some("resource_link") => Part::ResourceLink(serde_json::from_value(value.clone())?),
        _ => Part::Unknown(value.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_resources_and_links_are_recognized_in_tool_results() {
        let result: McpResult = serde_json::from_value(serde_json::json!({"content":[
            {"type":"resource","resource":{"uri":"test:///notes.txt","text":"  原文\n\n"}},
            {"type":"resource_link","uri":"test:///report.pdf","name":"report.pdf","size":0}
        ]}))
        .unwrap();
        for part in result.parts().unwrap() {
            assert!(
                !matches!(part, Part::Unknown(_)),
                "resource rendered as raw JSON"
            );
        }
    }

    #[test]
    fn resources_keep_exact_payloads_empty_values_and_raw_extensions() {
        let raw = json!({"id":"r","type":"acpMessage","content":[
            {"type":"resource","resource":{"uri":"test:///a","mimeType":"text/plain","text":"  原文\r\n\n","blob":"AA==","_meta":{"x":false}},"annotations":{"priority":0}},
            {"type":"resource","resource":{"uri":"test:///empty","text":""}},
            {"type":"resource_link","uri":"test:///zero","name":"zero","title":"","size":0,"_meta":{"x":42}},
            {"type":"resource","resource":{"uri":"test:///blob","blob":""}}
        ]});
        let item = item(raw.clone());
        let Some(Content::AgentParts(parts)) = item.content().unwrap() else {
            panic!("agent parts")
        };
        let Part::Resource(resource) = &parts[0] else {
            panic!("resource")
        };
        assert_eq!(resource.text.as_deref(), Some("  原文\r\n\n"));
        assert_eq!(resource.blob.as_deref(), Some("AA=="));
        assert!(
            matches!(&parts[1], Part::Resource(resource) if resource.text.as_deref() == Some(""))
        );
        assert!(
            matches!(&parts[2], Part::ResourceLink(link) if link.size == Some(0) && link.title.as_deref() == Some(""))
        );
        assert!(
            matches!(&parts[3], Part::Resource(resource) if resource.blob.as_deref() == Some(""))
        );
        // Item serialization, not the renderer's projection, owns unknown metadata.
        assert!(item.text().unwrap().contains("\"x\": false"));
        assert_eq!(serde_json::to_value(item).unwrap(), raw);
    }

    #[test]
    fn known_resource_errors_remain_explicit_and_future_content_remains_raw() {
        for block in [
            json!({"type":"resource"}),
            json!({"type":"resource","resource":{"uri":"test:///missing"}}),
            json!({"type":"resource","resource":{"uri":"test:///bad","text":false}}),
            json!({"type":"resource_link","uri":"test:///missing-name"}),
            json!({"type":"resource_link","uri":"test:///bad-size","name":"bad","size":"0"}),
        ] {
            let call: ToolCall = serde_json::from_value(json!({"title":"read","status":"completed","content":[{"type":"content","content":block}]})).unwrap();
            assert!(call.parts().is_err());
        }
        let block = json!({"type":"future_resource","resource":{"text":"future"}});
        assert_eq!(decode_part(&block, true).unwrap(), Part::Unknown(block));
    }
    #[test]
    fn tool_metadata_and_empty_titles_keep_original_json_semantics() {
        let tool: ToolCall = serde_json::from_value(serde_json::json!({"title":"","name":"read_file","kind":"read","status":"pending","rawInput":false,"rawOutput":0,"locations":[{"path":"/synthetic/file.rs","line":0}]})).unwrap();
        assert_eq!(tool.display_title(), "read_file");
        assert_eq!(tool.raw_input, Some(serde_json::json!(false)));
        assert_eq!(tool.raw_output, Some(serde_json::json!(0)));
        assert_eq!(tool.locations.unwrap()[0].line, Some(0));
    }
    use serde_json::json;

    fn item(value: Value) -> Item {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn acp_tool_outputs_decode_content_and_keep_unknown_blocks() {
        let tool: ToolCall = serde_json::from_value(json!({"title":"read","status":"completed","content":[{"type":"content","content":{"type":"text","text":"tool output"}},{"type":"content","content":{"type":"image","mimeType":"image/png","data":"synthetic"}},{"type":"terminal","terminalId":"t"}]})).unwrap();
        let parts = tool.parts().unwrap();
        assert_eq!(parts[0], Part::Text("tool output".into()));
        assert!(
            matches!(&parts[1], Part::Image(Image::Encoded {mime_type,..}) if mime_type == "image/png")
        );
        assert!(matches!(&parts[2], Part::Unknown(value) if value["terminalId"] == "t"));
        let bad: ToolCall = serde_json::from_value(
            json!({"title":"bad","status":"completed","content":[{"type":"content"}]}),
        )
        .unwrap();
        assert!(bad.parts().is_err());
    }

    #[test]
    fn preserves_markdown_and_mixed_user_attachments() {
        assert_eq!(
            item(json!({"id":"a","type":"agentMessage","text":"**你好** `x`"}))
                .content()
                .unwrap(),
            Some(Content::Markdown("**你好** `x`".into()))
        );
        let content = item(json!({"id":"u","type":"userMessage","content":[{"type":"text","text":"look"},{"type":"localImage","path":"/example/a.png"},{"type":"image","url":"data:image/png;base64,AA=="},{"type":"future","extra":42}]})).content().unwrap();
        let Some(Content::Message(parts)) = content else {
            panic!("expected message")
        };
        assert_eq!(parts[1], Part::Image(Image::Local("/example/a.png".into())));
        assert!(matches!(&parts[2], Part::Image(Image::Url(url)) if url.starts_with("data:")));
        assert_eq!(parts[3], Part::Unknown(json!({"type":"future","extra":42})));
    }

    #[test]
    fn command_failure_keeps_exit_status_and_multiline_output() {
        let content = item(json!({"id":"c","type":"commandExecution","command":"cargo test","cwd":"/example","status":"completed","exitCode":1,"aggregatedOutput":"first\n失败","durationMs":123})).content().unwrap();
        let Some(Content::Command(command)) = content else {
            panic!("expected command")
        };
        assert_eq!(command.exit_code, Some(1));
        assert_eq!(command.aggregated_output.as_deref(), Some("first\n失败"));
        assert_eq!(command.duration_ms, Some(123));
    }

    #[test]
    fn mcp_content_decodes_images_and_preserves_structured_result() {
        let content = item(json!({"id":"m","type":"mcpToolCall","server":"example","tool":"capture","status":"completed","arguments":{"id":1},"result":{"content":[{"type":"text","text":"done"},{"type":"image","mimeType":"image/png","data":"AA=="}],"structuredContent":{"ok":true},"isError":false}})).content().unwrap();
        let Some(Content::Mcp(call)) = content else {
            panic!("expected mcp call")
        };
        let result = call.result.unwrap();
        assert_eq!(result.structured_content, Some(json!({"ok":true})));
        assert_eq!(
            result.parts().unwrap()[1],
            Part::Image(Image::Encoded {
                mime_type: "image/png".into(),
                data: "AA==".into()
            })
        );
    }

    #[test]
    fn hides_only_empty_reasoning_and_reports_malformed_known_content() {
        assert_eq!(
            item(json!({"id":"r","type":"reasoning","summary":[],"content":[]}))
                .content()
                .unwrap(),
            None
        );
        assert_eq!(
            item(json!({"id":"r","type":"reasoning","summary":["Thinking"],"content":["detail"]}))
                .content()
                .unwrap(),
            Some(Content::Reasoning("Thinking\n\ndetail".into()))
        );
        assert!(
            item(json!({"id":"c","type":"commandExecution","command":42}))
                .content()
                .is_err()
        );
        assert!(
            item(json!({"id":"u","type":"userMessage","content":[{"type":"image"}]}))
                .content()
                .is_err()
        );
        assert!(matches!(
            item(json!({"id":"x","type":"futureItem","extra":42}))
                .content()
                .unwrap(),
            Some(Content::Unknown(_))
        ));
    }

    #[test]
    fn patch_adapter_preserves_kind_paths_hunks_and_final_newline() {
        let add: FileChange = serde_json::from_value(
            json!({"path":"/example/new file","kind":{"type":"add"},"diff":"one\ntwo"}),
        )
        .unwrap();
        assert_eq!(
            add.unified_patch().unwrap(),
            "--- /dev/null\n+++ \"/example/new file\"\n@@ -0,0 +1,2 @@\n+one\n+two\n\\ No newline at end of file\n"
        );
        let delete: FileChange =
            serde_json::from_value(json!({"path":"old","kind":{"type":"delete"},"diff":"old\n"}))
                .unwrap();
        assert!(
            delete
                .unified_patch()
                .unwrap()
                .ends_with("@@ -1,1 +0,0 @@\n-old\n")
        );
        let update: FileChange = serde_json::from_value(json!({"path":"old","kind":{"type":"update","move_path":"new"},"diff":"@@ -1 +1 @@\n-old\n+new\n"})).unwrap();
        assert_eq!(
            update.unified_patch().unwrap(),
            "--- \"old\"\n+++ \"new\"\n@@ -1 +1 @@\n-old\n+new\n"
        );
        let mut annotated = update.clone();
        annotated.diff.push_str("\n\nMoved to: new");
        assert_eq!(
            annotated.unified_patch().unwrap(),
            update.unified_patch().unwrap()
        );
    }
}
