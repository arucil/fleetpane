//! Synthetic two-page daemon for native-window checks; no provider or account.

use serde_json::{Value, json};
use std::{error::Error, io, net::TcpListener, time::Duration};
use tungstenite::Message;

fn main() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    println!("directory fixture: {}", listener.local_addr()?);
    for connection in listener.incoming() {
        let stream = connection?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;
        let mut socket = tungstenite::accept(stream)?;
        let hello: Value = serde_json::from_str(socket.read()?.to_text()?)?;
        if hello["type"] != "hello" {
            return Err(io::Error::other("expected hello").into());
        }
        socket.send(Message::text(
            json!({
                "type": "session", "message": {
                    "type": "status", "payload": {"status": "server_info"}
                }
            })
            .to_string(),
        ))?;
        let request: Value = serde_json::from_str(socket.read()?.to_text()?)?;
        let request = &request["message"];
        if request["type"] != "fetch_agents_request" || request["page"]["limit"] != 200 {
            return Err(io::Error::other("expected bounded directory request").into());
        }
        let start = match request["page"]["cursor"].as_str() {
            None => 0,
            Some("page-2") => 200,
            Some(_) => return Err(io::Error::other("unknown cursor").into()),
        };
        let entries: Vec<_> = (start..start + 200)
            .map(|index| {
                json!({
                    "agent": {
                        "id": format!("fixture-{index:05}"), "provider": "codex",
                        "cwd": "/example/workspace", "title": format!("Fixture session {index:05}"),
                        "status": "idle", "model": null, "updatedAt": "synthetic fixture"
                    }
                })
            })
            .collect();
        socket.send(Message::text(
            json!({
                "type": "session", "message": {
                    "type": "fetch_agents_response", "payload": {
                        "requestId": request["requestId"], "entries": entries,
                        "pageInfo": {"hasMore": start == 0,
                            "nextCursor": if start == 0 { Some("page-2") } else { None }}
                    }
                }
            })
            .to_string(),
        ))?;
        if !matches!(socket.read()?, Message::Close(_)) {
            return Err(io::Error::other("expected client close after one page").into());
        }
        println!("served entries {start}..{}", start + 200);
    }
    Ok(())
}
