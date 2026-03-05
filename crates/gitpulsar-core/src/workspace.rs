use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::repository::GitRepo;

#[derive(Debug, Clone)]
pub struct RepoIndicator {
    pub is_dirty: bool,
    pub ahead: usize,
    pub branch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_git_repo: bool,
    pub indicator: Option<RepoIndicator>,
}

/// Scan a workspace directory for git repositories (1 level deep).
/// If root itself is a git repo, returns a single entry for backward compat.
pub fn scan_workspace(root: &Path) -> Result<Vec<WorkspaceEntry>> {
    // Check if root itself is a git repo
    if root.join(".git").exists() {
        let indicator = compute_indicator(root);
        return Ok(vec![WorkspaceEntry {
            name: root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| root.to_string_lossy().to_string()),
            path: root.to_path_buf(),
            is_git_repo: true,
            indicator,
        }]);
    }

    let mut entries = Vec::new();

    let read_dir = std::fs::read_dir(root)?;
    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden dirs
        if name.starts_with('.') {
            continue;
        }

        let is_git_repo = path.join(".git").exists();
        let indicator = if is_git_repo {
            compute_indicator(&path)
        } else {
            None
        };

        entries.push(WorkspaceEntry {
            name,
            path,
            is_git_repo,
            indicator,
        });
    }

    entries.sort_by(|a, b| {
        // Git repos first, then alphabetically
        b.is_git_repo.cmp(&a.is_git_repo).then(a.name.cmp(&b.name))
    });

    Ok(entries)
}

/// Quick status check: dirty + ahead + branch name.
fn compute_indicator(path: &Path) -> Option<RepoIndicator> {
    let repo = GitRepo::open(&path.to_string_lossy()).ok()?;
    let branch = repo.current_branch_name();
    let (ahead, _) = repo.ahead_behind().unwrap_or((0, 0));
    let is_dirty = repo
        .status()
        .map(|s| !s.unstaged.is_empty() || !s.untracked.is_empty() || !s.staged.is_empty())
        .unwrap_or(false);

    Some(RepoIndicator {
        is_dirty,
        ahead,
        branch,
    })
}
