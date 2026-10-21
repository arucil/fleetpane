//! Window-local, bounded navigation metadata. File contents are always re-read.
use fleetpane_session::workspaces::{Directory, Workspace};
use std::{collections::VecDeque, path::PathBuf};
use uuid::Uuid;

const MAX_LOCATIONS: usize = 32;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Location {
    pub directory: String,
    pub file: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct Key {
    workspace: Uuid,
    socket: PathBuf,
    directory: Directory,
}

impl From<&Workspace> for Key {
    fn from(workspace: &Workspace) -> Self {
        Self {
            workspace: workspace.id,
            socket: workspace.registration.endpoint.socket.clone(),
            directory: workspace.registration.directory.clone(),
        }
    }
}

#[derive(Default)]
pub struct Locations(VecDeque<(Key, Location)>);

impl Locations {
    pub fn get(&mut self, workspace: &Workspace) -> Location {
        let key = Key::from(workspace);
        let Some(index) = self.0.iter().position(|(saved, _)| saved == &key) else {
            return Location::default();
        };
        let entry = self.0.remove(index).expect("located entry");
        let location = entry.1.clone();
        self.0.push_back(entry);
        location
    }

    pub fn save(&mut self, workspace: &Workspace, location: Location) {
        let key = Key::from(workspace);
        // A replaced workspace identity must not resurrect a previous location.
        self.0.retain(|(saved, _)| saved.workspace != key.workspace);
        if self.0.len() == MAX_LOCATIONS {
            self.0.pop_front();
        }
        self.0.push_back((key, location));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleetpane_session::{
        hosts::Endpoint,
        workspaces::{Kind, Registration},
    };

    fn workspace(id: u128) -> Workspace {
        let directory = Directory {
            path: "/fixture".into(),
            device: 1,
            inode: 2,
        };
        Workspace {
            id: Uuid::from_u128(id),
            position: 1,
            project_id: Uuid::from_u128(99),
            registration: Registration {
                endpoint: Endpoint {
                    name: "Fixture".into(),
                    socket: "/fixture.sock".into(),
                },
                directory: directory.clone(),
                project: directory,
                kind: Kind::Folder,
            },
        }
    }
    fn location() -> Location {
        Location {
            directory: "目录".into(),
            file: Some("目录/原文.txt".into()),
        }
    }

    #[test]
    fn restores_selection_and_isolates_workspace_host_and_root_identity() {
        let mut saved = Locations::default();
        let original = workspace(1);
        saved.save(&original, location());
        assert_eq!(saved.get(&original), location());
        assert_eq!(saved.get(&workspace(2)), Location::default());
        let mut changed = original.clone();
        changed.registration.endpoint.socket = "/other.sock".into();
        assert_eq!(saved.get(&changed), Location::default());
        changed = original.clone();
        changed.registration.directory.inode += 1;
        assert_eq!(saved.get(&changed), Location::default());
        saved.save(&changed, Location::default());
        assert_eq!(saved.get(&original), Location::default());
        assert_eq!(saved.0.len(), 1);
    }

    #[test]
    fn recently_opened_workspace_survives_bounded_eviction_and_updates_in_place() {
        let mut saved = Locations::default();
        for id in 1..=MAX_LOCATIONS {
            saved.save(&workspace(id as u128), location());
        }
        assert_eq!(saved.get(&workspace(1)), location());
        saved.save(&workspace(100), location());
        assert_eq!(saved.0.len(), MAX_LOCATIONS);
        assert_eq!(saved.get(&workspace(2)), Location::default());
        assert_eq!(saved.get(&workspace(1)), location());
        saved.save(&workspace(1), Location::default());
        assert_eq!(saved.get(&workspace(1)), Location::default());
        assert_eq!(saved.0.len(), MAX_LOCATIONS);
    }
}
