/// Represents an undoable operation.
#[derive(Debug, Clone)]
pub enum UndoableOp {
    StageFile(String),
    UnstageFile(String),
    StageAll,
    UnstageAll,
    /// Discard with saved file content for restoration.
    Discard(String, Vec<u8>),
}

/// Simple undo/redo stack for staging operations.
#[derive(Debug, Default)]
pub struct UndoStack {
    past: Vec<UndoableOp>,
    future: Vec<UndoableOp>,
}

impl UndoStack {
    pub fn push(&mut self, op: UndoableOp) {
        self.past.push(op);
        self.future.clear();
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
