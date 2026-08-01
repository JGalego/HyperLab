//! Errors from calling a tool.

use std::fmt;

use hyperlab_runtime::RuntimeError;

/// The result of calling a tool.
pub type ToolResult<T> = Result<T, ToolError>;

/// Why a tool call did not work.
///
/// Every message is written to be read by a model as well as a person: it
/// says what was wrong and, where possible, what to do instead, because the
/// caller will usually try again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    /// No tool by that name.
    UnknownTool(String),
    /// An argument was missing or the wrong shape.
    BadArguments(String),
    /// The runtime refused.
    Runtime(String),
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTool(name) => write!(f, "there is no tool called \"{name}\""),
            Self::BadArguments(what) => write!(f, "{what}"),
            Self::Runtime(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for ToolError {}

impl From<RuntimeError> for ToolError {
    fn from(error: RuntimeError) -> Self {
        Self::Runtime(error.to_string())
    }
}
