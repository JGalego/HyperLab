//! Cards.

use serde::{Deserialize, Serialize};

use crate::{Id, Object, ObjectCore, ObjectKind, Part, PartContainer, Value};

/// One card: the unit of navigation, and the thing the user actually sees.
///
/// A card shows its own parts on top of the parts of its background.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Card {
    #[serde(flatten)]
    core: ObjectCore,
    background: Id,
    #[serde(default)]
    parts: Vec<Part>,
}

impl Card {
    /// Creates an empty card belonging to `background`.
    pub fn new(id: Id, name: impl Into<String>, background: Id) -> Self {
        Self {
            core: ObjectCore::new(id, name),
            background,
            parts: Vec::new(),
        }
    }

    /// The background this card is drawn on.
    #[must_use]
    pub const fn background(&self) -> Id {
        self.background
    }

    /// Moves the card to another background.
    pub fn set_background(&mut self, background: Id) {
        self.background = background;
        self.touch();
    }
}

impl PartContainer for Card {
    fn parts(&self) -> &[Part] {
        &self.parts
    }

    fn parts_mut(&mut self) -> &mut Vec<Part> {
        &mut self.parts
    }
}

impl Object for Card {
    fn core(&self) -> &ObjectCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ObjectCore {
        &mut self.core
    }

    fn kind(&self) -> ObjectKind {
        ObjectKind::Card
    }

    fn intrinsic_property(&self, name: &str) -> Option<Value> {
        match name {
            "background" => Some(Value::from(self.background.get() as i64)),
            _ => None,
        }
    }

    fn intrinsic_property_names(&self) -> Vec<&'static str> {
        vec!["background"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PartKind, Point, Rect};

    fn card() -> Card {
        Card::new(Id::new(1), "First", Id::new(100))
    }

    fn part(id: u64, kind: PartKind, name: &str, rect: Rect) -> Part {
        Part::new(Id::new(id), kind, name, rect)
    }

    #[test]
    fn parts_are_kept_in_layer_order() {
        let mut card = card();
        card.add_part(part(2, PartKind::Button, "A", Rect::new(0, 0, 10, 10)));
        card.add_part(part(3, PartKind::Button, "B", Rect::new(0, 0, 10, 10)));
        assert_eq!(card.part_numbered(PartKind::Button, 1).unwrap().name(), "A");
        assert_eq!(card.part_number(Id::new(3)), Some(2));
    }

    #[test]
    fn numbering_is_per_kind() {
        let mut card = card();
        card.add_part(part(2, PartKind::Field, "F", Rect::default()));
        card.add_part(part(3, PartKind::Button, "B", Rect::default()));
        assert_eq!(card.part_number(Id::new(3)), Some(1));
        assert_eq!(card.parts_of_kind(PartKind::Field).len(), 1);
    }

    #[test]
    fn lookup_by_name_ignores_case() {
        let mut card = card();
        card.add_part(part(2, PartKind::Button, "Next Card", Rect::default()));
        assert!(card.part_named(PartKind::Button, "next card").is_some());
        assert!(card.part_named(PartKind::Field, "next card").is_none());
    }

    #[test]
    fn hit_testing_picks_the_frontmost_visible_part() {
        let mut card = card();
        card.add_part(part(2, PartKind::Button, "Back", Rect::new(0, 0, 50, 50)));
        card.add_part(part(3, PartKind::Button, "Front", Rect::new(0, 0, 50, 50)));
        assert_eq!(card.part_at(Point::new(10, 10)).unwrap().name(), "Front");

        card.part_mut(Id::new(3))
            .unwrap()
            .set_property("visible", false.into())
            .unwrap();
        assert_eq!(card.part_at(Point::new(10, 10)).unwrap().name(), "Back");
        assert!(card.part_at(Point::new(80, 80)).is_none());
    }

    #[test]
    fn removing_a_part_reports_its_layer() {
        let mut card = card();
        card.add_part(part(2, PartKind::Button, "A", Rect::default()));
        card.add_part(part(3, PartKind::Button, "B", Rect::default()));
        let (index, removed) = card.remove_part(Id::new(2)).unwrap();
        assert_eq!(index, 0);
        assert_eq!(removed.name(), "A");
        assert_eq!(card.parts().len(), 1);
    }
}
