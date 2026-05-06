use anyhow::{bail, Context, Result};
use git2::BranchType;

use crate::repository::GitRepo;

impl GitRepo {
    /// Checkout a local branch by name.
    pub fn checkout_branch(&self, name: &str) -> Result<()> {
        let repo = self.inner();

        // Check for dirty state
        let statuses = repo.statuses(None)?;
        let has_changes = statuses.iter().any(|s| {
            let st = s.status();
            st.intersects(
                git2::Status::INDEX_NEW
                    | git2::Status::INDEX_MODIFIED
                    | git2::Status::INDEX_DELETED
                    | git2::Status::WT_MODIFIED
                    | git2::Status::WT_DELETED,
            )
        });
        if has_changes {
            bail!("You have uncommitted changes. Commit or stash them first.");
        }

        let branch = repo
            .find_branch(name, BranchType::Local)
            .context("Branch not found")?;
        let refname = branch
            .get()
            .name()
            .context("Invalid branch ref name")?
            .to_string();
        let commit = branch.get().peel_to_commit()?;

        repo.set_head(&refname)?;
        repo.checkout_tree(
            commit.as_object(),
            Some(git2::build::CheckoutBuilder::new().safe()),
        )?;

        Ok(())
    }

    /// Checkout a remote tracking branch, creating a local branch.
    /// `remote_name` should be like "origin/feature-branch".
    pub fn checkout_remote_branch(&self, remote_name: &str) -> Result<()> {
        let repo = self.inner();

        // Check for dirty state
        let statuses = repo.statuses(None)?;
        let has_changes = statuses.iter().any(|s| {
            let st = s.status();
            st.intersects(
                git2::Status::INDEX_NEW
                    | git2::Status::INDEX_MODIFIED
                    | git2::Status::INDEX_DELETED
                    | git2::Status::WT_MODIFIED
                    | git2::Status::WT_DELETED,
            )
        });
        if has_changes {
            bail!("You have uncommitted changes. Commit or stash them first.");
        }

        // Extract local branch name from "origin/feature" → "feature"
        let local_name = remote_name
            .split_once('/')
            .map(|(_, name)| name)
            .unwrap_or(remote_name);

        // Check if local branch already exists
        if repo.find_branch(local_name, BranchType::Local).is_ok() {
            // Just checkout the existing local branch
            return self.checkout_branch(local_name);
        }

        // Find the remote branch
        let remote_ref_name = format!("refs/remotes/{}", remote_name);
        let remote_ref = repo
            .find_reference(&remote_ref_name)
            .context("Remote branch not found")?;
        let commit = remote_ref.peel_to_commit()?;

        // Create local tracking branch
        let mut branch = repo.branch(local_name, &commit, false)?;

        // Set upstream
        branch.set_upstream(Some(remote_name))?;

        // Checkout
        let refname = format!("refs/heads/{}", local_name);
        repo.set_head(&refname)?;
        repo.checkout_tree(
            commit.as_object(),
            Some(git2::build::CheckoutBuilder::new().safe()),
        )?;

        Ok(())
    }

    /// Checkout a specific commit in detached HEAD mode.
    pub fn checkout_detached(&self, commit_id: &str) -> Result<()> {
        let repo = self.inner();
        let oid = git2::Oid::from_str(commit_id)
            .context("Invalid commit SHA")?;
        let commit = repo.find_commit(oid)
            .context("Commit not found")?;
        repo.set_head_detached(commit.id())?;
        repo.checkout_head(Some(
            git2::build::CheckoutBuilder::new().force(),
        ))?;
        Ok(())
    }

    /// Restore a single file from the given commit into the working tree (and index).
    /// Equivalent to `git checkout <commit> -- <path>`.
    pub fn checkout_file_from_commit(&self, commit_id: &str, file_path: &str) -> Result<()> {
        let repo = self.inner();
        let oid = git2::Oid::from_str(commit_id).context("Invalid commit SHA")?;
        let commit = repo.find_commit(oid).context("Commit not found")?;
        let tree = commit.tree()?;

        let path = std::path::Path::new(file_path);
        let mut builder = git2::build::CheckoutBuilder::new();
        builder.path(path);
        builder.force();
        // Update index too so the change appears as staged (matches git CLI behavior)
        builder.update_index(true);

        repo.checkout_tree(tree.as_object(), Some(&mut builder))
            .context("Failed to restore file from commit")?;
        Ok(())
    }

    /// Create a new branch from HEAD.
    pub fn create_branch(&self, name: &str, checkout: bool) -> Result<String> {
        let repo = self.inner();
        let head_commit = repo.head()?.peel_to_commit()?;

        repo.branch(name, &head_commit, false)
            .context("Failed to create branch")?;

        let oid = head_commit.id().to_string();
        let short = &oid[..7.min(oid.len())];

        if checkout {
            self.checkout_branch(name)?;
        }

        Ok(format!("Created branch '{}' at {}", name, short))
    }

    /// Delete a local branch by name.
    /// If `force` is true, allows deleting unmerged branches.
    pub fn delete_branch(&self, name: &str, force: bool) -> Result<()> {
        let repo = self.inner();

        // Don't allow deleting the current branch
        if let Ok(head) = repo.head() {
            if head.is_branch() {
                if let Some(head_name) = head.shorthand() {
                    if head_name == name {
                        bail!("Cannot delete the currently checked out branch '{}'", name);
                    }
                }
            }
        }

        let mut branch = repo
            .find_branch(name, BranchType::Local)
            .context("Branch not found")?;

        if force {
            branch.delete().context("Failed to delete branch")?;
        } else {
            // Check if branch is merged before deleting
            if !branch.is_head() {
                branch.delete().context("Failed to delete branch. Use force to delete unmerged branches.")?;
            }
        }

        Ok(())
    }

    /// Rename a local branch.
    pub fn rename_branch(&self, old_name: &str, new_name: &str) -> Result<()> {
        let repo = self.inner();

        let mut branch = repo
            .find_branch(old_name, BranchType::Local)
            .context("Branch not found")?;

        branch.rename(new_name, false)
            .context("Failed to rename branch")?;

        Ok(())
    }
}
