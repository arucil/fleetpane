use super::{Desired, Status};
use futures_lite::StreamExt;
use std::{collections::HashMap, time::Duration};
use tokio::sync::watch;
use zbus::{Connection, Proxy, zvariant::Value};

const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const DEADLINE: Duration = Duration::from_secs(3);

pub(super) struct Worker {
    pub commands: Option<watch::Sender<Desired>>,
    pub status: watch::Receiver<Status>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Worker {
    pub fn start() -> Self {
        let (commands, receiver) = watch::channel(Default::default());
        let (sender, status) = watch::channel(Status::default());
        let errors = sender.clone();
        let started = std::thread::Builder::new()
            .name("fleetpane-notifications".into())
            .spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string())
                    .and_then(|runtime| runtime.block_on(run(receiver, &sender)));
                if let Err(error) = result {
                    sender.send_modify(|s| s.error = Some(error));
                }
            });
        let thread = match started {
            Ok(thread) => Some(thread),
            Err(error) => {
                errors.send_modify(|s| {
                    s.error = Some(format!("Cannot start notification worker: {error}"))
                });
                None
            }
        };
        Self {
            commands: Some(commands),
            status,
            thread,
        }
    }
    pub fn finished(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(|thread| thread.is_finished())
    }
}

async fn run(
    mut desired: watch::Receiver<Desired>,
    status: &watch::Sender<Status>,
) -> Result<(), String> {
    let connection = tokio::time::timeout(DEADLINE, async {
        zbus::connection::Builder::session()?
            .method_timeout(DEADLINE)
            .max_queued(16)
            .build()
            .await
    })
    .await
    .map_err(|_| "Notification bus connection timed out".to_string())?
    .map_err(|e| format!("Notification bus unavailable: {e}"))?;
    serve(&connection, &mut desired, status).await
}

async fn serve(
    connection: &Connection,
    desired: &mut watch::Receiver<Desired>,
    status: &watch::Sender<Status>,
) -> Result<(), String> {
    // Pin the unique server owner. Recycled IDs after a server restart must never
    // close another application's notifications or accept another server's actions.
    let bus = zbus::fdo::DBusProxy::new(connection)
        .await
        .map_err(|e| e.to_string())?;
    let owner = bus
        .get_name_owner(
            SERVICE
                .try_into()
                .map_err(|e: zbus::names::Error| e.to_string())?,
        )
        .await
        .map_err(|e| format!("Notification service unavailable: {e}"))?;
    let mut owners = bus
        .receive_name_owner_changed_with_args(&[(0, SERVICE)])
        .await
        .map_err(|e| e.to_string())?;
    let proxy = Proxy::new(connection, owner, PATH, SERVICE)
        .await
        .map_err(|e| e.to_string())?;
    let capabilities: Vec<String> = proxy
        .call("GetCapabilities", &())
        .await
        .map_err(|e| e.to_string())?;
    let actions_supported = capabilities.iter().any(|s| s == "actions");
    // ActionInvoked is commonly followed immediately by NotificationClosed.
    // Keep one stream so select cannot reverse those messages and lose a click.
    let mut signals = proxy
        .receive_all_signals()
        .await
        .map_err(|e| e.to_string())?;
    let mut handles = [None, None];
    let mut applied: Desired = Default::default();
    let mut serial = 0_u64;
    let outcome = loop {
        tokio::select! {
            _owner = owners.next() => {
                break Err("Notification service changed or disconnected; reconnect for future events".into());
            }
            changed = desired.changed() => {
                if changed.is_err() { break Ok(()); }
                let next = desired.borrow_and_update().clone();
                if let Err(error) = apply(&proxy, &next, &applied, &mut handles, actions_supported).await {
                    break Err(error);
                }
                applied = next;
            }
            message = signals.next() => {
                let Some(message) = message else { break Err("Notification signal stream ended".into()); };
                match message.header().member().map(|member|member.as_str()) {
                Some("ActionInvoked") => {
                let (id, action): (u32, String) = match message.body().deserialize() {
                    Ok(body) => body,
                    Err(error) => break Err(format!("Invalid notification action: {error}")),
                };
                let latest = desired.borrow().clone();
                for (index, handle) in handles.iter().enumerate() {
                    if *handle == Some(id) && applied[index] == latest[index]
                        && let Some(popup) = &applied[index]
                        && action == popup.generation.to_string()
                    {
                        serial += 1;
                        status.send_modify(|s| s.activation = Some((serial, popup.category, popup.generation)));
                    }
                }
                }
                Some("NotificationClosed") => {
                let (id, _reason): (u32, u32) = match message.body().deserialize() {
                    Ok(body) => body,
                    Err(error) => break Err(format!("Invalid notification close: {error}")),
                };
                for handle in &mut handles { if *handle == Some(id) { *handle = None; } }
                }
                _ => {}
                }
            }
        }
    };
    let cleanup = apply(
        &proxy,
        &Default::default(),
        &applied,
        &mut handles,
        actions_supported,
    )
    .await;
    match (outcome, cleanup) {
        (Err(error), Err(cleanup)) => {
            Err(format!("{error}; notification cleanup failed: {cleanup}"))
        }
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

async fn apply(
    proxy: &Proxy<'_>,
    next: &Desired,
    applied: &Desired,
    handles: &mut [Option<u32>; 2],
    actions_supported: bool,
) -> Result<(), String> {
    let mut errors = Vec::new();
    for (index, popup) in next.iter().enumerate() {
        if popup == &applied[index] && !(popup.is_none() && handles[index].is_some()) {
            continue;
        }
        let result = if let Some(popup) = popup {
            let mut hints = HashMap::new();
            hints.insert("suppress-sound", Value::from(true));
            hints.insert("transient", Value::from(true));
            let action_id = popup.generation.to_string();
            let actions: Vec<&str> = if actions_supported {
                vec![&action_id, "Open Attention"]
            } else {
                vec![]
            };
            let replace = handles[index].unwrap_or(0);
            let result: zbus::Result<u32> = proxy
                .call(
                    "Notify",
                    &(
                        "Fleetpane",
                        replace,
                        "",
                        popup.category.title(),
                        &popup.message,
                        actions,
                        hints,
                        8000_i32,
                    ),
                )
                .await;
            result.and_then(|id| {
                if id == 0 {
                    return Err(zbus::Error::Failure(
                        "Notification server returned ID zero".into(),
                    ));
                }
                handles[index] = Some(id);
                Ok(())
            })
        } else if let Some(id) = handles[index] {
            let result: zbus::Result<()> = proxy.call("CloseNotification", &(id,)).await;
            if result.is_ok() {
                handles[index] = None;
            }
            result
        } else {
            Ok(())
        };
        if let Err(error) = result {
            errors.push(error.to_string());
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

#[cfg(test)]
mod tests;
