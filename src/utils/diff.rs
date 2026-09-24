use anyhow::{Context, Result};
use git2::{DiffOptions, Oid};

use crate::model::*;
use crate::utils::repository::GitRepo;

impl GitRepo {
    /// Generate a diff for an untracked file (all lines are additions).
    pub fn diff_untracked(&self, path: &str) -> Result<DiffFile> {
        let repo_path = self.path();
        let full_path = repo_path.join(path);
        let bytes = std::fs::read(&full_path)
            .with_context(|| format!("Failed to read untracked file: {}", path))?;

        // Binary untracked file: return a flagged entry so the UI shows the
        // "binary diff not supported" message. The old read_to_string failed
        // here and callers swallowed the error, rendering nothing at all.
        if looks_binary(&bytes) {
            return Ok(DiffFile {
                path: path.to_string(),
                hunks: Vec::new(),
                stats: DiffStats::default(),
                is_binary: true,
            });
        }

        // Not binary — decode leniently so a non-UTF-8 *text* file still gets
        // a diff instead of a hard read error.
        let content = String::from_utf8_lossy(&bytes);
        let lines: Vec<DiffLine> = content
            .lines()
            .enumerate()
            .map(|(i, line)| DiffLine {
                kind: DiffLineKind::Addition,
                content: format!("{}\n", line),
                old_lineno: None,
                new_lineno: Some((i + 1) as u32),
            })
            .collect();

        let insertions = lines.len();
        let header = format!("@@ -0,0 +1,{} @@", insertions);

        Ok(DiffFile {
            path: path.to_string(),
            hunks: vec![DiffHunk {
                header,
                lines,
            }],
            stats: DiffStats {
                insertions,
                deletions: 0,
            },
            is_binary: false,
        })
    }

    /// Diff of unstaged changes (working tree vs index)
    pub fn diff_unstaged(&self) -> Result<Vec<DiffFile>> {
        let repo = self.inner();
        let diff = repo.diff_index_to_workdir(None, Some(DiffOptions::new().patience(true)))?;
        parse_diff(&diff)
    }

    /// Diff of staged changes (index vs HEAD).
    /// Returns an empty diff on an unborn HEAD (no commits yet).
    pub fn diff_staged(&self) -> Result<Vec<DiffFile>> {
        let repo = self.inner();
        let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        let diff = repo.diff_tree_to_index(
            head_tree.as_ref(),
            None,
            Some(DiffOptions::new().patience(true)),
        )?;
        parse_diff(&diff)
    }

    /// Diff for a specific commit (commit vs its first parent)
    /// Diff between two refs (branches, tags, or commit SHAs).
    /// Returns the changes that go from `base` to `target`.
    pub fn diff_refs(&self, base: &str, target: &str) -> Result<Vec<DiffFile>> {
        let repo = self.inner();
        let base_obj = repo
            .revparse_single(base)?
            .peel_to_commit()?;
        let target_obj = repo
            .revparse_single(target)?
            .peel_to_commit()?;

        let base_tree = base_obj.tree()?;
        let target_tree = target_obj.tree()?;

        let diff = repo.diff_tree_to_tree(
            Some(&base_tree),
            Some(&target_tree),
            Some(DiffOptions::new().patience(true)),
        )?;

        parse_diff(&diff)
    }

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

/// Git's own heuristic: a NUL byte within the first 8000 bytes means binary.
fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|&b| b == 0)
}

fn parse_diff(diff: &git2::Diff<'_>) -> Result<Vec<DiffFile>> {
    let mut files: Vec<DiffFile> = Vec::new();

    // Seed one entry per delta up front. A binary file can print no patch
    // lines at all, and the BINARY flag lives on the delta — without seeding,
    // such a file silently vanished from the list instead of showing the
    // "binary diff not supported" message.
    for delta in diff.deltas() {
        let path = delta
            .new_file()
            .path()
            .or(delta.old_file().path())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        files.push(DiffFile {
            path,
            hunks: Vec::new(),
            stats: DiffStats::default(),
            is_binary: delta.flags().contains(git2::DiffFlags::BINARY),
        });
    }

    diff.print(git2::DiffFormat::Patch, |delta, hunk, line| {
        let path = delta
            .new_file()
            .path()
            .or(delta.old_file().path())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        // Locate the seeded entry — fast path is the previous file, since a
        // patch prints one file contiguously.
        let idx = if files.last().map(|f| f.path == path).unwrap_or(false) {
            files.len() - 1
        } else {
            match files.iter().position(|f| f.path == path) {
                Some(i) => i,
                // Not in deltas() (shouldn't happen) — keep old behaviour and
                // create it rather than dropping lines on the floor.
                None => {
                    files.push(DiffFile {
                        path: path.clone(),
                        hunks: Vec::new(),
                        stats: DiffStats::default(),
                        is_binary: false,
                    });
                    files.len() - 1
                }
            }
        };
        let file = &mut files[idx];
        // libgit2 decides "binary" once it has read content — keep OR-ing it
        // in so a flag that arrives after seeding is not lost.
        file.is_binary |= delta.flags().contains(git2::DiffFlags::BINARY);

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
