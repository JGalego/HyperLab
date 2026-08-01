//! Backgrounds.

use serde::{Deserialize, Serialize};

use crate::{Id, Object, ObjectCore, ObjectKind, Part, PartContainer};

/// A layer shared by many cards.
///
/// Backgrounds are what make a stack a *stack* rather than a pile of
/// unrelated cards: the parts and the script live in one place, and every
/// card that uses the background inherits them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Background {
    #[serde(flatten)]
    core: ObjectCore,
    #[serde(default)]
    parts: Vec<Part>,
}

impl Background {
    /// Creates an empty background.
    pub fn new(id: Id, name: impl Into<String>) -> Self {
        Self {
            core: ObjectCore::new(id, name),
            parts: Vec::new(),
        }
    }
}

impl PartContainer for Background {
    fn parts(&self) -> &[Part] {
        &self.parts
    }

    fn parts_mut(&mut self) -> &mut Vec<Part> {
        &mut self.parts
    }
}

impl Object for Background {
    fn core(&self) -> &ObjectCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ObjectCore {
        &mut self.core
    }

    fn kind(&self) -> ObjectKind {
        ObjectKind::Background
    }
}
