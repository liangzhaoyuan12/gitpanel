use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use git2::{BranchType, Repository, StatusOptions};

use crate::models::*;

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

    pub fn status(&self) -> Result<RepoStatus> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
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
        let mut revwalk = self.repo.revwalk()?;
        revwalk.push_head()?;
        revwalk.set_sorting(git2::Sort::TIME)?;

        let mut commits = Vec::new();

        for (i, oid) in revwalk.enumerate() {
            if i >= max_count {
                break;
            }

            let oid = oid?;
            let commit = self.repo.find_commit(oid)?;
            commits.push(commit_to_info(&commit));
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

                let (ahead, behind) = if *branch_type == BranchType::Local {
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
        let mut opts = StatusOptions::new();
        opts.include_untracked(false)
            .include_ignored(false);
        self.repo
            .statuses(Some(&mut opts))
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    }

    pub fn inner(&self) -> &Repository {
        &self.repo
    }

    pub fn inner_mut(&mut self) -> &mut Repository {
        &mut self.repo
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

fn commit_to_info(commit: &git2::Commit<'_>) -> CommitInfo {
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
    }
}
