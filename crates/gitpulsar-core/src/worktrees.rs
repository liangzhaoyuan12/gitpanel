use anyhow::{Context, Result};

use crate::models::WorktreeInfo;
use crate::repository::GitRepo;

impl GitRepo {
    /// List all worktrees for the repository.
    pub fn list_worktrees(&self) -> Result<Vec<WorktreeInfo>> {
        let repo = self.inner();
        let worktree_names = repo.worktrees().context("Failed to list worktrees")?;
        let current_path = self.path().to_string_lossy().to_string();
        let mut result = Vec::new();

        // The main worktree
        result.push(WorktreeInfo {
            name: "main".to_string(),
            path: current_path.clone(),
            is_current: true,
        });

        for name in worktree_names.iter().flatten() {
            if let Ok(wt) = repo.find_worktree(name) {
                let wt_path = wt.path().to_string_lossy().to_string();
                result.push(WorktreeInfo {
                    name: name.to_string(),
                    path: wt_path,
                    is_current: false,
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

    /// Remove a worktree. Uses git CLI for reliability.
    pub fn remove_worktree(&self, name: &str) -> Result<String> {
        let repo_path = self.path().to_string_lossy().to_string();
        let output = std::process::Command::new("git")
            .args(["worktree", "remove", name])
            .current_dir(&repo_path)
            .output()
            .context("Failed to run git worktree remove")?;

        if output.status.success() {
            Ok(format!("Removed worktree '{}'", name))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("{}", stderr.trim());
        }
    }
}
