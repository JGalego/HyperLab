//! Object identity.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The identity of an object inside a stack.
///
/// Ids are small integers rather than UUIDs for two reasons: they are stable
/// and human readable in saved files (`card id 12`), and HyperTalk scripts
/// address objects by id (`go to card id 12`). Ids are unique within a stack
/// and are never reused, even after the object is deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id(u64);

impl Id {
    /// Wraps a raw number as an id.
    ///
    /// Prefer [`IdGenerator::next_id`] when creating new objects; this exists for
    /// deserialization and for tests.
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The underlying number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for Id {
    fn from(raw: u64) -> Self {
        Self(raw)
    }
}

/// Hands out fresh [`Id`]s for a single stack.
///
/// The generator is part of the saved stack so that ids stay unique across
/// save/load cycles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IdGenerator {
    next: u64,
}

impl IdGenerator {
    /// Creates a generator that starts at `1`.
    #[must_use]
    pub const fn new() -> Self {
        Self { next: 1 }
    }

    /// Returns the next unused id.
    pub fn next_id(&mut self) -> Id {
        let id = Id(self.next);
        self.next += 1;
        id
    }

    /// The id that would be handed out next, without taking it.
    ///
    /// Persistence saves this so that ids are never reused across sessions.
    #[must_use]
    pub const fn peek(&self) -> Id {
        Id::new(self.next)
    }

    /// Ensures that ids handed out later are greater than `id`.
    ///
    /// Persistence calls this after loading objects so that a hand-edited file
    /// containing large ids cannot cause collisions.
    pub fn reserve(&mut self, id: Id) {
        self.next = self.next.max(id.0 + 1);
    }
}

impl Default for IdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_increasing() {
        let mut ids = IdGenerator::new();
        assert_eq!(ids.next_id(), Id::new(1));
        assert_eq!(ids.next_id(), Id::new(2));
    }

    #[test]
    fn reserve_skips_past_existing_ids() {
        let mut ids = IdGenerator::new();
        ids.reserve(Id::new(41));
        assert_eq!(ids.next_id(), Id::new(42));
    }

    #[test]
    fn reserve_never_moves_backwards() {
        let mut ids = IdGenerator::new();
        ids.reserve(Id::new(10));
        ids.reserve(Id::new(2));
        assert_eq!(ids.next_id(), Id::new(11));
    }
}
