use anyhow::{bail, Context, Result};
use git2::{Cred, RemoteCallbacks};

use crate::utils::repository::GitRepo;

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
}
