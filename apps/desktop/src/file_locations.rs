//! Window-local, bounded navigation metadata. File contents are always re-read.
use fleetpane_session::workspaces::{Directory, Workspace};
use std::{collections::VecDeque, num::NonZeroUsize, path::PathBuf};
use uuid::Uuid;

const MAX_LOCATIONS: usize = 32;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Location {
    pub directory: String,
    pub file: Option<String>,
}

/// Paths only: revisiting a location always observes the filesystem again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Visit {
    pub location: Location,
    pub line: Option<NonZeroUsize>,
    pub top: Option<String>,
}

#[derive(Default)]
pub struct History {
    visits: VecDeque<Visit>,
    cursor: Option<usize>,
}

impl History {
    pub fn destination(&self, forward: bool) -> Option<(usize, Visit)> {
        let cursor = self.cursor?;
        let index = if forward {
            cursor.checked_add(1)?
        } else {
            cursor.checked_sub(1)?
        };
        self.visits.get(index).cloned().map(|visit| (index, visit))
    }

    pub fn remember_scroll(
        &mut self,
        location: &Location,
        line: Option<NonZeroUsize>,
        top: Option<String>,
    ) {
        if let Some(cursor) = self.cursor {
            let current = &mut self.visits[cursor];
            if &current.location == location && current.line == line {
                current.top = top;
            }
        }
    }

    /// Called only after the entire read succeeds. Failed visits never move the cursor.
    pub fn accept(&mut self, visit: Visit, destination: Option<usize>) {
        if let Some(index) = destination {
            self.visits[index] = visit;
            self.cursor = Some(index);
            return;
        }
        if let Some(cursor) = self.cursor {
            let current = &self.visits[cursor];
            if current.location == visit.location && current.line == visit.line {
                self.visits[cursor] = visit;
                return;
            }
            self.visits.truncate(cursor + 1);
        }
        if self.visits.len() == MAX_LOCATIONS {
            self.visits.pop_front();
        }
        self.visits.push_back(visit);
        self.cursor = Some(self.visits.len() - 1);
    }
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
            name: Default::default(),
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

    fn visit(file: &str) -> Visit {
        Visit {
            location: Location {
                directory: "目录".into(),
                file: Some(format!("目录/{file}")),
            },
            line: None,
            top: None,
        }
    }

    #[test]
    fn history_commits_only_success_and_preserves_forward_on_refresh() {
        let mut history = History::default();
        history.accept(visit("a"), None);
        history.remember_scroll(&visit("a").location, None, Some("目录/anchor".into()));
        history.accept(visit("b"), None);
        let (index, previous) = history.destination(false).unwrap();
        assert_eq!(previous.top.as_deref(), Some("目录/anchor"));
        history.remember_scroll(&Location::default(), None, Some("failed-target".into()));
        assert_eq!(history.visits[1].top, None);
        // Preparing a destination, including a failed read, changes no history.
        assert_eq!(history.cursor, Some(1));
        assert!(history.destination(true).is_none());
        history.accept(previous.clone(), Some(index));
        history.accept(previous, None);
        assert_eq!(history.visits.len(), 2);
        assert_eq!(history.destination(true).unwrap().1, visit("b"));
        history.accept(visit("c"), None);
        assert!(history.destination(true).is_none());
        assert_eq!(history.visits.len(), 2);
        assert_eq!(
            history.destination(false).unwrap().1.location,
            visit("a").location
        );
    }

    #[test]
    fn history_keeps_line_identity_and_bounds_only_metadata() {
        let mut history = History::default();
        for line in 1..=MAX_LOCATIONS + 3 {
            let mut next = visit("same.txt");
            next.line = NonZeroUsize::new(line);
            history.accept(next, None);
        }
        assert_eq!(history.visits.len(), MAX_LOCATIONS);
        for _ in 1..MAX_LOCATIONS {
            let (index, previous) = history.destination(false).unwrap();
            history.accept(previous, Some(index));
        }
        assert!(history.destination(false).is_none());
        assert_eq!(history.visits[0].line.unwrap().get(), 4);
        assert!(History::default().destination(true).is_none());
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
