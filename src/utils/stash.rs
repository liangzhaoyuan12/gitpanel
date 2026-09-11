use anyhow::{Context, Result};
use git2::StashFlags;

use crate::model::StashEntry;
use crate::utils::repository::GitRepo;

impl GitRepo {
    /// Stash current changes (including untracked files).
    pub fn stash_save(&mut self, message: Option<&str>) -> Result<String> {
        let repo = self.inner_mut();
        let sig = repo.signature().context("Failed to get default signature")?;
        let oid = repo.stash_save(
            &sig,
            message.unwrap_or("WIP"),
            Some(StashFlags::INCLUDE_UNTRACKED),
        )?;
        Ok(format!("Stashed: {}", &oid.to_string()[..7]))
    }

    /// Pop the top stash entry.
    pub fn stash_pop(&mut self) -> Result<()> {
        let repo = self.inner_mut();
        repo.stash_pop(0, None)?;
        Ok(())
    }

    /// List all stash entries.
    pub fn stash_list(&mut self) -> Result<Vec<StashEntry>> {
        let mut entries = Vec::new();
        let repo = self.inner_mut();
        repo.stash_foreach(|index, message, _oid| {
            entries.push(StashEntry {
                index,
                message: message.to_string(),
            });
            true
        })?;
        Ok(entries)
    }

    /// Drop a stash entry by index.
    pub fn stash_drop(&mut self, index: usize) -> Result<()> {
        let repo = self.inner_mut();
        repo.stash_drop(index)?;
        Ok(())
    }

    /// Apply a stash entry by index (without removing it from the stash list).
    pub fn stash_apply(&mut self, index: usize) -> Result<()> {
        let repo = self.inner_mut();
        repo.stash_apply(index, None)?;
        Ok(())
    }
}
