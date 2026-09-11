//! Shared helpers for integration tests. Spawns a tmp git repository,
//! authors deterministic commits, and yields a GitRepo handle.

use std::path::Path;

use git2::{Repository, Signature, Time};
use tempfile::TempDir;

use gitpanel::utils::repository::GitRepo;

pub struct TmpRepo {
    // Kept alive so the underlying directory persists for the test's lifetime;
    // tests interact only through `repo`.
    #[allow(dead_code)]
    pub dir: TempDir,
    pub repo: GitRepo,
}

pub fn make_repo_with_commits(n: usize) -> TmpRepo {
    let dir = tempfile::tempdir().expect("tempdir");
    let raw = Repository::init(dir.path()).expect("git init");

    // Use a fixed base epoch + per-commit offset so `Sort::TIME` produces a
    // deterministic newest-first ordering. `Signature::now` would assign the
    // same second-resolution timestamp to every commit and the revwalk order
    // would become indeterminate.
    let base: i64 = 1_700_000_000;
    let mut parent_oid: Option<git2::Oid> = None;
    for i in 0..n {
        std::fs::write(dir.path().join("README"), format!("v{i}\n")).expect("write");
        let mut index = raw.index().expect("index");
        index.add_path(Path::new("README")).expect("add");
        index.write().expect("write index");
        let tree_oid = index.write_tree().expect("write tree");
        let tree = raw.find_tree(tree_oid).expect("find tree");
        let parents: Vec<git2::Commit<'_>> = parent_oid
            .iter()
            .map(|oid| raw.find_commit(*oid).expect("parent"))
            .collect();
        let parent_refs: Vec<&git2::Commit<'_>> = parents.iter().collect();
        let when = Time::new(base + i as i64, 0);
        let sig = Signature::new("Test", "test@example.com", &when).expect("sig");
        let oid = raw
            .commit(Some("HEAD"), &sig, &sig, &format!("commit {i}"), &tree, &parent_refs)
            .expect("commit");
        parent_oid = Some(oid);
    }

    let repo = GitRepo::open(dir.path().to_str().expect("utf8 path")).expect("open");
    TmpRepo { dir, repo }
}
