//! One bounded directory request per connection. No agent mutations or subscriptions.

use fleetpane_protocol::{DirectoryPage, ServerMessage, SessionMessage};
use std::{
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};
use thiserror::Error;
use tungstenite::{Message, WebSocket, protocol::WebSocketConfig};

const DEADLINE: Duration = Duration::from_secs(8);
const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("network I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("WebSocket: {0}")]
    WebSocket(#[from] tungstenite::Error),
    #[error("invalid daemon message: {0}")]
    Json(#[from] serde_json::Error),
    #[error("daemon rejected directory request: {0}")]
    Remote(String),
    #[error("directory request exceeded its 8 second deadline")]
    Timeout,
    #[error("daemon closed before the directory response")]
    Closed,
    #[error("this prototype supports only loopback daemons")]
    NotLoopback,
    #[error("daemon returned an inconsistent or oversized directory page")]
    InvalidPage,
}

pub fn fetch_directory(
    address: SocketAddr,
    cursor: Option<&str>,
) -> Result<DirectoryPage, ClientError> {
    if !address.ip().is_loopback() {
        return Err(ClientError::NotLoopback);
    }
    let deadline = Instant::now() + DEADLINE;
    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES));
    let (mut socket, _) =
        tungstenite::client::client_with_config(format!("ws://{address}/ws"), stream, Some(config))
            .map_err(|error| match error {
                tungstenite::HandshakeError::Failure(error) => ClientError::WebSocket(error),
                tungstenite::HandshakeError::Interrupted(_) => ClientError::Timeout,
            })?;
    let client_id = format!("fleetpane-{}", uuid::Uuid::new_v4());
    send_json(&mut socket, fleetpane_protocol::hello(&client_id), deadline)?;
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut requested = false;

    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(ClientError::Timeout)?;
        socket.get_ref().set_read_timeout(Some(remaining))?;
        let message = match socket.read().map_err(read_error)? {
            Message::Text(text) => serde_json::from_str::<ServerMessage>(&text)?,
            Message::Close(_) => return Err(ClientError::Closed),
            Message::Ping(_) => {
                socket.flush()?;
                continue;
            }
            _ => continue,
        };
        match message {
            ServerMessage::Session {
                message: SessionMessage::Status { payload },
            } if !requested && payload.status == "server_info" => {
                send_json(
                    &mut socket,
                    fleetpane_protocol::directory_request(&request_id, cursor),
                    deadline,
                )?;
                requested = true;
            }
            ServerMessage::Session {
                message: SessionMessage::FetchAgentsResponse { payload: page },
            } if requested && page.request_id == request_id => {
                if page.entries.len() > 200
                    || page.page_info.has_more != page.page_info.next_cursor.is_some()
                    || page
                        .page_info
                        .next_cursor
                        .as_deref()
                        .is_some_and(str::is_empty)
                {
                    return Err(ClientError::InvalidPage);
                }
                socket.get_ref().set_write_timeout(Some(
                    deadline
                        .checked_duration_since(Instant::now())
                        .ok_or(ClientError::Timeout)?,
                ))?;
                socket.close(None)?;
                return Ok(page);
            }
            ServerMessage::Session {
                message: SessionMessage::RpcError { payload: error },
            } if error.request_id == request_id => return Err(ClientError::Remote(error.error)),
            _ => {}
        }
    }
}

fn send_json(
    socket: &mut WebSocket<TcpStream>,
    message: serde_json::Value,
    deadline: Instant,
) -> Result<(), ClientError> {
    socket.get_ref().set_write_timeout(Some(
        deadline
            .checked_duration_since(Instant::now())
            .ok_or(ClientError::Timeout)?,
    ))?;
    socket.send(Message::Text(serde_json::to_string(&message)?.into()))?;
    Ok(())
}

fn read_error(error: tungstenite::Error) -> ClientError {
    match error {
        tungstenite::Error::Io(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) =>
        {
            ClientError::Timeout
        }
        error => ClientError::WebSocket(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{net::TcpListener, thread};

    fn with_daemon(
        reply: impl FnOnce(String) -> serde_json::Value + Send + 'static,
    ) -> Result<DirectoryPage, ClientError> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let daemon = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            let hello: serde_json::Value =
                serde_json::from_str(ws.read().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(hello["type"], "hello");
            assert_eq!(hello["capabilities"]["explicit_event_subscriptions"], true);
            ws.send(Message::Text(
                json!({"type":"session","message":{"type":"status","payload":{"status":"server_info","serverId":"synthetic-daemon"}}})
                    .to_string()
                    .into(),
            ))
            .unwrap();
            let request: serde_json::Value =
                serde_json::from_str(ws.read().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(request["message"]["type"], "fetch_agents_request");
            assert!(request["message"].get("subscribe").is_none());
            let id = request["message"]["requestId"].as_str().unwrap().to_owned();
            ws.send(Message::Text(reply(id).to_string().into()))
                .unwrap();
            // Keep the transport alive until the client has consumed the response.
            let _closed = ws.read();
        });
        let result = fetch_directory(address, None);
        daemon.join().unwrap();
        result
    }

    #[test]
    fn waits_for_hello_then_reads_a_correlated_directory_page() {
        let page = with_daemon(|id| json!({"type":"session","message":{
            "type":"fetch_agents_response","payload":{"requestId":id,
            "entries":[{"agent":{"id":"synthetic-1","provider":"codex","cwd":"/work/demo",
            "title":"Verify protocol","status":"running","model":null,"updatedAt":"2026-01-01T00:00:00Z"},"project":{},"futureField":true}],
            "pageInfo":{"hasMore":true,"nextCursor":"page-2","prevCursor":null}}
        }})).unwrap();
        assert_eq!(page.entries[0].agent.display_title(), "Verify protocol");
        assert_eq!(page.page_info.next_cursor.as_deref(), Some("page-2"));
    }

    #[test]
    fn surfaces_daemon_rejection() {
        let result = with_daemon(|id| {
            json!({"type":"session","message":{
                "type":"rpc_error","payload":{"requestId":id,"error":"unsupported directory"}
            }})
        });
        assert!(
            matches!(result, Err(ClientError::Remote(error)) if error == "unsupported directory")
        );
    }

    #[test]
    fn rejects_inconsistent_pagination() {
        let result = with_daemon(|id| {
            json!({"type":"session","message":{
                "type":"fetch_agents_response","payload":{"requestId":id,"entries":[],
                "pageInfo":{"hasMore":true,"nextCursor":null}}
            }})
        });
        assert!(matches!(result, Err(ClientError::InvalidPage)));
    }

    #[test]
    fn rejects_remote_addresses_without_connecting() {
        assert!(matches!(
            fetch_directory("192.0.2.1:6767".parse().unwrap(), None),
            Err(ClientError::NotLoopback)
        ));
    }
}
