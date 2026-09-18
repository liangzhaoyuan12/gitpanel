/// Represents an undoable operation.
///
/// Every variant carries the absolute repository path it was performed in so
/// that undo/redo can refuse to apply operations belonging to a different
/// repository (cross-repo pollution would overwrite unrelated files).
#[derive(Debug, Clone)]
pub enum UndoableOp {
    StageFile(String, String),
    UnstageFile(String, String),
    StageAll(String),
    UnstageAll(String),
    /// Discard with saved file content for restoration.
    /// Fields: repo path, relative file path, saved content.
    Discard(String, String, Vec<u8>),
}

impl UndoableOp {
    /// The repository this operation belongs to.
    pub fn repo_path(&self) -> &str {
        match self {
            UndoableOp::StageFile(_, p)
            | UndoableOp::UnstageFile(_, p)
            | UndoableOp::StageAll(p)
            | UndoableOp::UnstageAll(p) => p,
            UndoableOp::Discard(p, _, _) => p,
        }
    }
}

/// Simple undo/redo stack for staging operations.
///
/// Bounded: at most [`UndoStack::MAX_ENTRIES`] past entries are kept, and a
/// single `Discard` snapshot larger than [`UndoStack::MAX_DISARD_BYTES`] does
/// not store its content (undo then reports the discard as unrecoverable via
/// oversized_discard_drops_content() instead of pinning megabytes in RAM).
#[derive(Debug, Default)]
pub struct UndoStack {
    past: Vec<UndoableOp>,
    future: Vec<UndoableOp>,
}

impl UndoStack {
    /// Maximum number of operations remembered in each direction.
    pub const MAX_ENTRIES: usize = 50;
    /// Maximum size of a discarded-file snapshot stored for undo.
    pub const MAX_DISCARD_BYTES: usize = 8 * 1024 * 1024; // 8 MB

    pub fn push(&mut self, op: UndoableOp) {
        self.past.push(op);
        if self.past.len() > Self::MAX_ENTRIES {
            self.past.remove(0);
        }
        self.future.clear();
    }

    /// Push a discard operation, capping the stored snapshot size.
    /// Returns true when the content was kept, false when it was too large
    /// (the entry is still pushed, but undo cannot restore the bytes).
    pub fn push_discard(&mut self, repo_path: String, file_path: String, content: Vec<u8>) -> bool {
        let kept = content.len() <= Self::MAX_DISCARD_BYTES;
        let stored = if kept { content } else { Vec::new() };
        self.push(UndoableOp::Discard(repo_path, file_path, stored));
        kept
    }

    pub fn undo(&mut self) -> Option<UndoableOp> {
        let op = self.past.pop()?;
        self.future.push(op.clone());
        Some(op)
    }

    pub fn redo(&mut self) -> Option<UndoableOp> {
        let op = self.future.pop()?;
        self.past.push(op.clone());
        Some(op)
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    pub fn clear(&mut self) {
        self.past.clear();
        self.future.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_path_is_recorded() {
        let op = UndoableOp::StageFile("src/a.rs".into(), "/repo/x".into());
        assert_eq!(op.repo_path(), "/repo/x");
        let d = UndoableOp::Discard("/repo/y".into(), "a.rs".into(), vec![1]);
        assert_eq!(d.repo_path(), "/repo/y");
    }

    #[test]
    fn stack_is_bounded() {
        let mut s = UndoStack::default();
        for i in 0..(UndoStack::MAX_ENTRIES + 10) {
            s.push(UndoableOp::StageAll(format!("/r{i}")));
        }
        assert_eq!(s.past.len(), UndoStack::MAX_ENTRIES);
        // oldest entries were evicted from the front
        assert_eq!(s.past[0].repo_path(), format!("/r{}", 10));
    }

    #[test]
    fn oversized_discard_drops_content() {
        let mut s = UndoStack::default();
        let big = vec![0u8; UndoStack::MAX_DISCARD_BYTES + 1];
        let kept = s.push_discard("/r".into(), "big.bin".into(), big);
        assert!(!kept);
        match s.undo().unwrap() {
            UndoableOp::Discard(_, _, content) => assert!(content.is_empty()),
            other => panic!("unexpected op {:?}", other),
        }
    }

    #[test]
    fn small_discard_keeps_content() {
        let mut s = UndoStack::default();
        let kept = s.push_discard("/r".into(), "a.txt".into(), b"hello".to_vec());
        assert!(kept);
        match s.undo().unwrap() {
            UndoableOp::Discard(_, path, content) => {
                assert_eq!(path, "a.txt");
                assert_eq!(content, b"hello");
            }
            other => panic!("unexpected op {:?}", other),
        }
    }
}
