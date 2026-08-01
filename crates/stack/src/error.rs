//! Errors raised by the object model.

use std::fmt;

use crate::{Id, ObjectKind};

/// The result of an operation on the object model.
pub type StackResult<T> = Result<T, StackError>;

/// Something the object model refused to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StackError {
    /// No object of that kind carries that id.
    NoSuchObject {
        /// The kind that was looked for.
        kind: ObjectKind,
        /// The id that was looked for.
        id: Id,
    },
    /// No object of that kind carries that name.
    NoSuchName {
        /// The kind that was looked for.
        kind: ObjectKind,
        /// The name that was looked for.
        name: String,
    },
    /// A positional lookup (`card 4`) fell outside the collection.
    OutOfRange {
        /// The kind that was looked for.
        kind: ObjectKind,
        /// The one-based position that was asked for.
        position: i64,
    },
    /// The property exists but cannot be written (`the id of card 1`).
    ReadOnlyProperty(String),
    /// The property was given a value it cannot hold.
    InvalidPropertyValue {
        /// The property that was written.
        property: String,
        /// Why the value was rejected.
        reason: String,
    },
    /// A stack must keep at least one card.
    LastCard,
    /// A background is still in use by at least one card.
    BackgroundInUse(Id),
}

impl fmt::Display for StackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchObject { kind, id } => write!(f, "no {kind} with id {id}"),
            Self::NoSuchName { kind, name } => write!(f, "no {kind} named \"{name}\""),
            Self::OutOfRange { kind, position } => {
                write!(f, "there is no {kind} number {position}")
            }
            Self::ReadOnlyProperty(name) => write!(f, "the property \"{name}\" is read-only"),
            Self::InvalidPropertyValue { property, reason } => {
                write!(f, "cannot set \"{property}\": {reason}")
            }
            Self::LastCard => write!(f, "a stack must contain at least one card"),
            Self::BackgroundInUse(id) => {
                write!(f, "background id {id} is still used by at least one card")
            }
        }
    }
}

impl std::error::Error for StackError {}
