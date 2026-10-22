//! Local presentation state. Restoring it never claims control or starts work.
use crate::{
    client::Error,
    session_selection::Target,
    storage::{StorageError, Store},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub const MAX_TABS: usize = 32;
const MAX_BYTES: usize = 512 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Panel {
    #[default]
    Conversation,
    Files,
    Terminal,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileLocation {
    pub directory: String,
    pub file: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tab {
    pub workspace: Uuid,
    pub socket: PathBuf,
    pub directory: PathBuf,
    pub device: u64,
    pub inode: u64,
    pub session: Option<Target>,
    pub panel: Panel,
    pub files: FileLocation,
    pub terminal: Option<(Uuid, Uuid)>,
    pub sidebar_width: u16,
    pub sidebar_visible: bool,
}

impl Tab {
    pub fn new(workspace: &crate::workspaces::Workspace) -> Self {
        let registration = &workspace.registration;
        Self {
            workspace: workspace.id,
            socket: registration.endpoint.socket.clone(),
            directory: registration.directory.path.clone(),
            device: registration.directory.device,
            inode: registration.directory.inode,
            session: None,
            panel: Panel::Conversation,
            files: FileLocation::default(),
            terminal: None,
            sidebar_width: 320,
            sidebar_visible: true,
        }
    }
    pub fn check(&self, workspace: &crate::workspaces::Workspace) -> Result<(), Error> {
        let current = Self::new(workspace);
        if self.workspace != current.workspace
            || self.socket != current.socket
            || self.directory != current.directory
            || self.device != current.device
            || self.inode != current.inode
        {
            return Err(Error::Input(
                "Saved workspace identity changed; open its current catalog entry explicitly"
                    .into(),
            ));
        }
        Ok(())
    }
    fn validate(&self) -> Result<(), StorageError> {
        crate::hosts::Configuration::single(self.socket.clone())?;
        if self.workspace.is_nil()
            || !self.directory.is_absolute()
            || self.directory.to_str().is_none()
            || self.directory.as_os_str().len() > 4096
            || !(240..=520).contains(&self.sidebar_width)
        {
            return Err("invalid workspace layout identity or sidebar width".into());
        }
        if let Some(session) = &self.session {
            session.validate()?;
            if Path::new(&session.cwd) != self.directory {
                return Err("saved session belongs to another workspace".into());
            }
        }
        for path in std::iter::once(&self.files.directory).chain(self.files.file.iter()) {
            if path.len() > 4096
                || path.contains('\0')
                || Path::new(path)
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err("saved file location must be a relative workspace path".into());
            }
        }
        if self
            .terminal
            .is_some_and(|(host, id)| host.is_nil() || id.is_nil())
        {
            return Err("invalid saved terminal identity".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    pub tabs: Vec<Tab>,
    pub selected: Option<Uuid>,
    pub visible: bool,
}

impl Layout {
    pub fn validate(&self) -> Result<(), StorageError> {
        if self.tabs.len() > MAX_TABS {
            return Err("workspace tabs reached the 32-tab limit".into());
        }
        let mut ids = BTreeSet::new();
        for tab in &self.tabs {
            tab.validate()?;
            if !ids.insert(tab.workspace) {
                return Err("duplicate workspace tab".into());
            }
        }
        if self.selected.is_some_and(|id| !ids.contains(&id))
            || (self.visible && self.selected.is_none())
        {
            return Err("selected workspace tab is absent".into());
        }
        if serde_json::to_vec(self)?.len() > MAX_BYTES {
            return Err("workspace layout exceeds 512 KiB".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Saved {
    pub revision: u64,
    pub layout: Layout,
}

fn read(db: &Connection) -> Result<Saved, StorageError> {
    let row: Option<(i64,Option<String>)> = db.query_row("SELECT revision,CASE WHEN length(CAST(data AS BLOB))<=524288 THEN data ELSE NULL END FROM workspace_layout WHERE singleton=1",[],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let Some((revision, data)) = row else {
        return Ok(Saved::default());
    };
    if revision <= 0 {
        return Err("invalid workspace layout revision".into());
    }
    let layout: Layout = serde_json::from_str(&data.ok_or("workspace layout exceeds 512 KiB")?)?;
    layout.validate()?;
    Ok(Saved {
        revision: u64::try_from(revision)?,
        layout,
    })
}

impl Store {
    pub async fn workspace_layout(&self) -> Result<Saved, Error> {
        self.job(|db| read(db)).await
    }
    pub async fn save_workspace_layout(
        &self,
        expected_revision: u64,
        layout: Layout,
    ) -> Result<Saved, Error> {
        self.job(move |db| {
            layout.validate()?;
            let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current=read(&tx)?;
            if current.revision!=expected_revision {return Err("workspace layout changed in another window; reload before saving".into());}
            let revision=expected_revision.checked_add(1).and_then(|r|i64::try_from(r).ok()).ok_or("workspace layout revision exhausted")?;
            tx.execute("INSERT INTO workspace_layout(singleton,revision,data) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET revision=excluded.revision,data=excluded.data",params![revision,serde_json::to_string(&layout)?])?;
            tx.commit()?;
            Ok(Saved {revision:u64::try_from(revision)?,layout})
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tab() -> Tab {
        Tab {
            workspace: Uuid::new_v4(),
            socket: "/example/host.sock".into(),
            directory: "/example/workspace".into(),
            device: 1,
            inode: 2,
            session: None,
            panel: Panel::Files,
            files: FileLocation {
                directory: "src".into(),
                file: Some("src/main.rs".into()),
            },
            terminal: Some((Uuid::new_v4(), Uuid::new_v4())),
            sidebar_width: 400,
            sidebar_visible: false,
        }
    }
    #[tokio::test]
    async fn reopen_conflict_and_atomic_failure_preserve_exact_layout() {
        let root = std::env::temp_dir().join(format!("fp-layout-{}", Uuid::new_v4()));
        let store = Store::open(root.clone()).await.unwrap();
        let a = tab();
        let mut b = tab();
        b.socket = "/example/other.sock".into();
        let layout = Layout {
            selected: Some(b.workspace),
            tabs: vec![a, b],
            visible: true,
        };
        let saved = store
            .save_workspace_layout(0, layout.clone())
            .await
            .unwrap();
        assert_eq!(
            Store::open(root.clone())
                .await
                .unwrap()
                .workspace_layout()
                .await
                .unwrap(),
            saved
        );
        assert!(
            store
                .save_workspace_layout(0, Layout::default())
                .await
                .is_err()
        );
        store.job(|db|{db.execute_batch("CREATE TRIGGER reject_layout BEFORE UPDATE ON workspace_layout BEGIN SELECT RAISE(ABORT,'injected write failure'); END;")?;Ok(())}).await.unwrap();
        assert!(
            store
                .save_workspace_layout(1, Layout::default())
                .await
                .is_err()
        );
        assert_eq!(store.workspace_layout().await.unwrap().layout, layout);
        store
            .job(|db| {
                db.execute_batch(
                    "DROP TRIGGER reject_layout; UPDATE workspace_layout SET data='corrupt';",
                )?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(store.workspace_layout().await.is_err());
        assert!(
            store
                .save_workspace_layout(1, Layout::default())
                .await
                .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn duplicate_tabs_cross_workspace_sessions_and_escaping_paths_are_rejected() {
        let a = tab();
        assert!(
            Layout {
                tabs: vec![a.clone(), a.clone()],
                ..Layout::default()
            }
            .validate()
            .is_err()
        );
        for path in ["../secret", "/etc/passwd", "a/../../secret"] {
            let mut invalid = a.clone();
            invalid.files.file = Some(path.into());
            assert!(invalid.validate().is_err());
        }
        let mut invalid = a.clone();
        invalid.session = Some(Target {
            host_id: Uuid::new_v4(),
            thread_id: "same-id".into(),
            cwd: "/different".into(),
            archived: false,
        });
        assert!(invalid.validate().is_err());
        let mut invalid = a;
        invalid.sidebar_width = 0;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn catalog_identity_checks_do_not_accept_names_or_replacements() {
        use crate::workspaces::{Directory, Kind, Registration, Workspace};
        let saved = tab();
        let root = Directory {
            path: saved.directory.clone(),
            device: saved.device,
            inode: saved.inode,
        };
        let mut workspace = Workspace {
            position: 1,
            id: saved.workspace,
            project_id: Uuid::new_v4(),
            registration: Registration {
                endpoint: crate::hosts::Endpoint {
                    name: "Renamed display label".into(),
                    socket: saved.socket.clone(),
                },
                directory: root.clone(),
                project: root,
                kind: Kind::Folder,
            },
        };
        saved.check(&workspace).unwrap();
        workspace.registration.directory.inode += 1;
        assert!(saved.check(&workspace).is_err());
        workspace.registration.directory.inode = saved.inode;
        workspace.registration.endpoint.socket = "/example/another.sock".into();
        assert!(saved.check(&workspace).is_err());
        workspace.registration.endpoint.socket = saved.socket.clone();
        workspace.id = Uuid::new_v4();
        assert!(saved.check(&workspace).is_err());
    }

    #[test]
    fn tab_limit_and_selected_identity_are_enforced_before_a_write() {
        let tabs: Vec<_> = (0..MAX_TABS).map(|_| tab()).collect();
        let mut layout = Layout {
            selected: Some(tabs[0].workspace),
            tabs,
            visible: true,
        };
        layout.validate().unwrap();
        layout.tabs.push(tab());
        assert!(layout.validate().is_err());
        layout.tabs.pop();
        layout.selected = Some(Uuid::new_v4());
        assert!(layout.validate().is_err());
        layout.selected = None;
        assert!(layout.validate().is_err());
        layout.visible = false;
        layout.validate().unwrap();
    }

    #[test]
    fn restoration_queries_cannot_admit_provider_mutations() {
        use crate::client::Command;
        for command in [
            Command::Inspect {
                thread_id: "saved".into(),
                resume: false,
            },
            Command::Open {
                thread_id: "saved".into(),
                resume: false,
            },
            Command::List(crate::directory::PageRequest::default()),
        ] {
            assert!(command.is_query().unwrap());
        }
        for command in [
            Command::Inspect {
                thread_id: "saved".into(),
                resume: true,
            },
            Command::Create {
                cwd: "/example".into(),
            },
            Command::Send {
                thread_id: "saved".into(),
                text: "do not send during restoration".into(),
                images: vec![],
                options: Default::default(),
            },
            Command::Stop {
                thread_id: "saved".into(),
                turn_id: "active".into(),
            },
        ] {
            assert!(!command.is_query().unwrap());
        }
    }
    #[tokio::test]
    async fn schema_thirteen_migrates_without_losing_drafts() {
        let root = std::env::temp_dir().join(format!("fp-layout-upgrade-{}", Uuid::new_v4()));
        let store = Store::open(root.clone()).await.unwrap();
        store.job(|db|{db.execute_batch("INSERT INTO drafts VALUES('retained',7,'exact draft'); DROP TABLE notification_preferences; DROP TABLE workspace_layout; PRAGMA user_version=13;")?;Ok(())}).await.unwrap();
        let reopened = Store::open(root.clone()).await.unwrap();
        assert_eq!(reopened.workspace_layout().await.unwrap(), Saved::default());
        reopened
            .job(|db| {
                assert_eq!(
                    db.query_row("SELECT data FROM drafts", [], |r| r.get::<_, String>(0))?,
                    "exact draft"
                );
                Ok(())
            })
            .await
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
