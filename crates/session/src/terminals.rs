//! Saved-workspace boundary for Host-owned shell creation.
use crate::{
    client::{Completed, Connection, Error},
    workspaces::{Workspace, directory},
};
pub use fleetpane_runtime::terminal::{Info, Mutation, Output, Read};
use uuid::Uuid;

pub mod input;
pub mod screen;

/// Terminal results go to a terminal view, never the conversation projection.
pub enum Command {
    Mutate(Mutation),
    Read(Read),
}

impl Connection {
    pub async fn execute_terminal(
        &self,
        command: &Command,
        operation: Uuid,
    ) -> Result<Completed, Error> {
        let (method, params) = match command {
            Command::Mutate(command) => {
                command.validate().map_err(Error::Input)?;
                (command.method(), serde_json::to_value(command))
            }
            Command::Read(read) => (
                fleetpane_runtime::terminal::READ_METHOD,
                serde_json::to_value(read),
            ),
        };
        self.execute_rpc(
            method,
            params.map_err(|e| Error::Input(e.to_string()))?,
            operation,
        )
        .await
    }
}

impl Workspace {
    pub async fn create_terminal(
        &self,
        connection: &Connection,
        operation: Uuid,
        label: String,
    ) -> Result<Completed, Error> {
        if connection.socket != self.registration.endpoint.socket {
            return Err(Error::Input(
                "workspace belongs to a different Host endpoint".into(),
            ));
        }
        let saved = &self.registration.directory;
        if directory(saved.path.clone()).await? != *saved {
            return Err(Error::Input(
                "saved workspace directory was replaced; shell not started".into(),
            ));
        }
        connection
            .execute_terminal(
                &Command::Mutate(Mutation::Create {
                    workspace_id: self.id,
                    cwd: saved.path.clone(),
                    device: saved.device,
                    inode: saved.inode,
                    label,
                    rows: 24,
                    cols: 80,
                }),
                operation,
            )
            .await
    }
}
