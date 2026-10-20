//! Read-only, demand-loaded, single-file Git comparisons. No provider/Host RPC.
use crate::{
    client::Error,
    workspaces::{Change, Workspace, git_observation},
};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Staged,
    Unstaged,
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Staged => "Staged · HEAD → index",
            Self::Unstaged => "Unstaged · index → working tree",
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Content {
    Patch { text: String },
    Binary { summary: String },
    Empty,
    Unavailable { reason: String },
}

#[derive(Debug, Serialize)]
pub struct FileDiff {
    pub workspace_id: Uuid,
    pub scope: Scope,
    /// Paths are relative to the worktree root, including for saved subdirectories.
    pub change: Change,
    pub head: Option<String>,
    pub content: Content,
}

fn invalid(error: impl std::fmt::Display) -> Error {
    Error::Input(format!("Git diff: {error}"))
}

fn literal(path: &str) -> Result<String, Error> {
    if path.is_empty()
        || path.contains('\0')
        || !Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(invalid(
            "choose a current changed path relative to the worktree root",
        ));
    }
    Ok(format!(":(top,literal){path}"))
}

fn classify(bytes: Vec<u8>) -> Result<Content, Error> {
    let text = String::from_utf8(bytes)
        .map_err(|_| invalid("non-UTF-8 diff content is unsupported; inspect with Git directly"))?;
    if text.is_empty() {
        return Ok(Content::Empty);
    }
    if text.lines().any(|line| line.starts_with("Binary files ")) {
        return Ok(Content::Binary { summary: text });
    }
    if text.contains('\0') {
        return Err(invalid(
            "text attributes produced NUL bytes; inspect with Git directly",
        ));
    }
    if text.lines().count() > 8000 || text.lines().any(|line| line.len() > 16384) {
        return Err(invalid(
            "patch exceeds 8000 lines or a 16 KiB line; inspect with Git directly",
        ));
    }
    Ok(Content::Patch { text })
}

impl Workspace {
    pub async fn diff(&self, scope: Scope, path: &str) -> Result<FileDiff, Error> {
        let selected = literal(path)?;
        let overview = self.inspect().await?;
        let git = overview
            .git
            .ok_or_else(|| invalid("saved workspace is an ordinary folder"))?;
        let change = git
            .changes
            .iter()
            .find(|change| change.path == path)
            .ok_or_else(|| {
                invalid("path is no longer in the changed-file list; refresh the workspace")
            })?
            .clone();
        let mut arguments = vec![
            "diff",
            "--patch",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--no-relative",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "--line-prefix=",
            "--diff-algorithm=myers",
            "--unified=3",
            "--word-diff=none",
            "--output-indicator-new=+",
            "--output-indicator-old=-",
            "--output-indicator-context= ",
            "--ignore-submodules=none",
            "--submodule=short",
            "--find-renames=50%",
        ];
        let original = change.original_path.as_deref().map(literal).transpose()?;
        let conflict = change.status.contains('U') || matches!(change.status.as_str(), "AA" | "DD");
        let content = if conflict {
            Content::Unavailable { reason: "Unmerged file. Resolve the index conflict with Git; two-side conflict review is not implemented.".into() }
        } else if change.status == "??" && scope == Scope::Staged {
            Content::Empty
        } else if change.status == "??" {
            let absolute = git.root.join(path);
            let metadata = tokio::task::spawn_blocking(move || std::fs::symlink_metadata(absolute))
                .await
                .map_err(invalid)?
                .map_err(invalid)?;
            if metadata.is_dir() {
                Content::Unavailable {
                    reason: "Untracked directory. Per-file directory expansion is not implemented."
                        .into(),
                }
            } else if !metadata.is_file() && !metadata.file_type().is_symlink() {
                Content::Unavailable {
                    reason: "Untracked special file cannot be previewed.".into(),
                }
            } else if metadata.len() > 1024 * 1024 {
                Content::Unavailable {
                    reason:
                        "Untracked file exceeds the 1 MiB input limit; inspect with Git directly."
                            .into(),
                }
            } else {
                // Git compares a symlink's target text rather than following it.
                arguments.extend(["--no-index", "--", "/dev/null", path]);
                classify(git_observation(&git.root, &arguments, true).await?)?
            }
        } else {
            if scope == Scope::Staged {
                // With an unborn HEAD Git uses the empty tree automatically.
                arguments.push("--cached");
            }
            arguments.extend(["--", &selected]);
            // A staged rename needs both sides. Unstaged compares the new name
            // against its index entry; including the old name could show another file.
            if scope == Scope::Staged
                && let Some(original) = &original
            {
                arguments.push(original);
            }
            classify(git_observation(&git.root, &arguments, false).await?)?
        };
        let after = self.inspect().await?;
        let current = after.git.ok_or_else(|| invalid("repository disappeared"))?;
        if git.root != current.root || git.head != current.head || git.branch != current.branch {
            return Err(invalid(
                "repository baseline changed while reading; refresh and select the file again",
            ));
        }
        Ok(FileDiff {
            workspace_id: self.id,
            scope,
            change,
            head: git.head,
            content,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pathspec_is_literal_and_root_relative() {
        for path in ["[ab]*.rs", ":(glob)**", "-file", "目录/line\nname"] {
            assert_eq!(literal(path).unwrap(), format!(":(top,literal){path}"));
        }
        for path in ["", "/tmp/file", "../file", "a/../b", "./file", "a\0b"] {
            assert!(literal(path).is_err(), "{path:?}");
        }
    }

    #[test]
    fn classifications_and_display_limits_are_explicit() {
        assert!(matches!(classify(vec![]).unwrap(), Content::Empty));
        assert!(matches!(
            classify(b"diff --git a/x b/x\nBinary files a/x and b/x differ\n".to_vec()).unwrap(),
            Content::Binary { .. }
        ));
        assert!(matches!(
            classify(b"@@ -1 +1 @@\n-a\n+b\n".to_vec()).unwrap(),
            Content::Patch { .. }
        ));
        assert!(classify(vec![255]).is_err());
        assert!(classify(b"text\0".to_vec()).is_err());
        assert!(classify("x".repeat(16385).into_bytes()).is_err());
        assert!(classify("x\n".repeat(8001).into_bytes()).is_err());
    }
}
