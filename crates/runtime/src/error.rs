//! Runtime errors.

use std::fmt;

use hyperlab_parser::ParseError;
use hyperlab_stack::StackError;

/// The result of a runtime operation.
pub type RuntimeResult<T> = Result<T, RuntimeError>;

/// Something went wrong while executing.
///
/// Errors are phrased for the person who wrote the script, and carry the line
/// they happened on where one is known, so the script editor can point at it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeError {
    /// What went wrong.
    pub message: String,
    /// The line of the script that was running, if any.
    pub line: Option<u32>,
}

impl RuntimeError {
    /// An error with no known source line.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
        }
    }

    /// Attaches a source line, keeping any line already recorded: the
    /// innermost frame knows best.
    #[must_use]
    pub fn at_line(mut self, line: u32) -> Self {
        self.line.get_or_insert(line);
        self
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for RuntimeError {}

impl From<ParseError> for RuntimeError {
    fn from(error: ParseError) -> Self {
        Self {
            message: error.message,
            line: Some(error.line),
        }
    }
}

impl From<StackError> for RuntimeError {
    fn from(error: StackError) -> Self {
        Self::new(error.to_string())
    }
}
