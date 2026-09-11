use anyhow::{Context, Result};

use crate::model::BlameLine;
use crate::utils::repository::GitRepo;

impl GitRepo {
    /// Blame a file, optionally at a specific commit.
    /// If `commit_id` is None, blames the working directory version.
    pub fn blame_file(&self, path: &str, commit_id: Option<&str>) -> Result<Vec<BlameLine>> {
        let repo = self.inner();

        let mut opts = git2::BlameOptions::new();
        opts.track_copies_same_file(true);

        if let Some(id) = commit_id {
            let oid = git2::Oid::from_str(id).context("Invalid commit ID")?;
            opts.newest_commit(oid);
        }

        let blame = repo
            .blame_file(std::path::Path::new(path), Some(&mut opts))
            .context("Failed to blame file")?;

        // Read file content: from commit tree or workdir
        let lines: Vec<String> = if let Some(id) = commit_id {
            let oid = git2::Oid::from_str(id)?;
            let commit = repo.find_commit(oid)?;
            let tree = commit.tree()?;
            let entry = tree
                .get_path(std::path::Path::new(path))
                .context("File not found in commit tree")?;
            let blob = repo.find_blob(entry.id())?;
            let content = std::str::from_utf8(blob.content()).unwrap_or("");
            content.lines().map(String::from).collect()
        } else {
            let workdir = repo
                .workdir()
                .context("Repository has no working directory")?;
            let full_path = workdir.join(path);
            let content = std::fs::read_to_string(&full_path)
                .context("Failed to read file from working directory")?;
            content.lines().map(String::from).collect()
        };

        let mut result = Vec::with_capacity(lines.len());

        for (i, line_content) in lines.iter().enumerate() {
            let line_no = i + 1;
            if let Some(hunk) = blame.get_line(line_no) {
                let commit_id = hunk.final_commit_id().to_string();
                let short_id = commit_id[..7.min(commit_id.len())].to_string();
                let sig = hunk.final_signature();
                let author = sig.name().unwrap_or("").to_string();
                let time = sig.when();
                let seconds = time.seconds();

                // Get commit summary
                let summary = repo
                    .find_commit(hunk.final_commit_id())
                    .ok()
                    .and_then(|c| c.summary().map(String::from))
                    .unwrap_or_default();

                result.push(BlameLine {
                    line_no,
                    content: line_content.clone(),
                    commit_id,
                    short_id,
                    author,
                    time: seconds,
                    summary,
                });
            } else {
                result.push(BlameLine {
                    line_no,
                    content: line_content.clone(),
                    commit_id: String::new(),
                    short_id: String::new(),
                    author: String::new(),
                    time: 0,
                    summary: String::new(),
                });
            }
        }

        Ok(result)
    }
}
