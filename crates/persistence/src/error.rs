//! Errors from reading and writing stacks.

use std::{fmt, path::PathBuf};

/// The result of a save or a load.
pub type PersistenceResult<T> = Result<T, PersistenceError>;

/// Something went wrong reading or writing a stack.
#[derive(Debug)]
pub enum PersistenceError {
    /// The file system said no.
    Io {
        /// What was being read or written.
        path: PathBuf,
        /// Why it failed.
        source: std::io::Error,
    },
    /// A file was not the JSON we expected.
    Json {
        /// Which file.
        path: PathBuf,
        /// Why it failed.
        source: serde_json::Error,
    },
    /// The bundle was written by a newer version of HyperLab.
    UnsupportedVersion {
        /// The version in the file.
        found: u32,
        /// The newest version this build understands.
        supported: u32,
    },
    /// The bundle is missing something it cannot do without.
    Incomplete(String),
}

impl PersistenceError {
    /// Wraps an IO error with the path it happened on, because "file not
    /// found" without a name helps nobody.
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub(crate) fn json(path: impl Into<PathBuf>, source: serde_json::Error) -> Self {
        Self::Json {
            path: path.into(),
            source,
        }
    }
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Json { path, source } => {
                write!(f, "{} is not valid HyperLab JSON: {source}", path.display())
            }
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "this stack was saved in format version {found}, but this version of \
                 HyperLab understands only up to {supported}"
            ),
            Self::Incomplete(what) => write!(f, "this stack is incomplete: {what}"),
        }
    }
}

impl std::error::Error for PersistenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            _ => None,
        }
    }
}
