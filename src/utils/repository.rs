use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use git2::{BranchType, Repository, StatusOptions};

use crate::model::*;

pub struct GitRepo {
    repo: Repository,
}

impl GitRepo {
    pub fn open(path: &str) -> Result<Self> {
        let repo = Repository::open(path).context("Failed to open repository")?;
        Ok(Self { repo })
    }

    pub fn path(&self) -> &std::path::Path {
        self.repo.workdir().unwrap_or(self.repo.path())
    }

    /// Get repository status. When `recurse_untracked` is false, untracked
    /// directories are listed as a single entry (faster for background polling).
    pub fn status(&self, recurse_untracked: bool) -> Result<RepoStatus> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(recurse_untracked)
            .include_ignored(false);

        let statuses = self.repo.statuses(Some(&mut opts))?;

        let mut staged = Vec::new();
        let mut unstaged = Vec::new();
        let mut untracked = Vec::new();

        for entry in statuses.iter() {
            let path = entry.path().unwrap_or("").to_string();
            let st = entry.status();

            if st.is_index_new() {
                staged.push(FileStatus { path: path.clone(), status: FileStatusKind::New });
            } else if st.is_index_modified() {
                staged.push(FileStatus { path: path.clone(), status: FileStatusKind::Modified });
            } else if st.is_index_deleted() {
                staged.push(FileStatus { path: path.clone(), status: FileStatusKind::Deleted });
            } else if st.is_index_renamed() {
                staged.push(FileStatus { path: path.clone(), status: FileStatusKind::Renamed });
            } else if st.is_index_typechange() {
                staged.push(FileStatus { path: path.clone(), status: FileStatusKind::Typechange });
            }

            if st.is_wt_modified() {
                unstaged.push(FileStatus { path: path.clone(), status: FileStatusKind::Modified });
            } else if st.is_wt_deleted() {
                unstaged.push(FileStatus { path: path.clone(), status: FileStatusKind::Deleted });
            } else if st.is_wt_renamed() {
                unstaged.push(FileStatus { path: path.clone(), status: FileStatusKind::Renamed });
            } else if st.is_wt_typechange() {
                unstaged.push(FileStatus { path: path.clone(), status: FileStatusKind::Typechange });
            }

            if st.is_wt_new() {
                untracked.push(path);
            }
        }

        Ok(RepoStatus {
            staged,
            unstaged,
            untracked,
        })
    }

    pub fn current_branch_name(&self) -> Option<String> {
        let head = self.repo.head().ok()?;
        head.shorthand().map(String::from)
    }

    pub fn ahead_behind(&self) -> Result<(usize, usize)> {
        let head = self.repo.head()?;
        let local_oid = head.target().context("HEAD has no target")?;

        let branch_name = head.shorthand().context("No branch name")?;
        let upstream_name = format!("refs/remotes/origin/{}", branch_name);

        let upstream_ref = self.repo.find_reference(&upstream_name);
        match upstream_ref {
            Ok(upstream) => {
                let remote_oid = upstream.target().context("Upstream has no target")?;
                let (ahead, behind) = self.repo.graph_ahead_behind(local_oid, remote_oid)?;
                Ok((ahead, behind))
            }
            Err(_) => Ok((0, 0)),
        }
    }

    pub fn log(&self, max_count: usize) -> Result<Vec<CommitInfo>> {
        self.log_page(0, max_count)
    }

    /// Load a page of commits, skipping the first `skip` entries.
    pub fn log_page(&self, skip: usize, max_count: usize) -> Result<Vec<CommitInfo>> {
        let mut revwalk = self.repo.revwalk()?;
        revwalk.push_head()?;
        revwalk.set_sorting(git2::Sort::TIME)?;

        let mut commits = Vec::new();

        for (i, oid) in revwalk.enumerate() {
            if i < skip {
                continue;
            }
            if commits.len() >= max_count {
                break;
            }

            let oid = oid?;
            let commit = self.repo.find_commit(oid)?;
            commits.push(commit_to_info_lite(&commit));
        }

        Ok(commits)
    }

    /// Read the reflog for HEAD. Most recent first.
    pub fn reflog(&self, max_count: usize) -> Result<Vec<crate::model::ReflogEntry>> {
        let reflog = self.repo.reflog("HEAD").context("Failed to read reflog")?;
        let mut entries = Vec::new();
        for i in 0..reflog.len() {
            if entries.len() >= max_count {
                break;
            }
            let Some(entry) = reflog.get(i) else { continue };
            let new_id = entry.id_new().to_string();
            let short_new = new_id[..7.min(new_id.len())].to_string();
            entries.push(crate::model::ReflogEntry {
                old_id: entry.id_old().to_string(),
                new_id,
                short_new,
                committer: signature_to_model(&entry.committer()),
                time: entry.committer().when().seconds(),
                message: entry.message().unwrap_or("").to_string(),
            });
        }
        Ok(entries)
    }

    /// List configured remotes.
    pub fn remotes(&self) -> Result<Vec<crate::model::RemoteInfo>> {
        let names = self.repo.remotes().context("Failed to list remotes")?;
        let mut result = Vec::new();
        for name in names.iter().flatten() {
            let url = self
                .repo
                .find_remote(name)
                .ok()
                .and_then(|r| r.url().map(|s| s.to_string()))
                .unwrap_or_default();
            result.push(crate::model::RemoteInfo {
                name: name.to_string(),
                url,
            });
        }
        Ok(result)
    }

    pub fn add_remote(&self, name: &str, url: &str) -> Result<()> {
        self.repo
            .remote(name, url)
            .context("Failed to add remote")?;
        Ok(())
    }

    pub fn remove_remote(&self, name: &str) -> Result<()> {
        self.repo
            .remote_delete(name)
            .context("Failed to remove remote")?;
        Ok(())
    }

    pub fn rename_remote(&self, old: &str, new: &str) -> Result<()> {
        self.repo
            .remote_rename(old, new)
            .context("Failed to rename remote")?;
        Ok(())
    }

    pub fn set_remote_url(&self, name: &str, url: &str) -> Result<()> {
        self.repo
            .remote_set_url(name, url)
            .context("Failed to set remote URL")?;
        Ok(())
    }

    /// History of commits that touched a specific file path.
    pub fn log_for_file(&self, file_path: &str, max_count: usize) -> Result<Vec<CommitInfo>> {
        let mut revwalk = self.repo.revwalk()?;
        revwalk.push_head()?;
        revwalk.set_sorting(git2::Sort::TIME)?;

        let target = std::path::Path::new(file_path);
        let mut commits = Vec::new();

        for oid in revwalk {
            if commits.len() >= max_count {
                break;
            }
            let oid = oid?;
            let commit = self.repo.find_commit(oid)?;

            // Diff against each parent (or empty tree for root commit)
            let tree = commit.tree()?;
            let touches = if commit.parent_count() == 0 {
                let diff = self.repo.diff_tree_to_tree(None, Some(&tree), None)?;
                diff_touches_path(&diff, target)
            } else {
                let mut touched = false;
                for i in 0..commit.parent_count() {
                    let parent = commit.parent(i)?;
                    let parent_tree = parent.tree()?;
                    let diff = self.repo.diff_tree_to_tree(
                        Some(&parent_tree),
                        Some(&tree),
                        None,
                    )?;
                    if diff_touches_path(&diff, target) {
                        touched = true;
                        break;
                    }
                }
                touched
            };

            if touches {
                commits.push(commit_to_info(&self.repo, &commit));
            }
        }

        Ok(commits)
    }

    pub fn branches(&self) -> Result<Vec<BranchInfo>> {
        let mut result = Vec::new();
        let head_ref = self.repo.head().ok();
        let head_oid = head_ref.as_ref().and_then(|h| h.target());

        for branch_type in &[BranchType::Local, BranchType::Remote] {
            let branches = self.repo.branches(Some(*branch_type))?;
            for branch in branches {
                let (branch, _) = branch?;
                let name = branch.name()?.unwrap_or("").to_string();

                let is_head = branch
                    .get()
                    .target()
                    .map(|oid| Some(oid) == head_oid)
                    .unwrap_or(false);

                // Only compute ahead/behind for the HEAD branch to avoid
                // expensive graph walks for every local branch.
                let (ahead, behind) = if is_head && *branch_type == BranchType::Local {
                    match branch.upstream() {
                        Ok(upstream) => {
                            let local_oid = branch.get().target();
                            let remote_oid = upstream.get().target();
                            match (local_oid, remote_oid) {
                                (Some(l), Some(r)) => {
                                    self.repo.graph_ahead_behind(l, r).unwrap_or((0, 0))
                                }
                                _ => (0, 0),
                            }
                        }
                        Err(_) => (0, 0),
                    }
                } else {
                    (0, 0)
                };

                let upstream = if *branch_type == BranchType::Local {
                    branch
                        .upstream()
                        .ok()
                        .and_then(|u| u.name().ok().flatten().map(String::from))
                } else {
                    None
                };

                result.push(BranchInfo {
                    name,
                    is_head,
                    is_remote: *branch_type == BranchType::Remote,
                    upstream,
                    ahead,
                    behind,
                });
            }
        }

        Ok(result)
    }

    /// Quick dirty check without parsing individual file statuses.
    pub fn is_dirty_quick(&self) -> bool {
        let (tracked, untracked) = self.dirty_kinds();
        tracked || untracked
    }

    /// Returns (has_tracked_changes, has_untracked_only).
    /// Tracked = modified/staged/deleted files. Untracked = new files only.
    pub fn dirty_kinds(&self) -> (bool, bool) {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(false)
            .include_ignored(false);
        let statuses = match self.repo.statuses(Some(&mut opts)) {
            Ok(s) => s,
            Err(_) => return (false, false),
        };
        let mut tracked = false;
        let mut untracked = false;
        for entry in statuses.iter() {
            let s = entry.status();
            if s.is_wt_new() {
                untracked = true;
            } else if !s.is_empty() {
                tracked = true;
            }
        }
        (tracked, untracked)
    }

    pub fn inner(&self) -> &Repository {
        &self.repo
    }

    pub fn inner_mut(&mut self) -> &mut Repository {
        &mut self.repo
    }

    /// Cheap signature presence check for a single commit. Called lazily by the
    /// UI when a commit row is expanded; the log_page hot path no longer pays
    /// this cost up front.
    pub fn commit_is_signed(&self, oid_hex: &str) -> bool {
        let Ok(oid) = git2::Oid::from_str(oid_hex) else {
            return false;
        };
        self.repo.extract_signature(&oid, None).is_ok()
    }
}

fn git_time_to_datetime(time: git2::Time) -> DateTime<Utc> {
    Utc.timestamp_opt(time.seconds(), 0)
        .single()
        .unwrap_or_default()
}

fn signature_to_model(sig: &git2::Signature<'_>) -> Signature {
    Signature {
        name: sig.name().unwrap_or("").to_string(),
        email: sig.email().unwrap_or("").to_string(),
    }
}

fn diff_touches_path(diff: &git2::Diff<'_>, target: &std::path::Path) -> bool {
    for delta in diff.deltas() {
        let old_matches = delta.old_file().path().map(|p| p == target).unwrap_or(false);
        let new_matches = delta.new_file().path().map(|p| p == target).unwrap_or(false);
        if old_matches || new_matches {
            return true;
        }
    }
    false
}

fn commit_to_info(repo: &Repository, commit: &git2::Commit<'_>) -> CommitInfo {
    let id = commit.id().to_string();
    let short_id = id[..7.min(id.len())].to_string();
    let is_signed = repo.extract_signature(&commit.id(), None).is_ok();

    CommitInfo {
        id,
        short_id,
        summary: commit.summary().unwrap_or("").to_string(),
        message: commit.message().unwrap_or("").to_string(),
        author: signature_to_model(&commit.author()),
        committer: signature_to_model(&commit.committer()),
        time: git_time_to_datetime(commit.time()),
        parent_ids: commit.parent_ids().map(|oid| oid.to_string()).collect(),
        is_signed,
    }
}

/// Same as `commit_to_info` but skips the signature check (which opens the
/// object database and is the hot-path cost when paginating large logs).
/// Callers that need `is_signed` set should look it up lazily on demand.
fn commit_to_info_lite(commit: &git2::Commit<'_>) -> CommitInfo {
    let id = commit.id().to_string();
    let short_id = id[..7.min(id.len())].to_string();
    CommitInfo {
        id,
        short_id,
        summary: commit.summary().unwrap_or("").to_string(),
        message: commit.message().unwrap_or("").to_string(),
        author: signature_to_model(&commit.author()),
        committer: signature_to_model(&commit.committer()),
        time: git_time_to_datetime(commit.time()),
        parent_ids: commit.parent_ids().map(|oid| oid.to_string()).collect(),
        is_signed: false,
    }
}
