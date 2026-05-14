use anyhow::{Context, Result};

use crate::models::ConflictFile;
use crate::repository::GitRepo;

impl GitRepo {
    /// Check if the repository index has conflicts.
    pub fn has_conflicts(&self) -> bool {
        self.inner().index()
            .map(|idx| idx.has_conflicts())
            .unwrap_or(false)
    }

    /// List all conflicted files with their content from each side.
    pub fn conflict_files(&self) -> Result<Vec<ConflictFile>> {
        let repo = self.inner();
        let index = repo.index()?;
        let conflicts = index.conflicts()?;
        let mut result = Vec::new();

        for conflict in conflicts {
            let conflict = conflict?;
            let path = conflict.our.as_ref()
                .or(conflict.their.as_ref())
                .and_then(|e| std::str::from_utf8(&e.path).ok())
                .unwrap_or("")
                .to_string();

            let read_blob = |entry: &Option<git2::IndexEntry>| -> Option<Vec<u8>> {
                let e = entry.as_ref()?;
                repo.find_blob(e.id).ok().map(|b| b.content().to_vec())
            };

            result.push(ConflictFile {
                path,
                ancestor: read_blob(&conflict.ancestor),
                ours: read_blob(&conflict.our),
                theirs: read_blob(&conflict.their),
            });
        }

        Ok(result)
    }

    /// Mark a conflict as resolved by staging the file.
    pub fn mark_resolved(&self, path: &str) -> Result<()> {
        self.stage_file(path)
    }

    /// Continue an in-progress merge (create the merge commit).
    pub fn continue_merge(&self) -> Result<String> {
        let path = self.path().to_string_lossy().to_string();
        let output = std::process::Command::new("git")
            .args(["merge", "--continue"])
            .current_dir(&path)
            .output()
            .context("Failed to continue merge")?;

        if output.status.success() {
            Ok("Merge completed".to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("{}", stderr.trim());
        }
    }

    /// Continue an in-progress rebase.
    pub fn continue_rebase(&self) -> Result<String> {
        let path = self.path().to_string_lossy().to_string();
        let output = std::process::Command::new("git")
            .args(["rebase", "--continue"])
            .current_dir(&path)
            .output()
            .context("Failed to continue rebase")?;

        if output.status.success() {
            Ok("Rebase continued".to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("{}", stderr.trim());
        }
    }

    /// Abort an in-progress merge.
    pub fn abort_merge(&self) -> Result<()> {
        let path = self.path().to_string_lossy().to_string();
        let output = std::process::Command::new("git")
            .args(["merge", "--abort"])
            .current_dir(&path)
            .output()
            .context("Failed to abort merge")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("{}", stderr.trim());
        }
        Ok(())
    }

    /// Abort an in-progress rebase.
    pub fn abort_rebase(&self) -> Result<()> {
        let path = self.path().to_string_lossy().to_string();
        let output = std::process::Command::new("git")
            .args(["rebase", "--abort"])
            .current_dir(&path)
            .output()
            .context("Failed to abort rebase")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("{}", stderr.trim());
        }
        Ok(())
    }

    /// Check if a merge is in progress.
    pub fn is_merging(&self) -> bool {
        self.path().join(".git/MERGE_HEAD").exists()
            || self.inner().path().join("MERGE_HEAD").exists()
    }

    /// Check if a rebase is in progress.
    pub fn is_rebasing(&self) -> bool {
        let git_dir = self.inner().path();
        git_dir.join("rebase-merge").exists()
            || git_dir.join("rebase-apply").exists()
    }

    /// Check if a bisect is in progress.
    pub fn is_bisecting(&self) -> bool {
        self.inner().path().join("BISECT_START").exists()
    }
}
