use anyhow::{Context, Result};

use crate::repository::GitRepo;

impl GitRepo {
    /// Read .gitignore from the repository root. Returns empty string if it doesn't exist.
    pub fn read_gitignore(&self) -> Result<String> {
        let path = self.path().join(".gitignore");
        if path.exists() {
            std::fs::read_to_string(&path).context("Failed to read .gitignore")
        } else {
            Ok(String::new())
        }
    }

    /// Write content to .gitignore at the repository root.
    pub fn write_gitignore(&self, content: &str) -> Result<()> {
        let path = self.path().join(".gitignore");
        std::fs::write(&path, content).context("Failed to write .gitignore")
    }

    /// Append a pattern to .gitignore (with trailing newline).
    pub fn add_ignore_pattern(&self, pattern: &str) -> Result<()> {
        let mut content = self.read_gitignore()?;
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(pattern);
        content.push('\n');
        self.write_gitignore(&content)
    }
}
