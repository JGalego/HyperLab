//! Parse errors.

use std::fmt;

/// The result of lexing or parsing.
pub type ParseResult<T> = Result<T, ParseError>;

/// A problem found while reading a script.
///
/// Errors carry a line and column so the script editor can put the cursor
/// where the trouble is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// What went wrong, phrased for the person writing the script.
    pub message: String,
    /// Source line, counting from one.
    pub line: u32,
    /// Source column, counting from one.
    pub column: u32,
}

impl ParseError {
    /// Builds an error.
    pub fn new(message: impl Into<String>, line: u32, column: u32) -> Self {
        Self {
            message: message.into(),
            line,
            column,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}
