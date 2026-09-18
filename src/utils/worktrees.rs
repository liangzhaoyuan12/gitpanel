use anyhow::{Context, Result};

use crate::model::WorktreeInfo;
use crate::utils::repository::GitRepo;

impl GitRepo {
    /// List all worktrees for the repository.
    pub fn list_worktrees(&self) -> Result<Vec<WorktreeInfo>> {
        let repo = self.inner();
        let worktree_names = repo.worktrees().context("Failed to list worktrees")?;
        let repo_path = self.path().to_string_lossy().to_string();
        let mut result = Vec::new();

        // The main worktree
        let main_name = std::path::Path::new(&repo_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "main".to_string());
        result.push(WorktreeInfo {
            name: main_name,
            path: repo_path.clone(),
            is_current: true,
        });

        for name in worktree_names.iter().flatten() {
            if let Ok(wt) = repo.find_worktree(name) {
                let wt_path = wt.path().to_string_lossy().to_string();
                // Determine is_current by comparing paths, not by name
                let is_current = wt_path == repo_path;
                result.push(WorktreeInfo {
                    name: name.to_string(),
                    path: wt_path,
                    is_current,
                });
            }
        }

        Ok(result)
    }

    /// Add a new worktree. Uses git CLI for reliability.
    pub fn add_worktree(&self, path: &str, branch: Option<&str>) -> Result<String> {
        let repo_path = self.path().to_string_lossy().to_string();
        let mut args = vec!["worktree", "add"];
        if let Some(b) = branch {
            args.push("-b");
            args.push(b);
        }
        args.push(path);

        let output = std::process::Command::new("git")
            .args(&args)
            .current_dir(&repo_path)
            .output()
            .context("Failed to run git worktree add")?;

        if output.status.success() {
            Ok(format!("Added worktree at '{}'", path))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("{}", stderr.trim());
        }
    }

    /// Remove a worktree by its filesystem path. Uses git CLI for reliability.
    pub fn remove_worktree(&self, path: &str) -> Result<String> {
        let repo_path = self.path().to_string_lossy().to_string();
        let output = std::process::Command::new("git")
            .args(["worktree", "remove", path])
            .current_dir(&repo_path)
            .output()
            .context("Failed to run git worktree remove")?;

        if output.status.success() {
            Ok(format!("Removed worktree '{}'", path))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("{}", stderr.trim());
        }
    }
}
