use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub message: String,
    pub author: Signature,
    pub committer: Signature,
    pub time: DateTime<Utc>,
    pub parent_ids: Vec<String>,
    #[serde(default)]
    pub is_signed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub is_head: bool,
    pub is_remote: bool,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoStatus {
    pub staged: Vec<FileStatus>,
    pub unstaged: Vec<FileStatus>,
    pub untracked: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStatus {
    pub path: String,
    pub status: FileStatusKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FileStatusKind {
    #[default]
    New,
    Modified,
    Deleted,
    Renamed,
    Typechange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffFile {
    pub path: String,
    pub hunks: Vec<DiffHunk>,
    pub stats: DiffStats,
    #[serde(default)]
    pub is_binary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffLineKind {
    Context,
    Addition,
    Deletion,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiffStats {
    pub insertions: usize,
    pub deletions: usize,
}

impl DiffFile {
    /// Estimated memory footprint of this diff in bytes (line content + struct overhead).
    pub fn estimated_bytes(&self) -> usize {
        let mut total = self.path.len() + std::mem::size_of::<DiffFile>();
        for h in &self.hunks {
            total += h.header.len() + std::mem::size_of::<DiffHunk>();
            for l in &h.lines {
                total += l.content.len() + std::mem::size_of::<DiffLine>();
            }
        }
        total
    }
}

/// Truncate a diff cache so its total estimated size stays under `max_bytes`.
/// Entries are kept in original order; trailing entries that push the cache
/// over the cap are dropped. Per-file diffs larger than the cap are kept
/// alone so that file's diff is still cached.
pub fn cap_diff_cache(diffs: &mut Vec<DiffFile>, max_bytes: usize) {
    if max_bytes == 0 {
        return;
    }
    let mut acc: usize = 0;
    let mut keep: usize = 0;
    for f in diffs.iter() {
        let size = f.estimated_bytes();
        let next = acc.saturating_add(size);
        if keep > 0 && next > max_bytes {
            break;
        }
        acc = next;
        keep += 1;
    }
    if keep < diffs.len() {
        diffs.truncate(keep);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StashEntry {
    pub index: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagInfo {
    pub name: String,
    pub target_id: String,
    pub is_annotated: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmoduleInfo {
    pub name: String,
    pub path: String,
    pub url: String,
    pub head_id: Option<String>,
    pub is_initialized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeInfo {
    pub name: String,
    pub path: String,
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictFile {
    pub path: String,
    pub ancestor: Option<Vec<u8>>,
    pub ours: Option<Vec<u8>>,
    pub theirs: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RebaseAction {
    Pick,
    Squash,
    Fixup,
    Reword,
    Edit,
    Drop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebaseEntry {
    pub commit_id: String,
    pub short_id: String,
    pub message: String,
    pub action: RebaseAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlameLine {
    pub line_no: usize,
    pub content: String,
    pub commit_id: String,
    pub short_id: String,
    pub author: String,
    pub time: i64,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflogEntry {
    pub old_id: String,
    pub new_id: String,
    pub short_new: String,
    pub committer: Signature,
    pub time: i64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteInfo {
    pub name: String,
    pub url: String,
}

/// Where a remote-tracking branch currently sits, relative to `HEAD`.
///
/// One entry is produced per remote-tracking ref so the commit list and the
/// graph can decorate the commit that ref points at — the same information
/// VS Code shows as `origin/main` markers in its commit graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteRefPos {
    /// Remote name, e.g. `origin`.
    pub remote: String,
    /// Branch name without the remote prefix, e.g. `main`.
    pub branch: String,
    /// Ref label shown in the UI, e.g. `origin/main`.
    pub label: String,
    /// Commit the remote-tracking ref points at. Empty when the remote has
    /// never been fetched.
    pub commit_id: String,
    /// Commits reachable from `HEAD` but not from the remote tip.
    pub ahead: usize,
    /// Commits reachable from the remote tip but not from `HEAD`.
    pub behind: usize,
    /// True when the branch name matches the checked-out branch — i.e. the ref
    /// push/pull would sync.
    pub is_tracking: bool,
}

impl RemoteRefPos {
    /// Human readable status used for tooltips, e.g. `2 ahead, 1 behind`.
    ///
    /// The badge itself stays plain (`origin/main`, like VS Code) — where the
    /// remote sits is told by *which* commit carries it, so the divergence only
    /// needs spelling out on hover.
    pub fn status_text(&self) -> String {
        match (self.ahead, self.behind) {
            (0, 0) => "in sync with HEAD".to_string(),
            (a, 0) => format!("{a} commit(s) ahead of this remote"),
            (0, b) => format!("{b} commit(s) behind this remote"),
            (a, b) => format!("diverged: {a} ahead, {b} behind"),
        }
    }
}

/// A local branch and the commit it points at.
///
/// Local branches are reported the same way as remote-tracking refs so a commit
/// can carry both kinds of marker — a local `main` and an `origin/main` sitting
/// on the same commit get one badge each, exactly like VS Code's graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalRefPos {
    /// Branch name as git reports it, e.g. `main`, `master`, `feature/x`.
    /// Never hardcoded to a default branch name.
    pub branch: String,
    /// Ref label shown in the UI, e.g. `local/main`.
    pub label: String,
    /// Commit the branch points at.
    pub commit_id: String,
    /// True for the branch `HEAD` is on.
    pub is_head: bool,
}

/// Which kind of ref a badge marks. Local and remote markers are styled
/// differently so the two are told apart at a glance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefKind {
    Local,
    Remote,
}

/// One decoration rendered next to a commit: the ref label plus its kind.
///
/// Both kinds carry hover text: a remote badge is a bare `origin/main` (VS Code
/// style) and cannot say on its own how far ahead or behind it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefBadge {
    pub label: String,
    pub kind: RefKind,
    /// Tooltip text — the pill is a bare ref name, so the detail goes here.
    pub tooltip: String,
}

impl RefBadge {
    pub fn local(branch: &str) -> Self {
        Self {
            label: format!("local/{branch}"),
            kind: RefKind::Local,
            tooltip: format!("Local branch {branch}"),
        }
    }

    pub fn remote(pos: &RemoteRefPos) -> Self {
        Self {
            // Plain ref name, VS Code style: the counts that used to be appended
            // here are in `tooltip` instead.
            label: pos.label.clone(),
            kind: RefKind::Remote,
            tooltip: format!("{} — {}", pos.label, pos.status_text()),
        }
    }
}
