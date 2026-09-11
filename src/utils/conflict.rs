use anyhow::{Context, Result};

use crate::utils::repository::GitRepo;

/// A chunk of a conflicted file — either shared context or a conflict region.
#[derive(Debug, Clone)]
pub struct ConflictChunk {
    pub ours: Vec<String>,
    pub theirs: Vec<String>,
    pub is_conflict: bool,
}

/// Parse conflict markers from file content.
/// Supports standard markers: <<<<<<< / ======= / >>>>>>>
pub fn parse_conflict_markers(content: &str) -> Vec<ConflictChunk> {
    let mut chunks = Vec::new();
    let mut context_lines = Vec::new();

    enum State {
        Normal,
        InOurs,
        InTheirs,
    }

    let mut state = State::Normal;
    let mut ours_lines = Vec::new();
    let mut theirs_lines = Vec::new();

    for line in content.lines() {
        match state {
            State::Normal => {
                if line.starts_with("<<<<<<<") {
                    // Flush context
                    if !context_lines.is_empty() {
                        chunks.push(ConflictChunk {
                            ours: context_lines.clone(),
                            theirs: context_lines.clone(),
                            is_conflict: false,
                        });
                        context_lines.clear();
                    }
                    ours_lines.clear();
                    theirs_lines.clear();
                    state = State::InOurs;
                } else {
                    context_lines.push(line.to_string());
                }
            }
            State::InOurs => {
                if line.starts_with("=======") {
                    state = State::InTheirs;
                } else if line.starts_with("|||||||") {
                    // diff3 ancestor marker — skip these lines
                } else {
                    ours_lines.push(line.to_string());
                }
            }
            State::InTheirs => {
                if line.starts_with(">>>>>>>") {
                    chunks.push(ConflictChunk {
                        ours: ours_lines.clone(),
                        theirs: theirs_lines.clone(),
                        is_conflict: true,
                    });
                    ours_lines.clear();
                    theirs_lines.clear();
                    state = State::Normal;
                } else {
                    theirs_lines.push(line.to_string());
                }
            }
        }
    }

    // Flush remaining context
    if !context_lines.is_empty() {
        chunks.push(ConflictChunk {
            ours: context_lines.clone(),
            theirs: context_lines,
            is_conflict: false,
        });
    }

    chunks
}

impl GitRepo {
    /// Read a conflicted file from the working tree and parse its conflict markers.
    pub fn parse_conflicts(&self, path: &str) -> Result<Vec<ConflictChunk>> {
        let full_path = self.path().join(path);
        let content = std::fs::read_to_string(&full_path)
            .with_context(|| format!("Failed to read conflicted file: {}", path))?;
        Ok(parse_conflict_markers(&content))
    }

    /// Save the resolved content for a file and stage it.
    pub fn save_resolved_file(&self, path: &str, content: &str) -> Result<()> {
        let full_path = self.path().join(path);
        std::fs::write(&full_path, content)
            .with_context(|| format!("Failed to write resolved file: {}", path))?;
        self.stage_file(path)
    }
}
