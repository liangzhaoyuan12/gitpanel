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
