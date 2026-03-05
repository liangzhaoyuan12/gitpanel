use anyhow::{Context, Result};
use std::collections::HashMap;

use crate::models::TagInfo;
use crate::repository::GitRepo;

impl GitRepo {
    /// List all tags in the repository.
    pub fn tags(&self) -> Result<Vec<TagInfo>> {
        let repo = self.inner();
        let tag_names = repo.tag_names(None).context("Failed to list tags")?;
        let mut tags = Vec::new();

        for name in tag_names.iter().flatten() {
            let refname = format!("refs/tags/{}", name);
            let reference = match repo.find_reference(&refname) {
                Ok(r) => r,
                Err(_) => continue,
            };

            let obj = match reference.peel(git2::ObjectType::Any) {
                Ok(o) => o,
                Err(_) => continue,
            };

            // Check if annotated tag
            let (is_annotated, message, target_id) = if let Ok(tag) = obj.clone().into_tag() {
                let msg = tag.message().map(String::from);
                let tid = tag
                    .target()
                    .ok()
                    .and_then(|t| t.as_commit().map(|c| c.id().to_string()))
                    .unwrap_or_else(|| tag.target_id().to_string());
                (true, msg, tid)
            } else {
                (false, None, obj.id().to_string())
            };

            tags.push(TagInfo {
                name: name.to_string(),
                target_id,
                is_annotated,
                message,
            });
        }

        Ok(tags)
    }

    /// Create a lightweight tag pointing to a commit.
    pub fn create_tag(&self, name: &str, commit_id: &str) -> Result<String> {
        let repo = self.inner();
        let oid = git2::Oid::from_str(commit_id).context("Invalid commit ID")?;
        let obj = repo
            .find_object(oid, None)
            .context("Failed to find commit")?;
        repo.tag_lightweight(name, &obj, false)
            .context("Failed to create tag")?;
        Ok(format!("Created tag '{}'", name))
    }

    /// Create an annotated tag with a message.
    pub fn create_annotated_tag(
        &self,
        name: &str,
        commit_id: &str,
        message: &str,
    ) -> Result<String> {
        let repo = self.inner();
        let oid = git2::Oid::from_str(commit_id).context("Invalid commit ID")?;
        let obj = repo
            .find_object(oid, None)
            .context("Failed to find commit")?;
        let sig = repo.signature().context("Failed to get signature")?;
        repo.tag(name, &obj, &sig, message, false)
            .context("Failed to create annotated tag")?;
        Ok(format!("Created annotated tag '{}'", name))
    }

    /// Delete a tag by name.
    pub fn delete_tag(&self, name: &str) -> Result<()> {
        let repo = self.inner();
        repo.tag_delete(name)
            .context("Failed to delete tag")?;
        Ok(())
    }

    /// Get a mapping of commit_id → list of tag names for all tags.
    pub fn tags_by_commit(&self) -> Result<HashMap<String, Vec<String>>> {
        let tags = self.tags()?;
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for tag in tags {
            map.entry(tag.target_id).or_default().push(tag.name);
        }
        Ok(map)
    }
}
