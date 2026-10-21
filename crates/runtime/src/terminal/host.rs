use super::*;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    os::unix::{fs::MetadataExt, process::ExitStatusExt},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, oneshot},
    task::{JoinHandle, JoinSet},
};

struct Retained {
    info: Info,
    bytes: VecDeque<u8>,
    end: u64,
}
impl Retained {
    fn append(&mut self, bytes: &[u8]) {
        self.end = self
            .end
            .checked_add(bytes.len() as u64)
            .expect("PTY byte cursor exhausted");
        self.bytes.extend(bytes);
        if self.bytes.len() > MAX_OUTPUT {
            self.bytes.drain(..self.bytes.len() - MAX_OUTPUT);
        }
    }
    fn output(&self, after: u64) -> Result<Output, String> {
        if after > self.end {
            return Err("output cursor is ahead of this terminal".into());
        }
        let retained = self.end - self.bytes.len() as u64;
        let start = after.max(retained);
        Ok(Output {
            terminal: self.info.clone(),
            start,
            end: self.end,
            dropped: start - after,
            bytes: self
                .bytes
                .iter()
                .skip((start - retained) as usize)
                .copied()
                .collect(),
        })
    }
}
type Reply = oneshot::Sender<Result<Value, String>>;
struct Control {
    command: Mutation,
    reply: Reply,
}
struct Terminal {
    retained: Arc<Mutex<Retained>>,
    control: mpsc::Sender<Control>,
    task: JoinHandle<()>,
}
impl Drop for Terminal {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Default)]
pub(crate) struct Manager {
    terminals: BTreeMap<Uuid, Terminal>,
    pending: JoinSet<Value>,
}
impl Manager {
    pub fn live(&self) -> Vec<Uuid> {
        self.terminals
            .values()
            .filter_map(|terminal| {
                let state = terminal.retained.lock().expect("terminal state poisoned");
                (!state.info.closed && (state.info.exit.is_none() || !state.info.output_eof))
                    .then_some(state.info.id)
            })
            .collect()
    }
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
    pub async fn next(&mut self) -> Option<Result<Value, tokio::task::JoinError>> {
        self.pending.join_next().await
    }
    pub fn read(&self, read: Read) -> Result<Value, String> {
        match read {
            Read::List { workspace_id } => Ok(json!(
                self.terminals
                    .values()
                    .filter_map(|t| {
                        let retained = t.retained.lock().expect("terminal state poisoned");
                        (retained.info.workspace_id == workspace_id).then(|| retained.info.clone())
                    })
                    .collect::<Vec<_>>()
            )),
            Read::Output {
                workspace_id,
                id,
                after,
            } => {
                let terminal = self.find(workspace_id, id)?;
                let retained = terminal.retained.lock().expect("terminal state poisoned");
                Ok(json!(retained.output(after)?))
            }
        }
    }
    fn find(&self, workspace: Uuid, id: Uuid) -> Result<&Terminal, String> {
        self.terminals
            .get(&id)
            .filter(|t| {
                t.retained
                    .lock()
                    .expect("terminal state poisoned")
                    .info
                    .workspace_id
                    == workspace
            })
            .ok_or_else(|| "terminal does not belong to this Host and workspace".into())
    }
    // Host has already committed the durable mutation identity. Completion is
    // deferred so a blocked terminal can never block provider output or snapshots.
    pub fn dispatch(&mut self, id: Uuid, method: &str, params: Value) {
        let result = serde_json::from_value::<Mutation>(params)
            .map_err(|e| e.to_string())
            .and_then(|command| {
                if command.method() != method {
                    return Err("terminal action does not match its RPC method".into());
                }
                command.validate()?;
                self.start(command)
            });
        self.pending.spawn(async move {
            let result = match result {
                Ok(receiver) => match receiver.await {
                    Ok(result) => result,
                    Err(e) => Err(format!("terminal worker stopped; outcome unknown: {e}")),
                },
                Err(error) => Err(error),
            };
            match result {
                Ok(value) => json!({"id":id.to_string(),"result":value}),
                Err(error) => json!({"id":id.to_string(),"error":{"code":-32000,"message":error}}),
            }
        });
    }
    fn start(
        &mut self,
        command: Mutation,
    ) -> Result<oneshot::Receiver<Result<Value, String>>, String> {
        let (reply, receiver) = oneshot::channel();
        if let Mutation::Create {
            workspace_id,
            cwd,
            device,
            inode,
            label,
            rows,
            cols,
        } = command
        {
            self.terminals.retain(|_, t| {
                !t.retained
                    .lock()
                    .expect("terminal state poisoned")
                    .info
                    .closed
            });
            if self.terminals.len() >= MAX_TERMINALS {
                return Err("Host terminal limit reached (16); close a terminal first".into());
            }
            let cwd = std::fs::canonicalize(cwd).map_err(|e| format!("terminal directory: {e}"))?;
            let directory = std::fs::File::from(
                rustix::fs::open(
                    &cwd,
                    rustix::fs::OFlags::RDONLY
                        | rustix::fs::OFlags::DIRECTORY
                        | rustix::fs::OFlags::NOFOLLOW
                        | rustix::fs::OFlags::CLOEXEC,
                    rustix::fs::Mode::empty(),
                )
                .map_err(|e| format!("terminal directory: {e}"))?,
            );
            let metadata = directory.metadata().map_err(|e| e.to_string())?;
            if !metadata.is_dir() {
                return Err("terminal cwd is not a directory".into());
            }
            if metadata.dev() != device || metadata.ino() != inode {
                return Err("saved workspace directory was replaced; shell not started".into());
            }
            // Pin the validated inode through chdir, including a rename racing
            // with spawn. macOS uses the saved path and is not runtime accepted yet.
            #[cfg(target_os = "linux")]
            let spawn_cwd = {
                use std::os::fd::AsRawFd;
                PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()))
            };
            #[cfg(not(target_os = "linux"))]
            let spawn_cwd = cwd.clone();
            let (pty, pts) = pty_process::open().map_err(|e| format!("allocate PTY: {e}"))?;
            pty.resize(pty_process::Size::new(rows, cols))
                .map_err(|e| format!("PTY size: {e}"))?;
            // An explicit POSIX shell keeps startup reproducible and avoids reading
            // interactive user configuration in this first engine slice.
            let child = pty_process::Command::new("/bin/sh")
                .arg("-i")
                .env_remove("ENV")
                .env_remove("BASH_ENV")
                .env("TERM", "xterm-256color")
                .current_dir(&spawn_cwd)
                .kill_on_drop(true)
                .spawn(pts)
                .map_err(|e| format!("start terminal shell: {e}"))?;
            let id = Uuid::new_v4();
            let info = Info {
                id,
                workspace_id,
                cwd,
                label,
                pid: child.id().ok_or("terminal shell has no PID")?,
                rows,
                cols,
                next_input: 0,
                input_fault: None,
                exit: None,
                output_eof: false,
                output_fault: None,
                closed: false,
            };
            let retained = Arc::new(Mutex::new(Retained {
                info: info.clone(),
                bytes: VecDeque::new(),
                end: 0,
            }));
            let (control, controls) = mpsc::channel(4);
            let task = tokio::spawn(serve(pty, child, controls, retained.clone()));
            self.terminals.insert(
                id,
                Terminal {
                    retained,
                    control,
                    task,
                },
            );
            let _disconnected = reply.send(Ok(json!(info)));
        } else {
            let (workspace_id, id) = match &command {
                Mutation::Input {
                    workspace_id, id, ..
                }
                | Mutation::Resize {
                    workspace_id, id, ..
                }
                | Mutation::Rename {
                    workspace_id, id, ..
                }
                | Mutation::Close { workspace_id, id } => (*workspace_id, *id),
                Mutation::Create { .. } => unreachable!(),
            };
            self.find(workspace_id, id)?
                .control
                .try_send(Control { command, reply })
                .map_err(|e| format!("terminal busy or closed; command not queued: {e}"))?;
        }
        Ok(receiver)
    }
}

async fn serve(
    mut pty: pty_process::Pty,
    mut child: tokio::process::Child,
    mut controls: mpsc::Receiver<Control>,
    retained: Arc<Mutex<Retained>>,
) {
    let mut buffer = [0; 8192];
    let mut exited = false;
    let mut eof = false;
    loop {
        tokio::select! {
            status = child.wait(), if !exited => {
                let mut state = retained.lock().expect("terminal state poisoned");
                match status {
                    Ok(status) => state.info.exit = Some(Exit { code: status.code(), signal: status.signal() }),
                    Err(error) => state.info.output_fault = Some(format!("wait for shell: {error}")),
                }
                exited = true;
            }
            read = pty.read(&mut buffer), if !eof => {
                let mut state = retained.lock().expect("terminal state poisoned");
                match read {
                    Ok(0) => { eof = true; state.info.output_eof = true; }
                    Ok(count) => state.append(&buffer[..count]),
                    // Linux PTYs report EIO when the final slave is closed.
                    Err(error) if error.raw_os_error() == Some(rustix::io::Errno::IO.raw_os_error()) => { eof = true; state.info.output_eof = true; }
                    Err(error) => { eof = true; state.info.output_fault = Some(format!("PTY read: {error}")); }
                }
            }
            control = controls.recv() => {
                let Some(Control { command, reply }) = control else { break; };
                let closing = matches!(command, Mutation::Close {..});
                let result = apply(command, &mut pty, &mut child, &retained, exited).await;
                let _disconnected = reply.send(result);
                if closing && retained.lock().expect("terminal state poisoned").info.closed { break; }
            }
        }
    }
}

async fn apply(
    command: Mutation,
    pty: &mut pty_process::Pty,
    child: &mut tokio::process::Child,
    retained: &Arc<Mutex<Retained>>,
    exited: bool,
) -> Result<Value, String> {
    match command {
        Mutation::Input {
            expected_input,
            bytes,
            ..
        } => {
            {
                let mut state = retained.lock().expect("terminal state poisoned");
                if exited || state.info.input_fault.is_some() || state.info.closed {
                    return Err("terminal is exited, closed or input outcome is uncertain; do not replay input".into());
                }
                if expected_input != state.info.next_input {
                    return Err(
                        "input sequence changed; read terminal state, do not replay input".into(),
                    );
                }
                state.info.next_input = state
                    .info
                    .next_input
                    .checked_add(1)
                    .ok_or("input sequence exhausted")?;
            }
            let outcome = tokio::time::timeout(Duration::from_secs(1), pty.write_all(&bytes)).await;
            let reason = match outcome {
                Ok(Ok(())) => None,
                Ok(Err(e)) => Some(e.to_string()),
                Err(e) => Some(e.to_string()),
            };
            if let Some(reason) = reason {
                let message = format!("input may be partially written; do not replay: {reason}");
                retained
                    .lock()
                    .expect("terminal state poisoned")
                    .info
                    .input_fault = Some(message.clone());
                return Err(message);
            }
        }
        Mutation::Resize { rows, cols, .. } => {
            if exited {
                return Err("terminal shell has exited".into());
            }
            pty.resize(pty_process::Size::new(rows, cols))
                .map_err(|e| e.to_string())?;
            let mut state = retained.lock().expect("terminal state poisoned");
            state.info.rows = rows;
            state.info.cols = cols;
        }
        Mutation::Rename { label, .. } => {
            retained.lock().expect("terminal state poisoned").info.label = label
        }
        Mutation::Close { .. } => {
            if !exited {
                // Job-control shells put the current command in a separate group.
                // Target only this PTY's foreground job and its owned shell.
                let foreground = rustix::termios::tcgetpgrp(&*pty)
                    .map_err(|e| format!("terminal foreground group: {e}"))?;
                match rustix::process::kill_process_group(foreground, rustix::process::Signal::KILL)
                {
                    Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                    Err(error) => return Err(format!("close terminal foreground job: {error}")),
                }
                child
                    .kill()
                    .await
                    .map_err(|e| format!("close shell: {e}"))?;
                let status = child.wait().await.map_err(|e| format!("reap shell: {e}"))?;
                retained.lock().expect("terminal state poisoned").info.exit = Some(Exit {
                    code: status.code(),
                    signal: status.signal(),
                });
            }
            retained
                .lock()
                .expect("terminal state poisoned")
                .info
                .closed = true;
        }
        Mutation::Create { .. } => unreachable!(),
    }
    Ok(json!(
        retained.lock().expect("terminal state poisoned").info
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_cursor_reports_loss_and_preserves_non_utf8_and_whitespace() {
        let mut state = Retained {
            info: Info {
                id: Uuid::new_v4(),
                workspace_id: Uuid::new_v4(),
                cwd: "/tmp".into(),
                label: "test".into(),
                pid: 1,
                rows: 24,
                cols: 80,
                next_input: 0,
                input_fault: None,
                exit: None,
                output_eof: false,
                output_fault: None,
                closed: false,
            },
            bytes: VecDeque::new(),
            end: 0,
        };
        state.append(&vec![b'x'; MAX_OUTPUT - 2]);
        state.append(&[0xff, 0, b' ', b'\n']);
        let output = state.output(0).unwrap();
        assert_eq!(
            (output.start, output.end, output.dropped),
            (2, MAX_OUTPUT as u64 + 2, 2)
        );
        assert_eq!(&output.bytes[MAX_OUTPUT - 4..], &[0xff, 0, b' ', b'\n']);
        assert_eq!(
            state.output(MAX_OUTPUT as u64).unwrap().bytes,
            vec![b' ', b'\n']
        );
        assert!(state.output(state.end + 1).is_err());
        assert!(state.output(state.end).unwrap().bytes.is_empty());
    }
}
