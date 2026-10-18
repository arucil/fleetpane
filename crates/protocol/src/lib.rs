//! A deliberately small, read-only projection of Paseo's JSON protocol.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: String,
    pub provider: String,
    pub cwd: String,
    pub title: Option<String>,
    pub status: String,
    pub model: Option<String>,
    pub updated_at: String,
}

impl Agent {
    pub fn display_title(&self) -> &str {
        self.title.as_deref().unwrap_or(&self.id)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DirectoryEntry {
    pub agent: Agent,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryPage {
    pub request_id: String,
    pub entries: Vec<DirectoryEntry>,
    pub page_info: PageInfo,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcError {
    pub request_id: String,
    pub error: String,
}

#[derive(Debug, Deserialize)]
pub struct Status {
    pub status: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionMessage {
    Status {
        payload: Status,
    },
    FetchAgentsResponse {
        payload: DirectoryPage,
    },
    RpcError {
        payload: RpcError,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Session {
        message: SessionMessage,
    },
    #[serde(other)]
    Other,
}

pub fn hello(client_id: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "hello", "clientId": client_id, "clientType": "cli", "protocolVersion": 1,
        "capabilities": {
            "all_providers": true,
            "explicit_event_subscriptions": true,
            "selective_agent_timeline": true
        }
    })
}

pub fn directory_request(request_id: &str, cursor: Option<&str>) -> serde_json::Value {
    let mut page = serde_json::json!({"limit": 200});
    if let Some(cursor) = cursor {
        page["cursor"] = serde_json::Value::String(cursor.to_owned());
    }
    serde_json::json!({
        "type": "session",
        "message": {
            "type": "fetch_agents_request", "requestId": request_id, "scope": "active",
            "page": page,
            "sort": [{"key": "updated_at", "direction": "desc"}]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn future_notifications_do_not_break_the_directory_reader() {
        let message: ServerMessage = serde_json::from_str(
            r#"{"type":"session","message":{"type":"future_notification","payload":{"new":true}}}"#,
        )
        .unwrap();
        assert!(matches!(
            message,
            ServerMessage::Session {
                message: SessionMessage::Other
            }
        ));
    }

    #[test]
    fn malformed_known_response_is_an_error() {
        assert!(serde_json::from_str::<ServerMessage>(
            r#"{"type":"session","message":{"type":"fetch_agents_response","payload":{"entries":[]}}}"#,
        ).is_err());
    }

    #[test]
    fn server_info_is_a_status_payload_not_a_message_type() {
        let message: ServerMessage = serde_json::from_str(
            r#"{"type":"session","message":{"type":"status","payload":{"status":"server_info","serverId":"synthetic-daemon"}}}"#,
        ).unwrap();
        assert!(matches!(message, ServerMessage::Session {
            message: SessionMessage::Status { payload: Status { status } }
        } if status == "server_info"));
    }

    #[test]
    fn directory_request_is_paged_without_live_or_timeline_subscriptions() {
        let request = directory_request("test", Some("next"));
        assert_eq!(request["message"]["page"]["cursor"], "next");
        assert_eq!(request["message"]["page"]["limit"], 200);
        assert!(request["message"].get("subscribe").is_none());
        assert_eq!(
            hello("test")["capabilities"]["selective_agent_timeline"],
            true
        );
    }
}
