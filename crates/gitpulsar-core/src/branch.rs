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
}
