mod common;

use common::make_repo_with_commits;

#[test]
fn log_returns_requested_count() {
    let tr = make_repo_with_commits(10);
    let commits = tr.repo.log(5).expect("log");
    assert_eq!(commits.len(), 5);
    assert_eq!(commits[0].summary, "commit 9"); // newest first
}

#[test]
fn log_page_skips_correctly() {
    let tr = make_repo_with_commits(10);
    let page = tr.repo.log_page(3, 4).expect("log_page");
    assert_eq!(page.len(), 4);
    assert_eq!(page[0].summary, "commit 6");
    assert_eq!(page[3].summary, "commit 3");
}

#[test]
fn log_page_handles_eof() {
    let tr = make_repo_with_commits(5);
    let page = tr.repo.log_page(3, 50).expect("log_page");
    assert_eq!(page.len(), 2); // only commits 1, 0 remain
}

#[test]
fn log_page_zero_skip_matches_log() {
    let tr = make_repo_with_commits(7);
    let a = tr.repo.log(7).expect("log");
    let b = tr.repo.log_page(0, 7).expect("log_page");
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b.iter()) {
        assert_eq!(x.id, y.id);
        assert_eq!(x.summary, y.summary);
    }
}

// FIXME(task-2): re-enable once commit_to_info_lite lands and log_page switches
// to it (so signature lookup is skipped on the log hot path).
// #[test]
// fn log_page_lite_skips_signature() {
//     let tr = make_repo_with_commits(3);
//     let commits = tr.repo.log_page(0, 3).expect("log_page");
//     // log_page uses commit_to_info_lite — is_signed must be false even
//     // though git2 *could* report signed state if checked.
//     for c in &commits {
//         assert!(!c.is_signed, "log_page must not pay the signature cost");
//     }
// }

// FIXME(task-2): re-enable once GitRepo::commit_is_signed is introduced.
// #[test]
// fn commit_is_signed_is_false_for_unsigned_commits() {
//     let tr = make_repo_with_commits(2);
//     let commits = tr.repo.log(2).expect("log");
//     let oid = &commits[0].id;
//     assert!(!tr.repo.commit_is_signed(oid));
// }
//
// #[test]
// fn commit_is_signed_handles_garbage_oid() {
//     let tr = make_repo_with_commits(1);
//     assert!(!tr.repo.commit_is_signed("not-a-real-oid"));
//     assert!(!tr.repo.commit_is_signed("deadbeef"));
// }
