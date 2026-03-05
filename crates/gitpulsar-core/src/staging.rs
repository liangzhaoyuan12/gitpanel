use anyhow::{Context, Result};
use std::path::Path;

use crate::repository::GitRepo;

impl GitRepo {
    /// Stage a file (add to index)
    pub fn stage_file(&self, path: &str) -> Result<()> {
        let repo = self.inner();
        let mut index = repo.index()?;
        index
            .add_path(Path::new(path))
            .context("Failed to stage file")?;
        index.write().context("Failed to write index")?;
        Ok(())
    }

    /// Stage all changes
    pub fn stage_all(&self) -> Result<()> {
        let repo = self.inner();
        let mut index = repo.index()?;
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .context("Failed to stage all files")?;
        index.write()?;
        Ok(())
    }

    /// Unstage a file (reset from index to HEAD)
    pub fn unstage_file(&self, path: &str) -> Result<()> {
        let repo = self.inner();
        let head = repo.head()?.peel_to_commit()?;
        repo.reset_default(Some(head.as_object()), [path])?;
        Ok(())
    }

    /// Unstage all files
    pub fn unstage_all(&self) -> Result<()> {
        let repo = self.inner();
        let head = repo.head()?.peel_to_commit()?;
        repo.reset_default(Some(head.as_object()), ["*"])?;
        Ok(())
    }

    /// Create a commit with staged changes
    pub fn commit(&self, message: &str) -> Result<String> {
        let repo = self.inner();
        let sig = repo.signature().context("Failed to get signature")?;
        let mut index = repo.index()?;
        let tree_oid = index.write_tree()?;
        let tree = repo.find_tree(tree_oid)?;

        let parents = if let Ok(head) = repo.head() {
            vec![head.peel_to_commit()?]
        } else {
            vec![]
        };

        let parent_refs: Vec<&git2::Commit<'_>> = parents.iter().collect();

        let oid = repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            message,
            &tree,
            &parent_refs,
        )?;

        Ok(oid.to_string())
    }

    /// Discard changes in a working tree file (checkout from index)
    pub fn discard_file(&self, path: &str) -> Result<()> {
        let repo = self.inner();
        repo.checkout_index(
            None,
            Some(
                git2::build::CheckoutBuilder::new()
                    .path(path)
                    .force(),
            ),
        )?;
        Ok(())
    }
}
