use std::collections::{HashMap, HashSet};

use anyhow::{bail, Context, Result};
use git2::{BranchType, Cred, RemoteCallbacks};

use crate::model::{LocalRefPos, RefBadge, RemoteRefPos};
use crate::utils::repository::GitRepo;

/// Upper bound on the number of remote-tracking refs annotated in the commit
/// list and the graph. A repository can track hundreds of remote branches;
/// decorating all of them would drown the UI (and cost one `graph_ahead_behind`
/// walk each).
const MAX_REMOTE_REFS: usize = 24;

/// Branch name git uses for the "default branch" pointer inside a remote.
const REMOTE_HEAD: &str = "HEAD";

fn credentials_callback(
    _url: &str,
    username_from_url: Option<&str>,
    allowed_types: git2::CredentialType,
) -> std::result::Result<Cred, git2::Error> {
    let username = username_from_url.unwrap_or("git");

    if allowed_types.contains(git2::CredentialType::SSH_KEY) {
        // Try SSH agent first
        if let Ok(cred) = Cred::ssh_key_from_agent(username) {
            return Ok(cred);
        }
        // Fallback: default SSH key
        let home = std::env::var("HOME").unwrap_or_default();
        let key_path = std::path::Path::new(&home).join(".ssh/id_ed25519");
        let key_path = if key_path.exists() {
            key_path
        } else {
            std::path::Path::new(&home).join(".ssh/id_rsa")
        };
        if key_path.exists() {
            return Cred::ssh_key(username, None, &key_path, None);
        }
    }

    if allowed_types.contains(git2::CredentialType::DEFAULT) {
        return Cred::default();
    }

    Err(git2::Error::from_str("no suitable credentials found"))
}

impl GitRepo {
    /// Fetch from origin remote.
    pub fn fetch(&self) -> Result<()> {
        let repo = self.inner();
        let mut remote = repo
            .find_remote("origin")
            .context("No 'origin' remote found")?;

        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(credentials_callback);

        let mut fetch_opts = git2::FetchOptions::new();
        fetch_opts.remote_callbacks(callbacks);

        let refspecs: Vec<String> = remote
            .fetch_refspecs()?
            .iter()
            .filter_map(|s| s.map(String::from))
            .collect();
        let refspec_strs: Vec<&str> = refspecs.iter().map(|s| s.as_str()).collect();

        remote
            .fetch(&refspec_strs, Some(&mut fetch_opts), None)
            .context("Fetch failed")?;

        Ok(())
    }

    /// Pull: fetch + fast-forward merge. Fails if not fast-forward.
    pub fn pull(&self) -> Result<String> {
        self.fetch()?;

        let repo = self.inner();
        let head = repo.head().context("No HEAD")?;
        let branch_name = head
            .shorthand()
            .context("HEAD is not a branch")?
            .to_string();

        let upstream_name = format!("refs/remotes/origin/{}", branch_name);
        let upstream_ref = repo
            .find_reference(&upstream_name)
            .context("No upstream tracking branch")?;
        let upstream_oid = upstream_ref
            .target()
            .context("Upstream ref has no target")?;

        let local_oid = head.target().context("HEAD has no target")?;

        if local_oid == upstream_oid {
            return Ok("Already up to date".to_string());
        }

        // Check if fast-forward is possible
        let (_, behind) = repo.graph_ahead_behind(local_oid, upstream_oid)?;
        if behind == 0 {
            return Ok("Already up to date (local is ahead)".to_string());
        }

        let can_ff = repo.graph_descendant_of(upstream_oid, local_oid)?;
        if !can_ff {
            bail!("Cannot fast-forward. Use merge or rebase manually.");
        }

        // Fast-forward
        let upstream_commit = repo.find_commit(upstream_oid)?;
        let mut head_ref = repo.head()?;
        head_ref.set_target(upstream_oid, &format!("pull: fast-forward to {}", upstream_oid))?;
        repo.checkout_tree(
            upstream_commit.as_object(),
            Some(git2::build::CheckoutBuilder::new().force()),
        )?;

        Ok(format!(
            "Fast-forwarded to {}",
            &upstream_oid.to_string()[..7]
        ))
    }

    /// Push current branch to origin.
    pub fn push(&self, force: bool) -> Result<()> {
        let repo = self.inner();
        let mut remote = repo
            .find_remote("origin")
            .context("No 'origin' remote found")?;

        let head = repo.head().context("No HEAD")?;
        let branch_name = head.shorthand().context("HEAD is not a branch")?;

        let refspec = if force {
            format!("+refs/heads/{}:refs/heads/{}", branch_name, branch_name)
        } else {
            format!("refs/heads/{}:refs/heads/{}", branch_name, branch_name)
        };

        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(credentials_callback);

        let mut push_error: Option<String> = None;
        let push_error_ref = &mut push_error as *mut Option<String>;
        callbacks.push_update_reference(move |refname, status| {
            if let Some(msg) = status {
                // Safety: callback is called synchronously during push
                unsafe {
                    *push_error_ref = Some(format!("Rejected {}: {}", refname, msg));
                }
            }
            Ok(())
        });

        let mut push_opts = git2::PushOptions::new();
        push_opts.remote_callbacks(callbacks);

        remote
            .push(&[&refspec], Some(&mut push_opts))
            .context("Push failed")?;

        if let Some(err) = push_error {
            bail!(err);
        }

        Ok(())
    }

    /// Resolve where every remote currently sits, relative to `HEAD`.
    ///
    /// Returns one entry per remote-tracking ref (`refs/remotes/<remote>/<branch>`),
    /// remotes in configuration order with `origin` first, and within a remote
    /// the branch matching the checked-out branch first.
    pub fn remote_positions(&self) -> Result<Vec<RemoteRefPos>> {
        let repo = self.inner();
        let head = repo.head().ok();
        let head_oid = head.as_ref().and_then(|h| h.target());
        let current_branch = head
            .as_ref()
            .and_then(|h| h.shorthand())
            .map(String::from);

        // Stable remote order: configuration order, but `origin` first.
        let mut remote_names: Vec<String> = repo
            .remotes()
            .context("Failed to list remotes")?
            .iter()
            .flatten()
            .map(String::from)
            .collect();
        remote_names.sort();
        if let Some(pos) = remote_names.iter().position(|n| n == "origin") {
            let origin = remote_names.remove(pos);
            remote_names.insert(0, origin);
        }

        // Collect every remote-tracking branch once: (remote, branch, oid).
        let mut tracked: Vec<(String, String, git2::Oid)> = Vec::new();
        for branch in repo.branches(Some(BranchType::Remote))? {
            let (branch, _) = branch?;
            let Some(name) = branch.name()? else { continue };
            let Some(oid) = branch.get().target() else { continue };
            let Some((remote, remote_branch)) = name.split_once('/') else {
                continue;
            };
            if remote_branch == REMOTE_HEAD {
                continue; // symbolic `origin/HEAD` — not a real position
            }
            tracked.push((remote.to_string(), remote_branch.to_string(), oid));
        }

        let mut out: Vec<RemoteRefPos> = Vec::new();
        'remotes: for remote in &remote_names {
            let mut owned: Vec<&(String, String, git2::Oid)> =
                tracked.iter().filter(|(r, _, _)| r == remote).collect();
            // The branch that push/pull would sync is the interesting one.
            let current = current_branch.clone();
            owned.sort_by_key(|(_, branch, _)| {
                (
                    current.as_deref() != Some(branch.as_str()),
                    branch.clone(),
                )
            });

            for (remote, branch, oid) in owned {
                if out.len() >= MAX_REMOTE_REFS {
                    break 'remotes;
                }
                let (ahead, behind) = match head_oid {
                    Some(h) => repo.graph_ahead_behind(h, *oid).unwrap_or((0, 0)),
                    None => (0, 0),
                };
                out.push(RemoteRefPos {
                    remote: remote.clone(),
                    branch: branch.clone(),
                    label: format!("{remote}/{branch}"),
                    commit_id: oid.to_string(),
                    ahead,
                    behind,
                    is_tracking: current_branch.as_deref() == Some(branch.as_str()),
                });
            }
        }

        Ok(out)
    }
}

/// Map `commit id -> badges` for the commits currently loaded.
///
/// Local branches decorate the commit they point at (`local/main`). Remote
/// branches do the same (`origin/main`); a *tracking* remote whose tip is
/// outside the loaded page is still pinned onto the newest loaded commit so the
/// divergence stays visible instead of silently disappearing — with the
/// all-refs walk this only happens for tips past the current page.
///
/// A commit can carry both kinds at once: when a local `main` and an
/// `origin/main` sit on the same commit, both badges are emitted, like VS Code.
pub fn ref_badge_map(
    remotes: &[RemoteRefPos],
    locals: &[LocalRefPos],
    loaded_ids: &[String],
) -> HashMap<String, Vec<RefBadge>> {
    let mut map: HashMap<String, Vec<RefBadge>> = HashMap::new();
    let known: HashSet<&str> = loaded_ids.iter().map(|s| s.as_str()).collect();

    // Local branches first: they read as "which branch is this" before
    // "where is that remote".
    for local in locals {
        if local.commit_id.is_empty() || !known.contains(local.commit_id.as_str()) {
            continue;
        }
        map.entry(local.commit_id.clone())
            .or_default()
            .push(RefBadge::local(&local.branch));
    }

    let Some(head) = loaded_ids.first() else {
        return map;
    };
    for pos in remotes {
        if pos.commit_id.is_empty() {
            continue;
        }
        let badge = RefBadge::remote(pos);
        if known.contains(pos.commit_id.as_str()) {
            map.entry(pos.commit_id.clone()).or_default().push(badge);
        } else if pos.is_tracking {
            map.entry(head.clone()).or_default().push(badge);
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RefKind;

    fn pos(remote: &str, branch: &str, commit: &str, ahead: usize, behind: usize) -> RemoteRefPos {
        RemoteRefPos {
            remote: remote.to_string(),
            branch: branch.to_string(),
            label: format!("{remote}/{branch}"),
            commit_id: commit.to_string(),
            ahead,
            behind,
            is_tracking: false,
        }
    }

    fn local(branch: &str, commit: &str) -> LocalRefPos {
        LocalRefPos {
            branch: branch.to_string(),
            label: format!("local/{branch}"),
            commit_id: commit.to_string(),
            is_head: false,
        }
    }

    fn nothing() -> Vec<RemoteRefPos> {
        Vec::new()
    }

    fn local_labels(map: &HashMap<String, Vec<RefBadge>>, id: &str) -> Vec<String> {
        map.get(id)
            .map(|b| {
                b.iter()
                    .filter(|b| b.kind == RefKind::Local)
                    .map(|b| b.label.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn remote_labels(map: &HashMap<String, Vec<RefBadge>>, id: &str) -> Vec<String> {
        map.get(id)
            .map(|b| {
                b.iter()
                    .filter(|b| b.kind == RefKind::Remote)
                    .map(|b| b.label.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The badge is the bare ref name (VS Code style) — the divergence lives in
    /// the tooltip, not in the pill.
    #[test]
    fn badge_label_is_the_bare_ref_name() {
        let diverged = pos("origin", "main", "a", 1, 2);
        let badge = RefBadge::remote(&diverged);
        assert_eq!(badge.label, "origin/main");
        assert_eq!(
            badge.tooltip,
            "origin/main — diverged: 1 ahead, 2 behind"
        );

        let behind = pos("fork", "master", "a", 0, 3);
        assert_eq!(RefBadge::remote(&behind).label, "fork/master");

        let in_sync = pos("upstream", "dev", "a", 0, 0);
        assert_eq!(RefBadge::remote(&in_sync).label, "upstream/dev");
    }

    /// A local `main` and an `origin/main` on the same commit each get a badge —
    /// one marker per ref, not one merged marker.
    #[test]
    fn local_and_remote_badges_coexist_on_one_commit() {
        let ids = vec!["head".to_string(), "old".to_string()];
        let origin = pos("origin", "main", "head", 0, 0);
        let locals = vec![local("main", "head"), local("dev", "head")];

        let map = ref_badge_map(&[origin], &locals, &ids);

        assert_eq!(local_labels(&map, "head"), ["local/main", "local/dev"]);
        assert_eq!(remote_labels(&map, "head"), ["origin/main"]);
        // Labels never leak onto a commit the ref does not point at.
        assert!(!map.contains_key("old"));
    }

    /// Branch names are whatever git reports — `master`, `main` or anything
    /// else, never assumed.
    #[test]
    fn local_badge_uses_the_real_branch_name() {
        let ids = vec!["c".to_string()];
        let locals = vec![local("master", "c"), local("feature/x", "c")];
        let map = ref_badge_map(&nothing(), &locals, &ids);
        let labels = map["c"].iter().map(|b| b.label.clone()).collect::<Vec<_>>();
        assert_eq!(labels, ["local/master", "local/feature/x"]);
    }

    /// A visible remote tip decorates the commit it points at; an invisible one
    /// is pinned onto HEAD so the user still sees how far the remote is off.
    #[test]
    fn badges_land_on_the_remote_tip_or_on_head() {
        let ids = vec!["head".to_string(), "mid".to_string(), "old".to_string()];
        let mut origin = pos("origin", "main", "old", 2, 0);
        origin.is_tracking = true;
        let mut upstream = pos("upstream", "main", "gone", 0, 4);
        upstream.is_tracking = true;
        // A different branch of the same remote: only annotated where it lands.
        let feature = pos("origin", "feature", "nowhere", 0, 1);

        let map = ref_badge_map(&[origin, upstream, feature], &[], &ids);

        assert_eq!(remote_labels(&map, "old"), ["origin/main"]);
        assert_eq!(remote_labels(&map, "head"), ["upstream/main"]);
        assert!(
            !map.contains_key("nowhere"),
            "non-tracking refs are not pinned onto HEAD"
        );
    }

    /// A local branch pointing outside the loaded page gets no badge rather than
    /// a wrong one — pinning is only meaningful for tracking remotes.
    #[test]
    fn local_branch_outside_the_page_is_not_pinned() {
        let ids = vec!["head".to_string()];
        let locals = vec![local("elsewhere", "not-loaded")];
        let map = ref_badge_map(&nothing(), &locals, &ids);
        assert!(map.is_empty());
    }

    /// A remote that was never fetched has no commit to decorate.
    #[test]
    fn unfetched_remote_is_skipped() {
        let ids = vec!["head".to_string()];
        let mut never = pos("origin", "main", "", 0, 0);
        never.is_tracking = true;
        assert!(ref_badge_map(&[never], &[], &ids).is_empty());
    }
}
