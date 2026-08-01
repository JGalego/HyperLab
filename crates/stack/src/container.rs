//! Behaviour shared by the two things that own parts: cards and backgrounds.

use crate::{Id, Object, Part, PartKind};

/// An object that owns buttons and fields.
///
/// Cards and backgrounds differ in where they sit in the message path, not in
/// how they hold parts, so the whole part API lives here once.
pub trait PartContainer {
    /// The parts, in layer order: the first is furthest back.
    fn parts(&self) -> &[Part];

    /// The parts, mutably. Implementors expose their own storage; callers
    /// should prefer the helpers below.
    fn parts_mut(&mut self) -> &mut Vec<Part>;

    /// Adds a part in front of every existing one.
    fn add_part(&mut self, part: Part) {
        self.parts_mut().push(part);
    }

    /// Inserts a part at a specific layer, clamping to the end.
    fn insert_part(&mut self, index: usize, part: Part) {
        let index = index.min(self.parts().len());
        self.parts_mut().insert(index, part);
    }

    /// Removes a part, returning its layer and the part itself.
    fn remove_part(&mut self, id: Id) -> Option<(usize, Part)> {
        let index = self.part_index(id)?;
        Some((index, self.parts_mut().remove(index)))
    }

    /// The layer of a part.
    fn part_index(&self, id: Id) -> Option<usize> {
        self.parts().iter().position(|part| part.id() == id)
    }

    /// Looks a part up by id.
    fn part(&self, id: Id) -> Option<&Part> {
        self.parts().iter().find(|part| part.id() == id)
    }

    /// Looks a part up by id, mutably.
    fn part_mut(&mut self, id: Id) -> Option<&mut Part> {
        self.parts_mut().iter_mut().find(|part| part.id() == id)
    }

    /// Every part of one kind, in layer order.
    fn parts_of_kind(&self, kind: PartKind) -> Vec<&Part> {
        self.parts()
            .iter()
            .filter(|part| part.part_kind() == kind)
            .collect()
    }

    /// The first part of `kind` with this name, compared case-insensitively
    /// the way HyperTalk compares names.
    fn part_named(&self, kind: PartKind, name: &str) -> Option<&Part> {
        self.parts()
            .iter()
            .find(|part| part.part_kind() == kind && part.name().eq_ignore_ascii_case(name.trim()))
    }

    /// The `number`-th part of `kind`, counting from one.
    fn part_numbered(&self, kind: PartKind, number: usize) -> Option<&Part> {
        if number == 0 {
            return None;
        }
        self.parts_of_kind(kind).get(number - 1).copied()
    }

    /// The position of a part among the parts of its own kind, counting from
    /// one. This is what scripts mean by `the number of button "Go"`.
    fn part_number(&self, id: Id) -> Option<usize> {
        let part = self.part(id)?;
        self.parts_of_kind(part.part_kind())
            .iter()
            .position(|candidate| candidate.id() == id)
            .map(|index| index + 1)
    }

    /// The frontmost part whose rectangle contains the point and which is
    /// visible — that is, the part a click would land on.
    fn part_at(&self, point: crate::Point) -> Option<&Part> {
        self.parts().iter().rev().find(|part| {
            part.property("visible")
                .and_then(|value| value.as_bool())
                .unwrap_or(true)
                && part.geometry().contains(point)
        })
    }
}
