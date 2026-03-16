use anyhow::{Context, Result};
use std::path::Path;

use crate::models::{DiffHunk, DiffLineKind};
use crate::repository::GitRepo;

/// Build a unified diff patch for a single hunk (for staging).
fn build_hunk_patch(path: &str, hunk: &DiffHunk) -> Vec<u8> {
    let mut patch = String::new();
    patch.push_str(&format!("--- a/{}\n", path));
    patch.push_str(&format!("+++ b/{}\n", path));
    patch.push_str(&hunk.header);
    if !hunk.header.ends_with('\n') {
        patch.push('\n');
    }
    for line in &hunk.lines {
        let prefix = match line.kind {
            DiffLineKind::Addition => '+',
            DiffLineKind::Deletion => '-',
            DiffLineKind::Context => ' ',
        };
        patch.push(prefix);
        patch.push_str(&line.content);
        if !line.content.ends_with('\n') {
            patch.push('\n');
        }
    }
    patch.into_bytes()
}

/// Build a reverse unified diff patch for a single hunk (for unstaging).
fn build_reverse_hunk_patch(path: &str, hunk: &DiffHunk) -> Vec<u8> {
    let mut patch = String::new();
    patch.push_str(&format!("--- a/{}\n", path));
    patch.push_str(&format!("+++ b/{}\n", path));
    // Reverse the hunk header line counts
    let reversed_header = reverse_hunk_header(&hunk.header);
    patch.push_str(&reversed_header);
    if !reversed_header.ends_with('\n') {
        patch.push('\n');
    }
    for line in &hunk.lines {
        let prefix = match line.kind {
            DiffLineKind::Addition => '-', // reverse: add becomes del
            DiffLineKind::Deletion => '+', // reverse: del becomes add
            DiffLineKind::Context => ' ',
        };
        patch.push(prefix);
        patch.push_str(&line.content);
        if !line.content.ends_with('\n') {
            patch.push('\n');
        }
    }
    patch.into_bytes()
}

/// Reverse the @@ line counts in a hunk header.
/// "@@ -a,b +c,d @@" becomes "@@ -c,d +a,b @@"
fn reverse_hunk_header(header: &str) -> String {
    // Parse: @@ -old_start,old_count +new_start,new_count @@ ...
    let trimmed = header.trim();
    if let Some(rest) = trimmed.strip_prefix("@@") {
        if let Some(end_idx) = rest.find("@@") {
            let range_part = &rest[..end_idx].trim();
            let suffix = &rest[end_idx + 2..];
            let parts: Vec<&str> = range_part.split_whitespace().collect();
            if parts.len() == 2 {
                return format!("@@ {} {} @@{}", parts[1].replace('+', "~").replace('-', "+").replace('~', "-"),
                    parts[0].replace('-', "~").replace('+', "-").replace('~', "+"), suffix);
            }
        }
    }
    header.to_string()
}

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

    /// Stage a single hunk of a file by applying a partial patch to the index.
    pub fn stage_hunk(&self, path: &str, hunk_index: usize) -> Result<()> {
        let diff_files = self.diff_unstaged()?;
        let file = diff_files.iter()
            .find(|f| f.path == path)
            .context("File not found in unstaged diff")?;
        let hunk = file.hunks.get(hunk_index)
            .context("Hunk index out of range")?;
        let patch = build_hunk_patch(path, hunk);
        let diff = git2::Diff::from_buffer(&patch)?;
        self.inner().apply(&diff, git2::ApplyLocation::Index, None)?;
        Ok(())
    }

    /// Unstage a single hunk by applying the reverse patch to the index.
    pub fn unstage_hunk(&self, path: &str, hunk_index: usize) -> Result<()> {
        let diff_files = self.diff_staged()?;
        let file = diff_files.iter()
            .find(|f| f.path == path)
            .context("File not found in staged diff")?;
        let hunk = file.hunks.get(hunk_index)
            .context("Hunk index out of range")?;
        let patch = build_reverse_hunk_patch(path, hunk);
        let diff = git2::Diff::from_buffer(&patch)?;
        self.inner().apply(&diff, git2::ApplyLocation::Index, None)?;
        Ok(())
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
