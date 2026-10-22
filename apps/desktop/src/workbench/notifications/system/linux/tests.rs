use super::super::{Category, Popup};
use super::*;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use tokio::sync::mpsc;

struct Bus(Child);
impl Drop for Bus {
    fn drop(&mut self) {
        self.0.kill().expect("stop private test bus");
        self.0.wait().expect("reap private test bus");
    }
}

#[derive(Debug)]
enum Event {
    Refused,
    Shown {
        id: u32,
        replace: u32,
        body: String,
        actions: Vec<String>,
    },
    Closed(u32),
}

type NotifyArgs = (
    String,
    u32,
    String,
    String,
    String,
    Vec<String>,
    HashMap<String, zbus::zvariant::OwnedValue>,
    i32,
);

async fn connection(address: &str) -> Connection {
    zbus::connection::Builder::address(address)
        .unwrap()
        .method_timeout(DEADLINE)
        .build()
        .await
        .unwrap()
}

async fn event(events: &mut mpsc::UnboundedReceiver<Event>) -> Event {
    tokio::time::timeout(DEADLINE, events.recv())
        .await
        .unwrap()
        .unwrap()
}

fn popup(category: Category, message: &str) -> Popup {
    Popup {
        category,
        generation: uuid::Uuid::new_v4(),
        message: message.into(),
    }
}

#[tokio::test]
async fn private_bus_replacement_silence_retraction_actions_and_shutdown() {
    scenario(true, false, false, false).await;
}

#[tokio::test]
async fn private_bus_without_actions_still_replaces_and_retracts() {
    scenario(false, false, false, false).await;
}

#[tokio::test]
async fn private_bus_refused_replacement_reports_failure_and_closes_previous() {
    scenario(true, true, false, false).await;
}

#[tokio::test]
async fn private_bus_server_replacement_never_contacts_new_owner_with_old_ids() {
    scenario(true, false, true, false).await;
}

#[tokio::test]
async fn private_bus_action_immediately_followed_by_close_preserves_wire_order() {
    // Queue both signals while the client is awaiting Notify's reply. Multiple
    // independent streams must not let select reverse their order.
    for _ in 0..16 {
        scenario(true, false, false, true).await;
    }
}

async fn scenario(
    actions_supported: bool,
    refuse_replacement: bool,
    replace_server: bool,
    action_then_close: bool,
) {
    let mut process = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("dbus-daemon is required for this Linux transport test");
    let mut address = String::new();
    BufReader::new(process.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let _bus = Bus(process);
    let address = address.trim();
    let server = connection(address).await;
    server.request_name(SERVICE).await.unwrap();
    let (events, mut received) = mpsc::unbounded_channel();
    let serve_connection = server.clone();
    let fixture = tokio::spawn(async move {
        let mut messages = zbus::MessageStream::from(&serve_connection);
        let mut next_id = 100_u32;
        while let Some(message) = messages.next().await {
            let message = message.unwrap();
            if message.message_type() != zbus::message::Type::MethodCall {
                continue;
            }
            let header = message.header();
            match header.member().unwrap().as_str() {
                "GetCapabilities" => {
                    let capabilities: Vec<&str> = if actions_supported {
                        vec!["actions"]
                    } else {
                        vec![]
                    };
                    serve_connection
                        .reply(&header, &capabilities)
                        .await
                        .unwrap();
                }
                "Notify" => {
                    let (app, replace, icon, title, body, actions, hints, expiry): NotifyArgs =
                        message.body().deserialize().unwrap();
                    assert_eq!(app, "Fleetpane");
                    assert!(icon.is_empty());
                    assert!(title.starts_with("Fleetpane · "));
                    assert!(bool::try_from(hints.get("suppress-sound").unwrap()).unwrap());
                    assert!(bool::try_from(hints.get("transient").unwrap()).unwrap());
                    assert_eq!(expiry, 8000);
                    if refuse_replacement && replace != 0 {
                        serve_connection
                            .reply_error(
                                &header,
                                "org.freedesktop.DBus.Error.Failed",
                                &"owned replacement refusal",
                            )
                            .await
                            .unwrap();
                        events.send(Event::Refused).unwrap();
                        continue;
                    }
                    let id = if replace == 0 {
                        next_id += 1;
                        next_id
                    } else {
                        replace
                    };
                    if action_then_close && replace != 0 {
                        serve_connection
                            .emit_signal(
                                None::<&str>,
                                PATH,
                                SERVICE,
                                "ActionInvoked",
                                &(id, actions[0].as_str()),
                            )
                            .await
                            .unwrap();
                        serve_connection
                            .emit_signal(
                                None::<&str>,
                                PATH,
                                SERVICE,
                                "NotificationClosed",
                                &(id, 2_u32),
                            )
                            .await
                            .unwrap();
                    }
                    serve_connection.reply(&header, &id).await.unwrap();
                    events
                        .send(Event::Shown {
                            id,
                            replace,
                            body,
                            actions,
                        })
                        .unwrap();
                }
                "CloseNotification" => {
                    let (id,): (u32,) = message.body().deserialize().unwrap();
                    serve_connection.reply(&header, &()).await.unwrap();
                    events.send(Event::Closed(id)).unwrap();
                }
                name => panic!("Unexpected notification call: {name}"),
            }
        }
    });
    let client = connection(address).await;
    let (commands, mut desired) = watch::channel(Default::default());
    let (status, mut state) = watch::channel(Status::default());
    let runner = tokio::spawn(async move { serve(&client, &mut desired, &status).await });
    let first = popup(Category::Pending, "2 new requests");
    commands.send_replace([None, Some(first.clone())]);
    let Event::Shown {
        id,
        replace,
        body,
        actions,
    } = event(&mut received).await
    else {
        panic!("show");
    };
    assert_eq!(replace, 0);
    assert_eq!(body, "2 new requests");
    if actions_supported {
        assert_eq!(
            actions,
            [first.generation.to_string(), "Open Attention".into()]
        );
    } else {
        assert!(actions.is_empty());
    }
    if replace_server {
        server.release_name(SERVICE).await.unwrap();
        let replacement = connection(address).await;
        replacement.request_name(SERVICE).await.unwrap();
        let outcome = tokio::time::timeout(DEADLINE, runner)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert!(outcome.contains("Notification service changed"));
        // A wrongly addressed old-ID close would block on the new owner and fail
        // the deadline above. Cleanup is pinned to the original unique owner.
        assert!(matches!(event(&mut received).await, Event::Closed(closed) if closed == id));
        fixture.abort();
        assert!(fixture.await.unwrap_err().is_cancelled());
        return;
    }
    let second = popup(Category::Pending, "3 new requests");
    commands.send_replace([None, Some(second.clone())]);
    if refuse_replacement {
        assert!(matches!(event(&mut received).await, Event::Refused));
        assert!(matches!(event(&mut received).await, Event::Closed(closed) if closed == id));
        let error = tokio::time::timeout(DEADLINE, runner)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert!(error.contains("owned replacement refusal"));
        fixture.abort();
        assert!(fixture.await.unwrap_err().is_cancelled());
        return;
    }
    let Event::Shown {
        id: replaced,
        replace,
        ..
    } = event(&mut received).await
    else {
        panic!("replace");
    };
    assert_eq!((replaced, replace), (id, id));
    if action_then_close {
        tokio::time::timeout(DEADLINE, state.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            state.borrow().activation,
            Some((1, Category::Pending, second.generation))
        );
        drop(commands);
        tokio::time::timeout(DEADLINE, runner)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        fixture.abort();
        assert!(fixture.await.unwrap_err().is_cancelled());
        return;
    }
    if actions_supported {
        server
            .emit_signal(
                None::<&str>,
                PATH,
                SERVICE,
                "ActionInvoked",
                &(id, first.generation.to_string()),
            )
            .await
            .unwrap();
        server
            .emit_signal(
                None::<&str>,
                PATH,
                SERVICE,
                "ActionInvoked",
                &(id, second.generation.to_string()),
            )
            .await
            .unwrap();
        tokio::time::timeout(DEADLINE, state.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            state.borrow().activation,
            Some((1, Category::Pending, second.generation))
        );
    }
    commands.send_replace(Default::default());
    assert!(matches!(event(&mut received).await, Event::Closed(closed) if closed == id));
    let third = popup(Category::Results, "1 new turn result");
    commands.send_replace([Some(third), None]);
    let Event::Shown {
        id: result,
        replace,
        ..
    } = event(&mut received).await
    else {
        panic!("new result");
    };
    assert_eq!(replace, 0);
    drop(commands);
    assert!(matches!(event(&mut received).await, Event::Closed(closed) if closed == result));
    tokio::time::timeout(DEADLINE, runner)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    fixture.abort();
    assert!(fixture.await.unwrap_err().is_cancelled());
}
