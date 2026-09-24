//! Binary-file diff reporting: every diff entry point must set `is_binary`
//! so the UI can show "binary diff is not supported" instead of rendering
//! Regression tests for the untracked-file path that used to fail
//! silently (`read_to_string` error swallowed by the caller).

use std::path::Path;

use git2::Repository;
use gitpanel::utils::repository::GitRepo;
use tempfile::TempDir;

/// Bytes that are unambiguously binary (contain NUL, like git's own heuristic).
const BINARY_BYTES: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0xff, 0xfe];

struct Fixture {
    #[allow(dead_code)]
    dir: TempDir,
    repo: GitRepo,
}

fn init_repo() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    Repository::init(dir.path()).expect("git init");
    let repo = GitRepo::open(dir.path().to_str().expect("utf8 path")).expect("open");
    Fixture { dir, repo }
}

/// Commit everything currently written in the workdir; returns the commit id.
fn commit_all(repo: &GitRepo, message: &str) -> String {
    repo.stage_all().expect("stage_all");
    // `commit` uses repo.signature(); give the temp repo its own identity so
    // the test does not depend on the host's git config.
    {
        let raw = repo.inner();
        let mut cfg = raw.config().expect("config");
        cfg.set_str("user.name", "Test").expect("user.name");
        cfg.set_str("user.email", "test@example.com").expect("user.email");
    }
    repo.commit(message).expect("commit")
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).expect("write file");
}

/// Path 2 of 3: an untracked binary file must yield `is_binary == true`.
#[test]
fn untracked_binary_file_is_flagged_binary() {
    let fx = init_repo();
    write(&fx.dir.path().join("logo.png"), BINARY_BYTES);

    let diff = fx.repo.diff_untracked("logo.png").expect("diff_untracked");
    assert!(diff.is_binary, "untracked binary file must be flagged");
    assert!(diff.hunks.is_empty(), "binary file has no hunks to render");
}

/// Control for the test above: an untracked *text* file must NOT be flagged
/// and must still produce a real diff after the bytes-based rewrite.
#[test]
fn untracked_text_file_is_not_flagged_binary() {
    let fx = init_repo();
    write(&fx.dir.path().join("notes.txt"), b"hello\nworld\n");

    let diff = fx.repo.diff_untracked("notes.txt").expect("diff_untracked");
    assert!(!diff.is_binary, "text file must not be flagged binary");
    assert_eq!(diff.stats.insertions, 2, "both lines are additions");
}

/// Path 1 of 3: a tracked binary file modified in the workdir.
#[test]
fn modified_tracked_binary_file_is_flagged_binary() {
    let fx = init_repo();
    write(&fx.dir.path().join("data.bin"), BINARY_BYTES);
    commit_all(&fx.repo, "add binary");

    let changed: &[u8] = &[0x00, 0x01, 0x02, 0x03, 0xde, 0xad];
    write(&fx.dir.path().join("data.bin"), changed);

    let diffs = fx.repo.diff_unstaged().expect("diff_unstaged");
    let entry = diffs
        .iter()
        .find(|f| f.path == "data.bin")
        .expect("changed binary file must appear in the diff list");
    assert!(entry.is_binary, "modified tracked binary must be flagged");
}

/// Path 1b: the same file after staging.
#[test]
fn staged_binary_file_is_flagged_binary() {
    let fx = init_repo();
    write(&fx.dir.path().join("data.bin"), BINARY_BYTES);
    commit_all(&fx.repo, "add binary");

    write(&fx.dir.path().join("data.bin"), &[0x00, 0x99, 0x88]);
    fx.repo.stage_file("data.bin").expect("stage");

    let diffs = fx.repo.diff_staged().expect("diff_staged");
    let entry = diffs
        .iter()
        .find(|f| f.path == "data.bin")
        .expect("staged binary file must appear in the diff list");
    assert!(entry.is_binary, "staged binary must be flagged");
}

/// Path 3 of 3: a commit that changes a binary file.
#[test]
fn commit_changing_binary_file_is_flagged_binary() {
    let fx = init_repo();
    write(&fx.dir.path().join("data.bin"), BINARY_BYTES);
    commit_all(&fx.repo, "add binary");

    write(&fx.dir.path().join("data.bin"), &[0x00, 0x11, 0x22, 0x33]);
    let second = commit_all(&fx.repo, "change binary");

    let diffs = fx.repo.diff_commit(&second).expect("diff_commit");
    let entry = diffs
        .iter()
        .find(|f| f.path == "data.bin")
        .expect("binary file must appear in the commit diff");
    assert!(entry.is_binary, "commit diff must flag the binary file");
}

/// Mixed diff: a binary and a text file changed together — the text file must
/// keep its hunks while the binary one stays flagged (seeding in parse_diff
/// must not corrupt neighbouring entries).
#[test]
fn mixed_binary_and_text_diffs_are_independent() {
    let fx = init_repo();
    write(&fx.dir.path().join("readme.txt"), b"one\n");
    write(&fx.dir.path().join("data.bin"), BINARY_BYTES);
    commit_all(&fx.repo, "initial");

    write(&fx.dir.path().join("readme.txt"), b"one\ntwo\n");
    write(&fx.dir.path().join("data.bin"), &[0x00, 0x02]);

    let diffs = fx.repo.diff_unstaged().expect("diff_unstaged");
    let text = diffs.iter().find(|f| f.path == "readme.txt").expect("text");
    let bin = diffs.iter().find(|f| f.path == "data.bin").expect("binary");
    assert!(!text.is_binary && text.stats.insertions == 1, "text diff intact");
    assert!(bin.is_binary, "binary flagged alongside text");
    assert_eq!(diffs.len(), 2, "exactly the two changed files");
}