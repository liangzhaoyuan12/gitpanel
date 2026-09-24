//! One-shot timing probe for any repo path (GOAL 0.3 companion).
//!
//! Usage: `cargo run --release --example repo_probe /path/to/repo`
//!
//! Prints wall-clock timings for the same calls the background refresh makes,
//! so big-repo baselines and Phase 3 before/after comparisons come from the
//! real code paths, not `git` CLI approximations.

use std::time::Instant;

use gitpanel::utils::repository::GitRepo;

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| ".".to_string());

    let t = Instant::now();
    let repo = GitRepo::open(&path).expect("open repo");
    println!("open:            {:>10.2?}", t.elapsed());

    let t = Instant::now();
    let status = repo.status(false).expect("status");
    println!(
        "status(collapsed):{:>10.2?}  ({} staged, {} unstaged, {} untracked)",
        t.elapsed(),
        status.staged.len(),
        status.unstaged.len(),
        status.untracked.len()
    );

    let t = Instant::now();
    let status = repo.status(true).expect("status");
    println!("status(recursive):{:>10.2?}  ({} entries)", t.elapsed(),
        status.staged.len() + status.unstaged.len() + status.untracked.len());

    let t = Instant::now();
    let diffs = repo.diff_unstaged().expect("diff");
    println!("diff_unstaged:   {:>10.2?}  ({} files)", t.elapsed(), diffs.len());

    let t = Instant::now();
    let diffs = repo.diff_staged().expect("diff");
    println!("diff_staged:     {:>10.2?}  ({} files)", t.elapsed(), diffs.len());

    let t = Instant::now();
    let commits = repo.log(50).expect("log");
    println!("log(50):         {:>10.2?}  ({} commits)", t.elapsed(), commits.len());

    let t = Instant::now();
    let commits = repo.log_all_page(0, 50).expect("log_all_page");
    println!("log_all_page(50):{:>10.2?}  ({} commits)", t.elapsed(), commits.len());

    let t = Instant::now();
    let tags = repo.tags_by_commit().unwrap_or_default();
    println!("tags_by_commit:  {:>10.2?}  ({} tagged commits)", t.elapsed(), tags.len());

    let t = Instant::now();
    let (ahead, behind) = repo.ahead_behind().unwrap_or((0, 0));
    println!("ahead_behind:    {:>10.2?}  ({} / {})", t.elapsed(), ahead, behind);
}
