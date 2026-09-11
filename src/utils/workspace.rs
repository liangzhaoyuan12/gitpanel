use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::utils::repository::GitRepo;

#[derive(Debug, Clone)]
pub struct RepoIndicator {
    pub is_dirty: bool,
    /// True if there are tracked changes (modified/staged/deleted). Untracked-only stays false.
    pub has_tracked_changes: bool,
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

    let mut dirs: Vec<(String, PathBuf, bool)> = Vec::new();

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
        dirs.push((name, path, is_git_repo));
    }

    // Compute indicators in parallel for git repos
    let git_paths: Vec<(usize, PathBuf)> = dirs
        .iter()
        .enumerate()
        .filter(|(_, (_, _, is_git))| *is_git)
        .map(|(i, (_, path, _))| (i, path.clone()))
        .collect();

    let indicators: Vec<(usize, Option<RepoIndicator>)> = if git_paths.len() <= 1 {
        // No point in parallelizing for 0-1 repos
        git_paths
            .into_iter()
            .map(|(i, path)| (i, compute_indicator(&path)))
            .collect()
    } else {
        // Parallel indicator computation
        let (tx, rx) = std::sync::mpsc::channel();
        let num_threads = git_paths.len().min(8);
        let chunks: Vec<Vec<(usize, PathBuf)>> = {
            let chunk_size = git_paths.len().div_ceil(num_threads);
            git_paths.chunks(chunk_size).map(|c| c.to_vec()).collect()
        };

        for chunk in chunks {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for (i, path) in chunk {
                    let indicator = compute_indicator(&path);
                    let _ = tx.send((i, indicator));
                }
            });
        }
        drop(tx);

        rx.into_iter().collect()
    };

    let mut indicator_map: Vec<Option<RepoIndicator>> = vec![None; dirs.len()];
    for (i, indicator) in indicators {
        indicator_map[i] = indicator;
    }

    let mut entries: Vec<WorkspaceEntry> = dirs
        .into_iter()
        .enumerate()
        .map(|(i, (name, path, is_git_repo))| WorkspaceEntry {
            name,
            path,
            is_git_repo,
            indicator: indicator_map[i].take(),
        })
        .collect();

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
    let (tracked, untracked) = repo.dirty_kinds();

    Some(RepoIndicator {
        is_dirty: tracked || untracked,
        has_tracked_changes: tracked,
        ahead,
        branch,
    })
}
