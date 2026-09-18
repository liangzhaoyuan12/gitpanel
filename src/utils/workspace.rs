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
    /// Sub-entries (folders and nested git repos). Empty for leaves.
    pub children: Vec<WorkspaceEntry>,
}

/// Find an entry (or its parent folder) by path, recursively.
pub fn find_entry_by_path<'a>(entries: &'a [WorkspaceEntry], path: &Path) -> Option<&'a WorkspaceEntry> {
    for entry in entries {
        if entry.path == path {
            return Some(entry);
        }
        if let Some(found) = find_entry_by_path(&entry.children, path) {
            return Some(found);
        }
    }
    None
}

/// Find a mutable entry by path, recursively.
pub fn find_entry_by_path_mut<'a>(entries: &'a mut [WorkspaceEntry], path: &Path) -> Option<&'a mut WorkspaceEntry> {
    for entry in entries.iter_mut() {
        if entry.path == path {
            return Some(entry);
        }
        if let Some(found) = find_entry_by_path_mut(&mut entry.children, path) {
            return Some(found);
        }
    }
    None
}

/// Scan a workspace directory for git repositories recursively (up to 5 levels deep).
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
            children: Vec::new(),
        }]);
    }

    let mut entries = scan_dir_recursive(root, 0, 5)?;

    // Compute indicators in parallel for all git repos
    let mut git_paths: Vec<(usize, Vec<usize>, PathBuf)> = Vec::new();
    collect_git_paths(&entries, &[], &mut git_paths);

    if !git_paths.is_empty() {
        let indicators: Vec<(Vec<usize>, Option<RepoIndicator>)> = if git_paths.len() <= 1 {
            git_paths
                .into_iter()
                .map(|(_, idx, path)| (idx, compute_indicator(&path)))
                .collect()
        } else {
            let (tx, rx) = std::sync::mpsc::channel();
            let num_threads = git_paths.len().min(8);
            let chunks: Vec<Vec<(Vec<usize>, PathBuf)>> = {
                let chunk_size = git_paths.len().div_ceil(num_threads);
                git_paths
                    .into_iter()
                    .map(|(_, idx, path)| (idx, path))
                    .collect::<Vec<_>>()
                    .chunks(chunk_size)
                    .map(|c| c.to_vec())
                    .collect()
            };

            for chunk in chunks {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    for (idx, path) in chunk {
                        let indicator = compute_indicator(&path);
                        let _ = tx.send((idx, indicator));
                    }
                });
            }
            drop(tx);

            rx.into_iter().collect()
        };

        // Apply indicators to entries using index paths
        for (idx_path, indicator) in indicators {
            if let Some(entry) = get_entry_by_index_path_mut(&mut entries, &idx_path) {
                entry.indicator = indicator;
            }
        }
    }

    Ok(entries)
}

/// Directories skipped during workspace scanning (heavyweight or
/// version-control-internal directories that should never be recursed into).
const SKIP_DIRS: &[&str] = &[
    "node_modules", "target", ".cache", ".gradle", "Pods",
    "__pycache__", "dist", "build", ".venv", "venv",
];

/// Scan a directory recursively, building a tree of entries.
fn scan_dir_recursive(dir: &Path, depth: usize, max_depth: usize) -> Result<Vec<WorkspaceEntry>> {
    let mut entries: Vec<WorkspaceEntry> = Vec::new();
    let read_dir = std::fs::read_dir(dir)?;

    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        // Use file_type() to avoid following symlinks — symlink cycles would
        // otherwise cause exponential expansion even with a depth limit.
        let file_type = entry.file_type()?;
        if !file_type.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }

        let is_git_repo = path.join(".git").exists();

        let children = if !is_git_repo && depth < max_depth {
            scan_dir_recursive(&path, depth + 1, max_depth).unwrap_or_default()
        } else {
            Vec::new()
        };

        entries.push(WorkspaceEntry {
            name,
            path,
            is_git_repo,
            indicator: None,
            children,
        });
    }

    // Sort: git repos first, then alphabetically
    entries.sort_by(|a, b| {
        b.is_git_repo
            .cmp(&a.is_git_repo)
            .then(a.name.cmp(&b.name))
    });

    Ok(entries)
}

/// Collect paths to all git repos in the tree, with their index paths.
fn collect_git_paths(
    entries: &[WorkspaceEntry],
    parent_idx: &[usize],
    out: &mut Vec<(usize, Vec<usize>, PathBuf)>,
) {
    for (i, entry) in entries.iter().enumerate() {
        let mut idx_path = parent_idx.to_vec();
        idx_path.push(i);
        if entry.is_git_repo {
            out.push((i, idx_path.clone(), entry.path.clone()));
        }
        collect_git_paths(&entry.children, &idx_path, out);
    }
}

/// Get a mutable reference to an entry by its index path (sequence of child indices).
fn get_entry_by_index_path_mut<'a>(
    entries: &'a mut [WorkspaceEntry],
    idx_path: &[usize],
) -> Option<&'a mut WorkspaceEntry> {
    if idx_path.is_empty() {
        return None;
    }
    let (first, rest) = idx_path.split_first()?;
    let entry = entries.get_mut(*first)?;
    if rest.is_empty() {
        Some(entry)
    } else {
        get_entry_by_index_path_mut(&mut entry.children, rest)
    }
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
