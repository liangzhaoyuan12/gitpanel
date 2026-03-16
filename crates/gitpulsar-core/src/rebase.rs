use anyhow::{Context, Result};

use crate::models::{RebaseAction, RebaseEntry};
use crate::repository::GitRepo;

impl GitRepo {
    /// List commits that would be rebased (from HEAD back `count` commits).
    /// Returns entries in reverse order (oldest first) matching rebase todo format.
    pub fn list_rebase_commits(&self, count: usize) -> Result<Vec<RebaseEntry>> {
        let commits = self.log_page(0, count)?;
        let entries: Vec<RebaseEntry> = commits.into_iter().rev().map(|c| {
            RebaseEntry {
                commit_id: c.id,
                short_id: c.short_id,
                message: c.summary,
                action: RebaseAction::Pick,
            }
        }).collect();
        Ok(entries)
    }

    /// Execute an interactive rebase using a custom GIT_SEQUENCE_EDITOR.
    /// The `entries` define the rebase todo list.
    /// `onto` is the base commit (e.g., "HEAD~N" or a branch name).
    pub fn execute_rebase(&self, entries: &[RebaseEntry], onto: &str) -> Result<String> {
        let repo_path = self.path().to_string_lossy().to_string();

        // Build the todo content
        let todo_content = entries.iter().map(|e| {
            let action = match e.action {
                RebaseAction::Pick => "pick",
                RebaseAction::Squash => "squash",
                RebaseAction::Fixup => "fixup",
                RebaseAction::Reword => "reword",
                RebaseAction::Edit => "edit",
                RebaseAction::Drop => "drop",
            };
            format!("{} {} {}", action, e.short_id, e.message)
        }).collect::<Vec<_>>().join("\n");

        // Write todo to a temp file
        let todo_path = std::env::temp_dir().join("gitpulsar_rebase_todo");
        std::fs::write(&todo_path, &todo_content)
            .context("Failed to write rebase todo")?;

        // Create a script that copies our todo file over the git-provided one
        let script_path = std::env::temp_dir().join("gitpulsar_rebase_editor.sh");
        let script_content = format!(
            "#!/bin/sh\ncp '{}' \"$1\"\n",
            todo_path.to_string_lossy()
        );
        std::fs::write(&script_path, &script_content)
            .context("Failed to write editor script")?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755));
        }

        let output = std::process::Command::new("git")
            .args(["rebase", "-i", onto])
            .env("GIT_SEQUENCE_EDITOR", script_path.to_string_lossy().as_ref())
            .current_dir(&repo_path)
            .output()
            .context("Failed to run git rebase")?;

        // Cleanup temp files
        let _ = std::fs::remove_file(&todo_path);
        let _ = std::fs::remove_file(&script_path);

        if output.status.success() {
            Ok("Rebase completed".to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let msg = if stderr.trim().is_empty() { stdout } else { stderr };
            anyhow::bail!("{}", msg.trim());
        }
    }
}
