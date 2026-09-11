use anyhow::{bail, Context, Result};

use crate::model::ResetMode;
use crate::utils::repository::GitRepo;

impl GitRepo {
    /// Cherry-pick a commit onto the current HEAD.
    pub fn cherry_pick(&self, commit_id: &str) -> Result<String> {
        let repo = self.inner();
        let oid = git2::Oid::from_str(commit_id).context("Invalid commit ID")?;
        let commit = repo.find_commit(oid).context("Commit not found")?;

        repo.cherrypick(&commit, None)
            .context("Cherry-pick failed")?;

        // Check for conflicts
        let mut index = repo.index()?;
        if index.has_conflicts() {
            bail!("Cherry-pick resulted in conflicts. Resolve them and commit manually.");
        }

        // Auto-commit the cherry-pick
        let sig = repo.signature().context("Failed to get signature")?;
        let message = commit.message().unwrap_or("cherry-pick");
        let tree_oid = index.write_tree()?;
        let tree = repo.find_tree(tree_oid)?;
        let head = repo.head()?.peel_to_commit()?;

        let new_oid = repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            message,
            &tree,
            &[&head],
        )?;

        // Clean up cherry-pick state
        repo.cleanup_state()?;

        let short = &new_oid.to_string()[..7];
        Ok(format!("Cherry-picked as {}", short))
    }

    /// Revert a commit (create a new commit that undoes the given commit).
    pub fn revert_commit(&self, commit_id: &str) -> Result<String> {
        let repo = self.inner();
        let oid = git2::Oid::from_str(commit_id).context("Invalid commit ID")?;
        let commit = repo.find_commit(oid).context("Commit not found")?;

        repo.revert(&commit, None).context("Revert failed")?;

        // Check for conflicts
        let mut index = repo.index()?;
        if index.has_conflicts() {
            bail!("Revert resulted in conflicts. Resolve them and commit manually.");
        }

        // Auto-commit the revert
        let sig = repo.signature().context("Failed to get signature")?;
        let summary = commit.summary().unwrap_or("commit");
        let message = format!("Revert \"{}\"", summary);
        let tree_oid = index.write_tree()?;
        let tree = repo.find_tree(tree_oid)?;
        let head = repo.head()?.peel_to_commit()?;

        let new_oid = repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &message,
            &tree,
            &[&head],
        )?;

        // Clean up revert state
        repo.cleanup_state()?;

        let short = &new_oid.to_string()[..7];
        Ok(format!("Reverted as {}", short))
    }

    /// Amend the HEAD commit. If `message` is Some, use it; otherwise keep the original.
    pub fn amend_commit(&self, message: Option<&str>) -> Result<String> {
        let repo = self.inner();
        let head = repo.head()?.peel_to_commit()?;

        let msg = match message {
            Some(m) => m.to_string(),
            None => head.message().unwrap_or("").to_string(),
        };

        // Use current index tree for amend
        let mut index = repo.index()?;
        let tree_oid = index.write_tree()?;
        let tree = repo.find_tree(tree_oid)?;

        let new_oid = head.amend(
            Some("HEAD"),
            None, // keep author
            None, // keep committer
            None, // keep encoding
            Some(&msg),
            Some(&tree),
        )?;

        let short = &new_oid.to_string()[..7];
        Ok(format!("Amended commit: {}", short))
    }

    /// Get the HEAD commit message (for pre-filling amend UI).
    pub fn head_commit_message(&self) -> Result<String> {
        let repo = self.inner();
        let head = repo.head()?.peel_to_commit()?;
        Ok(head.message().unwrap_or("").to_string())
    }

    /// Reset HEAD to a given commit.
    pub fn reset(&self, commit_id: &str, mode: ResetMode) -> Result<String> {
        let repo = self.inner();
        let oid = git2::Oid::from_str(commit_id).context("Invalid commit ID")?;
        let commit = repo.find_commit(oid).context("Commit not found")?;

        let reset_type = match mode {
            ResetMode::Soft => git2::ResetType::Soft,
            ResetMode::Mixed => git2::ResetType::Mixed,
            ResetMode::Hard => git2::ResetType::Hard,
        };

        repo.reset(commit.as_object(), reset_type, None)
            .context("Reset failed")?;

        let mode_str = match mode {
            ResetMode::Soft => "soft",
            ResetMode::Mixed => "mixed",
            ResetMode::Hard => "hard",
        };

        let short = &commit_id[..7.min(commit_id.len())];
        Ok(format!("Reset {} to {}", mode_str, short))
    }
}
