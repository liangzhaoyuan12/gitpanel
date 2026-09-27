//! 端到端：`execute_rebase` 真实驱动 `git rebase -i`，
//! 验证 todo 注入机制（env 传递 → 序列编辑器拷贝 → 重排生效）。

use std::path::Path;

use git2::{Repository, Signature, Time};
use serial_test::serial;
use tempfile::TempDir;

/// 与 `common::make_repo_with_commits` 不同：每个提交写**不同**文件，
/// 这样重排不会在同一行上撞三方合并冲突。
fn make_disjoint_repo() -> (TempDir, gitpanel::utils::repository::GitRepo) {
    let dir = tempfile::tempdir().expect("tempdir");
    let raw = Repository::init(dir.path()).expect("git init");
    let files = ["base.txt", "a.txt", "b.txt", "c.txt"];
    for (i, f) in files.iter().enumerate() {
        std::fs::write(dir.path().join(f), format!("{i}\n")).unwrap();
        let mut index = raw.index().unwrap();
        index.add_path(Path::new(f)).unwrap();
        index.write().unwrap();
        let tree = raw.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = raw
            .head()
            .ok()
            .and_then(|h| h.target())
            .map(|o| raw.find_commit(o).unwrap());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        // 秒级时间戳必须逐提交递增，否则 Sort::TIME 下 revwalk 顺序不定
        // （与 common::make_repo_with_commits 同样的坑）。
        let sig = Signature::new(
            "Test",
            "test@example.com",
            &Time::new(1_700_000_000 + i as i64, 0),
        )
        .unwrap();
        raw.commit(Some("HEAD"), &sig, &sig, f, &tree, &parents)
            .unwrap();
    }
    let repo =
        gitpanel::utils::repository::GitRepo::open(dir.path().to_str().expect("utf8")).unwrap();
    (dir, repo)
}

#[test]
#[serial] // 会改进程级环境变量 GP_SEQUENCE_EDITOR
fn execute_rebase_applies_the_prepared_todo() {
    let (_dir, repo) = make_disjoint_repo();

    // oldest-first：[base, a, b, c]，取最近 3 个 = [a, b, c]。
    let mut entries = repo.list_rebase_commits(3).expect("entries");
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].message, "a.txt");
    assert_eq!(entries[2].message, "c.txt");

    // 默认 todo = pick a pick b pick c；我们改成 b a c 重排。
    entries.swap(0, 1);

    // 用覆盖钩子喂一个等价的“拷贝型”序列编辑器：源路径由我们经 env
    // 传给 git，与生产路径同一套 env 管道，只把“我们 exe 的引号+再入”
    // 换成 sh 内联（我们 exe 路径的引号逻辑已由 rebase.rs 单测覆盖）。
    std::env::set_var("GP_SEQUENCE_EDITOR", r#"cp "$GP_SEQ_TODO_SRC""#);
    let result = repo.execute_rebase(&entries, "HEAD~3");
    std::env::remove_var("GP_SEQUENCE_EDITOR");
    result.expect("rebase should succeed");

    let log = repo.log_page(0, 10).expect("log");
    let summaries: Vec<&str> = log.iter().map(|c| c.summary.as_str()).collect();
    // newest-first：重排后顶上是 c，然后 a/b 互换，最后 base。
    assert_eq!(summaries, vec!["c.txt", "a.txt", "b.txt", "base.txt"]);
}

#[test]
#[serial]
fn execute_rebase_reports_git_failure() {
    let (_dir, repo) = make_disjoint_repo();

    // 不存在的 onto → git 报错，错误信息应冒泡成 Err 而不是 panic。
    let entries = repo.list_rebase_commits(1).expect("entries");
    std::env::set_var("GP_SEQUENCE_EDITOR", r#"cp "$GP_SEQ_TODO_SRC""#);
    let result = repo.execute_rebase(&entries, "no-such-ref");
    std::env::remove_var("GP_SEQUENCE_EDITOR");

    let err = result.expect_err("bad ref must fail");
    assert!(!err.to_string().trim().is_empty());
}
