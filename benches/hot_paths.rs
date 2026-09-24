//! Criterion baselines for the hot pure-logic paths (GOAL 0.1).
//!
//! These functions run on background threads during every refresh, so their
//! cost directly sets the auto-refresh latency on big repos. Record the first
//! run's numbers in GOAL.md as the pre-optimisation baseline, and re-run
//! after each Phase 3 change to check for regressions (>10% = regression).
//!
//! Run: `cargo bench`  (single bench: `cargo bench -- bench_name_fragment`)

use criterion::{Criterion, criterion_group, criterion_main};
use gitpanel::i18n::{self, Key};
use gitpanel::utils::repository::GitRepo;
use gitpanel::widgets::changes_view::{ChangedFileEntry, collect_changed_files};

/// Open this repo once — bench bodies must not pay open cost.
fn open_repo() -> GitRepo {
    GitRepo::open(env!("CARGO_MANIFEST_DIR")).expect("open gitpanel repo")
}

/// status() is the probe behind every background refresh.
fn bench_status(c: &mut Criterion) {
    let repo = open_repo();
    c.bench_function("status_collapsed", |b| {
        b.iter(|| repo.status(false).expect("status"))
    });
    c.bench_function("status_recursive", |b| {
        b.iter(|| repo.status(true).expect("status"))
    });
}

/// Full unstaged+staged diff recompute happens whenever the status hash moves.
fn bench_diff(c: &mut Criterion) {
    let repo = open_repo();
    c.bench_function("diff_unstaged", |b| {
        b.iter(|| repo.diff_unstaged().expect("diff"))
    });
    c.bench_function("diff_staged", |b| {
        b.iter(|| repo.diff_staged().expect("diff"))
    });
}

/// Commit list fill on repo open (log_all_page is the heavy walk).
fn bench_log(c: &mut Criterion) {
    let repo = open_repo();
    c.bench_function("log_50", |b| {
        b.iter(|| repo.log(50).expect("log"))
    });
}

/// i18n lookup runs per row render.
fn bench_i18n(c: &mut Criterion) {
    i18n::init_language(gitpanel::i18n::Language::ZhCn);
    let keys = [
        Key::changes,
        Key::no_changes,
        Key::binary_diff_not_supported,
        Key::status_modified,
        Key::menu_fetch,
    ];
    c.bench_function("i18n_t_5_keys", |b| {
        b.iter(|| keys.iter().map(|k| i18n::t(*k)).count())
    });
}

/// Status → row list conversion, once per refresh.
fn bench_collect(c: &mut Criterion) {
    let files: Vec<ChangedFileEntry> = (0..500)
        .map(|i| ChangedFileEntry {
            path: format!("src/module_{i}/file_{i}.rs"),
            status: gitpanel::model::FileStatusKind::Modified,
            is_staged: i % 3 == 0,
        })
        .collect();
    c.bench_function("collect_changed_files_500", |b| {
        b.iter(|| collect_changed_files_files(&files))
    });
}

/// collect_changed_files takes &RepoStatus; build one from the row entries so
/// the bench measures the conversion, not RepoStatus construction.
fn collect_changed_files_files(files: &[ChangedFileEntry]) -> Vec<ChangedFileEntry> {
    use gitpanel::model::{FileStatus, RepoStatus};
    let status = RepoStatus {
        staged: files
            .iter()
            .filter(|f| f.is_staged)
            .map(|f| FileStatus {
                path: f.path.clone(),
                status: f.status,
            })
            .collect(),
        unstaged: files
            .iter()
            .filter(|f| !f.is_staged)
            .map(|f| FileStatus {
                path: f.path.clone(),
                status: f.status,
            })
            .collect(),
        untracked: vec![],
    };
    collect_changed_files(&status)
}

criterion_group!(benches, bench_status, bench_diff, bench_log, bench_i18n, bench_collect);
criterion_main!(benches);
