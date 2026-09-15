//! Ref positions: every ref must report where *it* is, commits that exist only
//! on a remote must reach the commit list, and the decorations must land on the
//! commit each ref points at — a local branch and a remote branch on the same
//! commit get one badge each.

mod common;

use std::path::Path;

use git2::{Oid, Repository, Signature, Time};

use gitpanel::model::RefKind;
use gitpanel::utils::remote::ref_badge_map;

fn add_remote(raw: &Repository, name: &str) {
    raw.remote(name, &format!("https://example.com/{name}.git"))
        .expect("add remote");
}

fn set_remote_ref(raw: &Repository, remote: &str, branch: &str, oid: Oid) {
    raw.reference(
        &format!("refs/remotes/{remote}/{branch}"),
        oid,
        true,
        "test fixture",
    )
    .expect("remote-tracking ref");
}

/// Commit on a side ref so it is *not* part of HEAD's history.
fn commit_on_side_ref(raw: &Repository, message: &str, parent: Oid) -> Oid {
    let workdir = raw.workdir().expect("workdir").to_path_buf();
    std::fs::write(workdir.join("EXTRA"), format!("{message}\n")).expect("write file");
    let mut index = raw.index().expect("index");
    index.add_path(Path::new("EXTRA")).expect("stage");
    index.write().expect("write index");
    let tree = raw.find_tree(index.write_tree().expect("tree")).expect("tree");
    let parent_commit = raw.find_commit(parent).expect("parent commit");
    let when = Time::new(1_800_000_000, 0);
    let sig = Signature::new("Test", "test@example.com", &when).expect("signature");
    raw.commit(
        Some("refs/heads/side"),
        &sig,
        &sig,
        message,
        &tree,
        &[&parent_commit],
    )
    .expect("side commit")
}

fn labels_of(map: &std::collections::HashMap<String, Vec<gitpanel::model::RefBadge>>, id: &str) -> Vec<String> {
    map.get(id).map(|b| b.iter().map(|b| b.label.clone()).collect()).unwrap_or_default()
}

fn kind_of(map: &std::collections::HashMap<String, Vec<gitpanel::model::RefBadge>>, id: &str) -> Vec<RefKind> {
    map.get(id).map(|b| b.iter().map(|b| b.kind).collect()).unwrap_or_default()
}

/// Two remotes parked on different commits must each report their own
/// ahead/behind, and decorate the commit they point at.
#[test]
fn each_remote_reports_its_own_position() {
    let tr = common::make_repo_with_commits(5);
    let raw = Repository::open(tr.dir.path()).expect("reopen repo");
    let branch = tr.repo.current_branch_name().expect("branch name");

    let log = tr.repo.log(5).expect("log"); // newest first
    let head = &log[0];
    let two_back = &log[2];
    add_remote(&raw, "origin");
    add_remote(&raw, "upstream");
    set_remote_ref(&raw, "origin", &branch, Oid::from_str(&two_back.id).unwrap());
    set_remote_ref(&raw, "upstream", &branch, Oid::from_str(&head.id).unwrap());

    let positions = tr.repo.remote_positions().expect("remote_positions");
    assert_eq!(positions.len(), 2, "one entry per remote-tracking ref");

    let origin = positions.iter().find(|p| p.remote == "origin").expect("origin");
    assert_eq!(origin.commit_id, two_back.id);
    assert_eq!((origin.ahead, origin.behind), (2, 0));
    assert!(origin.is_tracking, "same branch name as HEAD");

    let upstream = positions
        .iter()
        .find(|p| p.remote == "upstream")
        .expect("upstream");
    assert_eq!((upstream.ahead, upstream.behind), (0, 0));

    let ids: Vec<String> = log.iter().map(|c| c.id.clone()).collect();
    let badges = ref_badge_map(&positions, &[], &ids);
    // Plain ref names, VS Code style — the divergence is not in the label.
    assert_eq!(labels_of(&badges, &two_back.id), [format!("origin/{branch}")]);
    assert_eq!(labels_of(&badges, &head.id), [format!("upstream/{branch}")]);
}

/// A local branch and a remote branch on the same commit each get a badge, and
/// the local branch names come from git — `main`, `master` or anything else.
#[test]
fn local_and_remote_badges_coexist_on_the_same_commit() {
    let tr = common::make_repo_with_commits(3);
    let raw = Repository::open(tr.dir.path()).expect("reopen repo");
    let branch = tr.repo.current_branch_name().expect("branch name");

    let log = tr.repo.log(3).expect("log");
    let head = &log[0];
    let head_oid = Oid::from_str(&head.id).unwrap();

    // A second local branch on HEAD, plus a third further back.
    raw.branch("dev", &raw.find_commit(head_oid).unwrap(), false)
        .expect("branch dev");
    raw.branch("other", &raw.find_commit(Oid::from_str(&log[1].id).unwrap()).unwrap(), false)
        .expect("branch other");
    add_remote(&raw, "origin");
    set_remote_ref(&raw, "origin", &branch, head_oid);

    let positions = tr.repo.remote_positions().expect("remote_positions");
    let locals = tr.repo.local_branch_positions().expect("local positions");
    assert_eq!(locals.len(), 3, "every local branch reports its commit");
    assert_eq!(
        locals.iter().filter(|l| l.is_head).map(|l| l.branch.as_str()).collect::<Vec<_>>(),
        [branch.as_str()],
        "only the checked-out branch is flagged as HEAD"
    );
    assert_eq!(locals[0].branch, branch, "HEAD's branch sorts first");

    let ids: Vec<String> = log.iter().map(|c| c.id.clone()).collect();
    let badges = ref_badge_map(&positions, &locals, &ids);

    // HEAD carries the remote badge *and* both local branches pointing at it.
    assert_eq!(
        labels_of(&badges, &head.id),
        vec![
            format!("local/{branch}"),
            "local/dev".to_string(),
            format!("origin/{branch}"),
        ]
    );
    assert_eq!(
        kind_of(&badges, &head.id),
        vec![RefKind::Local, RefKind::Local, RefKind::Remote]
    );
    // The third branch is decorated on its own commit, not on HEAD.
    assert_eq!(labels_of(&badges, &log[1].id), ["local/other".to_string()]);
}

/// Commits that exist only on a remote must appear in the commit list: the
/// graph has to show where the remote actually is, not just a count on HEAD.
#[test]
fn remote_only_commits_are_listed_by_the_all_refs_walk() {
    let tr = common::make_repo_with_commits(3);
    let raw = Repository::open(tr.dir.path()).expect("reopen repo");
    let branch = tr.repo.current_branch_name().expect("branch name");

    let log = tr.repo.log(3).expect("log");
    let head = &log[0];
    let side = commit_on_side_ref(&raw, "only on the remote", Oid::from_str(&head.id).unwrap());

    add_remote(&raw, "origin");
    set_remote_ref(&raw, "origin", &branch, side);

    // HEAD-only walk never sees it…
    let head_only: Vec<String> = tr.repo.log(50).expect("log").iter().map(|c| c.id.clone()).collect();
    assert!(
        !head_only.contains(&side.to_string()),
        "log() must stay inside HEAD's history"
    );

    // …but the all-refs walk lists it, so the commit list can render a row.
    let all = tr.repo.log_all_page(0, 50).expect("log_all_page");
    assert!(
        all.iter().any(|c| c.id == side.to_string()),
        "the remote-only commit must be listed"
    );
    assert_eq!(all.len(), 4, "three local commits plus the remote-only one");

    let ids: Vec<String> = all.iter().map(|c| c.id.clone()).collect();
    let positions = tr.repo.remote_positions().expect("remote_positions");
    let locals = tr.repo.local_branch_positions().expect("local positions");
    let badges = ref_badge_map(&positions, &locals, &ids);

    // The remote badge lands on the real remote commit now, not on HEAD. The
    // helper committed onto a local branch (`side`), so that branch is badged
    // there too — one marker per ref.
    assert_eq!(
        labels_of(&badges, &side.to_string()),
        ["local/side".to_string(), format!("origin/{branch}")]
    );
    assert_eq!(
        labels_of(&badges, &head.id),
        [format!("local/{branch}")],
        "HEAD carries only its local branch — origin's tip is in the list now"
    );
}

/// The all-refs walk pages deterministically: two pages must not overlap.
#[test]
fn all_refs_pages_do_not_overlap() {
    let tr = common::make_repo_with_commits(6);
    let raw = Repository::open(tr.dir.path()).expect("reopen repo");
    let branch = tr.repo.current_branch_name().expect("branch name");
    let log = tr.repo.log(6).expect("log");
    let side = commit_on_side_ref(&raw, "remote only", Oid::from_str(&log[3].id).unwrap());
    add_remote(&raw, "origin");
    set_remote_ref(&raw, "origin", &branch, side);

    let first = tr.repo.log_all_page(0, 3).expect("page 1");
    let second = tr.repo.log_all_page(3, 3).expect("page 2");
    assert_eq!(first.len(), 3);
    let first_ids: Vec<String> = first.iter().map(|c| c.id.clone()).collect();
    for c in &second {
        assert!(
            !first_ids.contains(&c.id),
            "commit {} appears on both pages",
            c.id
        );
    }
}

/// A repository without remotes has nothing to decorate.
#[test]
fn no_remotes_means_no_positions() {
    let tr = common::make_repo_with_commits(2);
    assert!(tr.repo.remote_positions().expect("positions").is_empty());
}
