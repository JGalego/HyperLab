//! Undo and redo.
//!
//! The history stores *inverse* commands: the thing that puts the stack back
//! the way it was. Undoing applies one and keeps its own inverse for redo, so
//! the two stacks can never disagree about what a change was.

use crate::command::Command;

/// How many changes are remembered before the oldest is forgotten.
const DEFAULT_LIMIT: usize = 200;

/// The undo and redo stacks.
#[derive(Debug, Clone)]
pub struct History {
    undo: Vec<Command>,
    redo: Vec<Command>,
    limit: usize,
}

impl History {
    /// A history that remembers the default number of changes.
    #[must_use]
    pub fn new() -> Self {
        Self::with_limit(DEFAULT_LIMIT)
    }

    /// A history that remembers `limit` changes.
    #[must_use]
    pub fn with_limit(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: limit.max(1),
        }
    }

    /// Records the inverse of a change the user just made.
    ///
    /// This clears the redo stack: once you make a new change, the future you
    /// undid is gone.
    pub fn record(&mut self, inverse: Command) {
        self.redo.clear();
        self.push_undo(inverse);
    }

    /// Takes the next thing to undo.
    pub fn take_undo(&mut self) -> Option<Command> {
        self.undo.pop()
    }

    /// Takes the next thing to redo.
    pub fn take_redo(&mut self) -> Option<Command> {
        self.redo.pop()
    }

    /// Records the inverse produced by an undo, so it can be redone.
    pub fn push_redo(&mut self, inverse: Command) {
        self.redo.push(inverse);
    }

    /// Records the inverse produced by a redo, so it can be undone again.
    pub fn push_undo(&mut self, inverse: Command) {
        self.undo.push(inverse);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
    }

    /// Whether there is anything to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether there is anything to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// What undo would do, for the Edit menu.
    #[must_use]
    pub fn undo_label(&self) -> Option<&'static str> {
        self.undo.last().map(Command::label)
    }

    /// What redo would do, for the Edit menu.
    #[must_use]
    pub fn redo_label(&self) -> Option<&'static str> {
        self.redo.last().map(Command::label)
    }

    /// Forgets everything, as when a stack is loaded.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperlab_stack::Id;

    fn command(id: u64) -> Command {
        Command::DeleteCard { id: Id::new(id) }
    }

    #[test]
    fn recording_clears_the_redo_stack() {
        let mut history = History::new();
        history.record(command(1));
        let undone = history.take_undo().unwrap();
        history.push_redo(undone);
        assert!(history.can_redo());

        history.record(command(2));
        assert!(
            !history.can_redo(),
            "a new change discards the redone future"
        );
    }

    #[test]
    fn the_oldest_change_is_forgotten_first() {
        let mut history = History::with_limit(2);
        history.record(command(1));
        history.record(command(2));
        history.record(command(3));
        assert_eq!(history.take_undo(), Some(command(3)));
        assert_eq!(history.take_undo(), Some(command(2)));
        assert_eq!(history.take_undo(), None);
    }

    #[test]
    fn labels_describe_what_would_happen() {
        let mut history = History::new();
        assert_eq!(history.undo_label(), None);
        history.record(command(1));
        assert_eq!(history.undo_label(), Some("Delete Card"));
    }
}
