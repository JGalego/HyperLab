//! Stacks: the document.

use serde::{Deserialize, Serialize};

use crate::{
    Background, Card, Id, IdGenerator, Object, ObjectCore, ObjectKind, Part, PartKind, Rect, Size,
    StackError, StackResult, Value, geometry::Point,
};

/// A HyperLab document: one or more cards drawn on one or more backgrounds.
///
/// The stack owns every object it contains, and owns the [`IdGenerator`] that
/// keeps their ids unique. It knows nothing about editing, undo or scripts —
/// that is the runtime's job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stack {
    #[serde(flatten)]
    core: ObjectCore,
    size: Size,
    ids: IdGenerator,
    backgrounds: Vec<Background>,
    cards: Vec<Card>,
}

impl Stack {
    /// Creates a stack containing one empty background and one empty card,
    /// because a stack with no cards has nothing to show.
    pub fn new(name: impl Into<String>) -> Self {
        let mut ids = IdGenerator::new();
        let stack_id = ids.next_id();
        let background = Background::new(ids.next_id(), "Background 1");
        let card = Card::new(ids.next_id(), "Card 1", background.id());
        Self {
            core: ObjectCore::new(stack_id, name),
            size: Size::default(),
            ids,
            backgrounds: vec![background],
            cards: vec![card],
        }
    }

    /// The size of every card in this stack.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// Resizes every card in this stack.
    pub fn set_size(&mut self, size: Size) {
        self.size = size;
        self.touch();
    }

    /// Reserves a fresh id.
    ///
    /// Callers that build objects by hand (commands, importers) take their ids
    /// from here so that ids stay unique.
    pub fn next_id(&mut self) -> Id {
        self.ids.next_id()
    }

    /// Ensures future ids do not collide with `id`.
    pub fn reserve_id(&mut self, id: Id) {
        self.ids.reserve(id);
    }

    // ---------------------------------------------------------------- cards

    /// Every card, in order.
    #[must_use]
    pub fn cards(&self) -> &[Card] {
        &self.cards
    }

    /// How many cards the stack holds. Always at least one.
    #[must_use]
    pub fn card_count(&self) -> usize {
        self.cards.len()
    }

    /// A card by id.
    #[must_use]
    pub fn card(&self, id: Id) -> Option<&Card> {
        self.cards.iter().find(|card| card.id() == id)
    }

    /// A card by id, mutably.
    pub fn card_mut(&mut self, id: Id) -> Option<&mut Card> {
        self.cards.iter_mut().find(|card| card.id() == id)
    }

    /// The zero-based position of a card.
    #[must_use]
    pub fn card_index(&self, id: Id) -> Option<usize> {
        self.cards.iter().position(|card| card.id() == id)
    }

    /// The card at a zero-based position.
    #[must_use]
    pub fn card_at(&self, index: usize) -> Option<&Card> {
        self.cards.get(index)
    }

    /// The first card with this name, compared case-insensitively.
    #[must_use]
    pub fn card_named(&self, name: &str) -> Option<&Card> {
        self.cards
            .iter()
            .find(|card| card.name().eq_ignore_ascii_case(name.trim()))
    }

    /// Creates a card on `background` without adding it to the stack.
    ///
    /// # Errors
    ///
    /// Returns [`StackError::NoSuchObject`] if the background does not exist.
    pub fn new_card(&mut self, background: Id) -> StackResult<Card> {
        if self.background(background).is_none() {
            return Err(StackError::NoSuchObject {
                kind: ObjectKind::Background,
                id: background,
            });
        }
        let id = self.next_id();
        let name = format!("Card {}", self.cards.len() + 1);
        Ok(Card::new(id, name, background))
    }

    /// Inserts a card at a zero-based position, clamping to the end.
    pub fn insert_card(&mut self, index: usize, card: Card) {
        self.reserve_id(card.id());
        let index = index.min(self.cards.len());
        self.cards.insert(index, card);
        self.touch();
    }

    /// Adds a card after every existing one.
    pub fn add_card(&mut self, card: Card) {
        let index = self.cards.len();
        self.insert_card(index, card);
    }

    /// Removes a card, returning its position and the card itself.
    ///
    /// # Errors
    ///
    /// Returns [`StackError::LastCard`] when the card is the only one left,
    /// and [`StackError::NoSuchObject`] when it does not exist.
    pub fn remove_card(&mut self, id: Id) -> StackResult<(usize, Card)> {
        if self.cards.len() == 1 {
            return Err(StackError::LastCard);
        }
        let index = self.card_index(id).ok_or(StackError::NoSuchObject {
            kind: ObjectKind::Card,
            id,
        })?;
        let card = self.cards.remove(index);
        self.touch();
        Ok((index, card))
    }

    // ---------------------------------------------------------- backgrounds

    /// Every background, in order.
    #[must_use]
    pub fn backgrounds(&self) -> &[Background] {
        &self.backgrounds
    }

    /// A background by id.
    #[must_use]
    pub fn background(&self, id: Id) -> Option<&Background> {
        self.backgrounds
            .iter()
            .find(|background| background.id() == id)
    }

    /// A background by id, mutably.
    pub fn background_mut(&mut self, id: Id) -> Option<&mut Background> {
        self.backgrounds
            .iter_mut()
            .find(|background| background.id() == id)
    }

    /// The background a card is drawn on.
    #[must_use]
    pub fn background_of(&self, card: Id) -> Option<&Background> {
        self.background(self.card(card)?.background())
    }

    /// The first background with this name, compared case-insensitively.
    #[must_use]
    pub fn background_named(&self, name: &str) -> Option<&Background> {
        self.backgrounds
            .iter()
            .find(|background| background.name().eq_ignore_ascii_case(name.trim()))
    }

    /// Creates a background, adds it to the stack and returns its id.
    pub fn add_background(&mut self, name: impl Into<String>) -> Id {
        let id = self.next_id();
        self.backgrounds.push(Background::new(id, name));
        self.touch();
        id
    }

    /// Puts a whole background back, parts and all. This is what undoing a
    /// background deletion does.
    pub fn insert_background(&mut self, background: Background) {
        self.reserve_id(background.id());
        self.backgrounds.push(background);
        self.touch();
    }

    /// Removes a background no card is using.
    ///
    /// # Errors
    ///
    /// Returns [`StackError::BackgroundInUse`] if any card still refers to it.
    pub fn remove_background(&mut self, id: Id) -> StackResult<Background> {
        if self.cards.iter().any(|card| card.background() == id) {
            return Err(StackError::BackgroundInUse(id));
        }
        let index = self
            .backgrounds
            .iter()
            .position(|background| background.id() == id)
            .ok_or(StackError::NoSuchObject {
                kind: ObjectKind::Background,
                id,
            })?;
        self.touch();
        Ok(self.backgrounds.remove(index))
    }

    // ---------------------------------------------------------------- parts

    /// Creates a part without attaching it to anything.
    ///
    /// The caller decides whether it belongs to a card or a background.
    pub fn new_part(&mut self, kind: PartKind, name: impl Into<String>, geometry: Rect) -> Part {
        let id = self.next_id();
        Part::new(id, kind, name, geometry)
    }

    /// A sensible place to drop a new part: near the top-left of the card,
    /// nudged so that repeated creations do not stack up exactly.
    #[must_use]
    pub fn default_part_geometry(&self, kind: PartKind, nth: usize) -> Rect {
        let size = kind.default_size();
        let offset = i32::try_from(nth % 8).unwrap_or(0) * 12;
        Rect::new(20 + offset, 20 + offset, size.width, size.height)
    }

    /// Finds a part anywhere in the stack, reporting who owns it.
    ///
    /// Used by commands and the inspector, which are handed an id and must
    /// work out where it lives.
    #[must_use]
    pub fn locate_part(&self, id: Id) -> Option<PartLocation> {
        use crate::PartContainer;
        for card in &self.cards {
            if card.part(id).is_some() {
                return Some(PartLocation::Card(card.id()));
            }
        }
        for background in &self.backgrounds {
            if background.part(id).is_some() {
                return Some(PartLocation::Background(background.id()));
            }
        }
        None
    }

    /// A part anywhere in the stack.
    #[must_use]
    pub fn part(&self, id: Id) -> Option<&Part> {
        use crate::PartContainer;
        match self.locate_part(id)? {
            PartLocation::Card(card) => self.card(card)?.part(id),
            PartLocation::Background(background) => self.background(background)?.part(id),
        }
    }

    /// A part anywhere in the stack, mutably.
    pub fn part_mut(&mut self, id: Id) -> Option<&mut Part> {
        use crate::PartContainer;
        match self.locate_part(id)? {
            PartLocation::Card(card) => self.card_mut(card)?.part_mut(id),
            PartLocation::Background(background) => self.background_mut(background)?.part_mut(id),
        }
    }

    /// Any object in the stack, by kind and id, as a plain [`Object`].
    ///
    /// The inspector and the script engine use this when they only need the
    /// shared behaviour.
    #[must_use]
    pub fn object(&self, kind: ObjectKind, id: Id) -> Option<&dyn Object> {
        match kind {
            ObjectKind::Stack if id == self.id() => Some(self),
            ObjectKind::Stack => None,
            ObjectKind::Background => self.background(id).map(|b| b as &dyn Object),
            ObjectKind::Card => self.card(id).map(|c| c as &dyn Object),
            ObjectKind::Button | ObjectKind::Field => self
                .part(id)
                .filter(|part| part.kind() == kind)
                .map(|p| p as &dyn Object),
        }
    }

    /// Any object in the stack, mutably.
    pub fn object_mut(&mut self, kind: ObjectKind, id: Id) -> Option<&mut dyn Object> {
        match kind {
            ObjectKind::Stack if id == self.id() => Some(self),
            ObjectKind::Stack => None,
            ObjectKind::Background => self.background_mut(id).map(|b| b as &mut dyn Object),
            ObjectKind::Card => self.card_mut(id).map(|c| c as &mut dyn Object),
            ObjectKind::Button | ObjectKind::Field => self
                .part_mut(id)
                .filter(|part| part.kind() == kind)
                .map(|p| p as &mut dyn Object),
        }
    }
}

/// Where a part lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartLocation {
    /// Owned by this card.
    Card(Id),
    /// Owned by this background, and so shared by every card that uses it.
    Background(Id),
}

impl Object for Stack {
    fn core(&self) -> &ObjectCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ObjectCore {
        &mut self.core
    }

    fn kind(&self) -> ObjectKind {
        ObjectKind::Stack
    }

    fn intrinsic_property(&self, name: &str) -> Option<Value> {
        match name {
            "width" => Some(self.size.width.into()),
            "height" => Some(self.size.height.into()),
            "cardcount" | "numberofcards" => Some(self.cards.len().into()),
            _ => None,
        }
    }

    fn set_intrinsic_property(&mut self, name: &str, value: &Value) -> StackResult<bool> {
        let number = || {
            value
                .as_number()
                .map(|n| n as i32)
                .ok_or_else(|| StackError::InvalidPropertyValue {
                    property: name.to_string(),
                    reason: format!("\"{value}\" is not a number"),
                })
        };
        match name {
            "width" => self.size = Size::new(number()?, self.size.height),
            "height" => self.size = Size::new(self.size.width, number()?),
            "cardcount" | "numberofcards" => {
                return Err(StackError::ReadOnlyProperty(name.to_string()));
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn intrinsic_property_names(&self) -> Vec<&'static str> {
        vec!["width", "height", "cardCount"]
    }
}

/// The centre of a card of this size, useful when placing new parts.
#[must_use]
pub fn centre_of(size: Size) -> Point {
    Point::new(size.width / 2, size.height / 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PartContainer;

    #[test]
    fn a_new_stack_is_immediately_usable() {
        let stack = Stack::new("Test");
        assert_eq!(stack.card_count(), 1);
        assert_eq!(stack.backgrounds().len(), 1);
        assert_eq!(
            stack.cards()[0].background(),
            stack.backgrounds()[0].id(),
            "the first card must sit on the first background"
        );
    }

    #[test]
    fn ids_are_unique_across_kinds() {
        let mut stack = Stack::new("Test");
        let card = stack.new_card(stack.backgrounds()[0].id()).unwrap();
        let part = stack.new_part(PartKind::Button, "Go", Rect::default());
        let mut ids = vec![
            stack.id(),
            stack.backgrounds()[0].id(),
            stack.cards()[0].id(),
        ];
        ids.push(card.id());
        ids.push(part.id());
        let unique: std::collections::BTreeSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
    }

    #[test]
    fn the_last_card_cannot_be_removed() {
        let mut stack = Stack::new("Test");
        let only = stack.cards()[0].id();
        assert_eq!(stack.remove_card(only), Err(StackError::LastCard));
    }

    #[test]
    fn cards_can_be_removed_and_put_back_where_they_were() {
        let mut stack = Stack::new("Test");
        let background = stack.backgrounds()[0].id();
        let second = stack.new_card(background).unwrap();
        let second_id = second.id();
        stack.add_card(second);

        let (index, card) = stack.remove_card(second_id).unwrap();
        assert_eq!(index, 1);
        stack.insert_card(index, card);
        assert_eq!(stack.card_index(second_id), Some(1));
    }

    #[test]
    fn a_background_in_use_cannot_be_removed() {
        let mut stack = Stack::new("Test");
        let background = stack.backgrounds()[0].id();
        assert_eq!(
            stack.remove_background(background),
            Err(StackError::BackgroundInUse(background))
        );
    }

    #[test]
    fn parts_can_be_found_from_the_stack_down() {
        let mut stack = Stack::new("Test");
        let card_id = stack.cards()[0].id();
        let background_id = stack.backgrounds()[0].id();

        let button = stack.new_part(PartKind::Button, "Go", Rect::default());
        let button_id = button.id();
        stack.card_mut(card_id).unwrap().add_part(button);

        let field = stack.new_part(PartKind::Field, "Name", Rect::default());
        let field_id = field.id();
        stack.background_mut(background_id).unwrap().add_part(field);

        assert_eq!(
            stack.locate_part(button_id),
            Some(PartLocation::Card(card_id))
        );
        assert_eq!(
            stack.locate_part(field_id),
            Some(PartLocation::Background(background_id))
        );
        assert_eq!(stack.part(field_id).unwrap().name(), "Name");
        assert!(stack.part(Id::new(9999)).is_none());
    }

    #[test]
    fn objects_are_reachable_by_kind_and_id() {
        let mut stack = Stack::new("Test");
        let card_id = stack.cards()[0].id();
        let button = stack.new_part(PartKind::Button, "Go", Rect::default());
        let button_id = button.id();
        stack.card_mut(card_id).unwrap().add_part(button);

        assert_eq!(
            stack.object(ObjectKind::Card, card_id).unwrap().name(),
            "Card 1"
        );
        assert_eq!(
            stack.object(ObjectKind::Button, button_id).unwrap().name(),
            "Go"
        );
        assert!(
            stack.object(ObjectKind::Field, button_id).is_none(),
            "a button must not answer to `field`"
        );
    }

    #[test]
    fn stack_geometry_is_a_property() {
        let mut stack = Stack::new("Test");
        assert_eq!(stack.property("width"), Some(Value::Number(512.0)));
        stack.set_property("height", 400.into()).unwrap();
        assert_eq!(stack.size(), Size::new(512, 400));
        assert!(stack.set_property("cardCount", 5.into()).is_err());
    }

    #[test]
    fn a_stack_survives_a_json_round_trip() {
        let mut stack = Stack::new("Test");
        let card_id = stack.cards()[0].id();
        let button = stack.new_part(PartKind::Button, "Go", Rect::new(1, 2, 3, 4));
        stack.card_mut(card_id).unwrap().add_part(button);

        let json = serde_json::to_string(&stack).unwrap();
        let restored: Stack = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, stack);
    }
}
