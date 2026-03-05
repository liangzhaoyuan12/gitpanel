use anyhow::Result;
use git2::{DiffOptions, Oid};

use crate::models::*;
use crate::repository::GitRepo;

impl GitRepo {
    /// Diff of unstaged changes (working tree vs index)
    pub fn diff_unstaged(&self) -> Result<Vec<DiffFile>> {
        let repo = self.inner();
        let diff = repo.diff_index_to_workdir(None, Some(DiffOptions::new().patience(true)))?;
        parse_diff(&diff)
    }

    /// Diff of staged changes (index vs HEAD)
    pub fn diff_staged(&self) -> Result<Vec<DiffFile>> {
        let repo = self.inner();
        let head_tree = repo.head()?.peel_to_tree()?;
        let diff = repo.diff_tree_to_index(
            Some(&head_tree),
            None,
            Some(DiffOptions::new().patience(true)),
        )?;
        parse_diff(&diff)
    }

    /// Diff for a specific commit (commit vs its first parent)
    pub fn diff_commit(&self, commit_id: &str) -> Result<Vec<DiffFile>> {
        let repo = self.inner();
        let oid = Oid::from_str(commit_id)?;
        let commit = repo.find_commit(oid)?;
        let tree = commit.tree()?;

        let parent_tree = if commit.parent_count() > 0 {
            Some(commit.parent(0)?.tree()?)
        } else {
            None
        };

        let diff = repo.diff_tree_to_tree(
            parent_tree.as_ref(),
            Some(&tree),
            Some(DiffOptions::new().patience(true)),
        )?;

        parse_diff(&diff)
    }
}

fn parse_diff(diff: &git2::Diff<'_>) -> Result<Vec<DiffFile>> {
    let mut files: Vec<DiffFile> = Vec::new();

    diff.print(git2::DiffFormat::Patch, |delta, hunk, line| {
        let path = delta
            .new_file()
            .path()
            .or(delta.old_file().path())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        // Find or create the file entry
        let file = if files.last().map(|f| f.path == path).unwrap_or(false) {
            files.last_mut().unwrap()
        } else {
            files.push(DiffFile {
                path: path.clone(),
                hunks: Vec::new(),
                stats: DiffStats::default(),
            });
            files.last_mut().unwrap()
        };

        // Handle hunk header
        if let Some(hunk) = hunk {
            let header = String::from_utf8_lossy(hunk.header()).to_string();
            if file.hunks.last().map(|h| h.header != header).unwrap_or(true) {
                file.hunks.push(DiffHunk {
                    header,
                    lines: Vec::new(),
                });
            }
        }

        // Handle diff lines
        if let Some(current_hunk) = file.hunks.last_mut() {
            let content = String::from_utf8_lossy(line.content()).to_string();
            let kind = match line.origin() {
                '+' => {
                    file.stats.insertions += 1;
                    DiffLineKind::Addition
                }
                '-' => {
                    file.stats.deletions += 1;
                    DiffLineKind::Deletion
                }
                _ => DiffLineKind::Context,
            };

            // Only add actual content lines, skip file headers
            if line.origin() == '+' || line.origin() == '-' || line.origin() == ' ' {
                current_hunk.lines.push(DiffLine {
                    kind,
                    content,
                    old_lineno: line.old_lineno(),
                    new_lineno: line.new_lineno(),
                });
            }
        }

        true
    })?;

    Ok(files)
}
