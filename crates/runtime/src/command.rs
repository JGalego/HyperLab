//! Commands: the only way anything ever changes.
//!
//! Every mutation of a stack — by the user, by a script, by an AI assistant —
//! is a [`Command`]. Applying one returns the command that undoes it, so undo
//! costs nothing extra and can never drift out of step with the change it is
//! meant to reverse.
//!
//! This is the rule the whole architecture leans on. Because the UI cannot
//! reach past commands, scripting, undo, automation, testing and AI all take
//! the same path through the system.

use hyperlab_stack::{
    Card, Id, Image, Object, ObjectId, ObjectKind, Part, PartContainer, PartKind, Rect, Size,
    Stack, StackError, Value,
};
use serde::{Deserialize, Serialize};

use crate::error::{RuntimeError, RuntimeResult};

/// Who owns a part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PartOwner {
    /// A card: the part appears on that card only.
    Card {
        /// Which card.
        id: Id,
    },
    /// A background: the part appears on every card that uses it.
    Background {
        /// Which background.
        id: Id,
    },
}

impl PartOwner {
    /// The id of the owning object.
    #[must_use]
    pub const fn id(self) -> Id {
        match self {
            Self::Card { id } | Self::Background { id } => id,
        }
    }
}

/// A change to a stack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "camelCase")]
pub enum Command {
    /// Adds a new empty card after `index`.
    CreateCard {
        /// Zero-based position of the card it goes after; the new card lands
        /// at `index + 1`.
        after: usize,
        /// Which background it uses. `None` reuses the background of the card
        /// it follows.
        background: Option<Id>,
    },
    /// Puts a whole card back. This is what undoing a deletion does.
    InsertCard {
        /// Zero-based position.
        index: usize,
        /// The card, with all its parts.
        card: Box<Card>,
    },
    /// Removes a card.
    DeleteCard {
        /// Which card.
        id: Id,
    },
    /// Adds a background with no cards on it yet.
    CreateBackground {
        /// Its name.
        name: String,
    },
    /// Puts a whole background back, parts and all.
    InsertBackground {
        /// The background.
        background: Box<hyperlab_stack::Background>,
    },
    /// Removes a background that no card is using.
    DeleteBackground {
        /// Which background.
        id: Id,
    },
    /// Adds a button or a field.
    CreatePart {
        /// Where it goes.
        owner: PartOwner,
        /// Button or field.
        kind: PartKind,
        /// Its name.
        name: String,
        /// Where it sits.
        geometry: Rect,
    },
    /// Puts a whole part back, at its old layer.
    InsertPart {
        /// Where it goes.
        owner: PartOwner,
        /// Its layer among its siblings.
        index: usize,
        /// The part.
        part: Box<Part>,
    },
    /// Removes a part.
    DeletePart {
        /// Which part.
        id: Id,
    },
    /// Moves or resizes a part.
    SetGeometry {
        /// Which part.
        id: Id,
        /// Its new rectangle.
        geometry: Rect,
    },
    /// Writes a property. `None` removes it, which is how undo restores an
    /// object that never had the property in the first place.
    SetProperty {
        /// Which object.
        object: ObjectId,
        /// The property name.
        property: String,
        /// The new value, or `None` to remove it.
        value: Option<Value>,
    },
    /// Replaces an object's script.
    SetScript {
        /// Which object.
        object: ObjectId,
        /// The new source.
        script: String,
    },
    /// Renames an object.
    Rename {
        /// Which object.
        object: ObjectId,
        /// Its new name.
        name: String,
    },
    /// Resizes every card in the stack.
    SetStackSize {
        /// The new card size.
        size: Size,
    },
    /// Puts a picture in the stack's library, or takes one out.
    ///
    /// Pictures travel with the stack, so importing one changes the document
    /// and belongs in the undo history alongside everything else.
    SetImage {
        /// What the picture is called, which is also its file name in the
        /// bundle.
        name: String,
        /// The picture, or `None` to remove it.
        image: Option<Box<Image>>,
    },
}

/// What applying a command produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    /// The command that puts things back exactly as they were.
    pub inverse: Command,
    /// The object the command created, if it created one.
    pub created: Option<ObjectId>,
}

impl Command {
    /// Whether this command belongs in the undo history.
    ///
    /// Everything currently does; navigation deliberately is not a command,
    /// because "undo" should not mean "go back".
    #[must_use]
    pub const fn is_undoable(&self) -> bool {
        true
    }

    /// A short description, for menus and for the history panel.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::CreateCard { .. } | Self::InsertCard { .. } => "New Card",
            Self::DeleteCard { .. } => "Delete Card",
            Self::CreateBackground { .. } | Self::InsertBackground { .. } => "New Background",
            Self::DeleteBackground { .. } => "Delete Background",
            Self::CreatePart { .. } | Self::InsertPart { .. } => "New Part",
            Self::DeletePart { .. } => "Delete Part",
            Self::SetGeometry { .. } => "Move Part",
            Self::SetProperty { .. } => "Set Property",
            Self::SetScript { .. } => "Edit Script",
            Self::Rename { .. } => "Rename",
            Self::SetStackSize { .. } => "Resize Stack",
            Self::SetImage { image: Some(_), .. } => "Add Image",
            Self::SetImage { image: None, .. } => "Remove Image",
        }
    }

    /// Applies the command to a stack.
    ///
    /// # Errors
    ///
    /// Returns a [`RuntimeError`] when the command refers to something that
    /// is not there, or asks for something the object model forbids (such as
    /// deleting the only card).
    pub fn apply(self, stack: &mut Stack) -> RuntimeResult<Applied> {
        match self {
            Self::CreateCard { after, background } => {
                let background = match background {
                    Some(id) => id,
                    None => stack
                        .card_at(after.min(stack.card_count().saturating_sub(1)))
                        .map(Card::background)
                        .ok_or_else(|| RuntimeError::new("this stack has no cards"))?,
                };
                let card = stack.new_card(background)?;
                let id = card.id();
                let index = (after + 1).min(stack.card_count());
                stack.insert_card(index, card);
                Ok(Applied {
                    inverse: Self::DeleteCard { id },
                    created: Some(ObjectId::new(ObjectKind::Card, id)),
                })
            }

            Self::InsertCard { index, card } => {
                let id = card.id();
                stack.insert_card(index, *card);
                Ok(Applied {
                    inverse: Self::DeleteCard { id },
                    created: Some(ObjectId::new(ObjectKind::Card, id)),
                })
            }

            Self::DeleteCard { id } => {
                let (index, card) = stack.remove_card(id)?;
                Ok(Applied {
                    inverse: Self::InsertCard {
                        index,
                        card: Box::new(card),
                    },
                    created: None,
                })
            }

            Self::CreateBackground { name } => {
                let id = stack.add_background(name);
                Ok(Applied {
                    // A background nothing uses yet can be removed outright.
                    inverse: Self::DeleteBackground { id },
                    created: Some(ObjectId::new(ObjectKind::Background, id)),
                })
            }

            Self::InsertBackground { background } => {
                let id = background.id();
                stack.insert_background(*background);
                Ok(Applied {
                    inverse: Self::DeleteBackground { id },
                    created: Some(ObjectId::new(ObjectKind::Background, id)),
                })
            }

            Self::DeleteBackground { id } => {
                let background = stack.remove_background(id)?;
                Ok(Applied {
                    inverse: Self::InsertBackground {
                        background: Box::new(background),
                    },
                    created: None,
                })
            }

            Self::CreatePart {
                owner,
                kind,
                name,
                geometry,
            } => {
                let part = stack.new_part(kind, name, geometry);
                let id = part.id();
                container_mut(stack, owner)?.add_part(part);
                Ok(Applied {
                    inverse: Self::DeletePart { id },
                    created: Some(ObjectId::new(kind.object_kind(), id)),
                })
            }

            Self::InsertPart { owner, index, part } => {
                let id = part.id();
                let kind = part.part_kind();
                stack.reserve_id(id);
                container_mut(stack, owner)?.insert_part(index, *part);
                Ok(Applied {
                    inverse: Self::DeletePart { id },
                    created: Some(ObjectId::new(kind.object_kind(), id)),
                })
            }

            Self::DeletePart { id } => {
                let owner = owner_of(stack, id)?;
                let (index, part) = container_mut(stack, owner)?
                    .remove_part(id)
                    .ok_or_else(|| missing_part(id))?;
                Ok(Applied {
                    inverse: Self::InsertPart {
                        owner,
                        index,
                        part: Box::new(part),
                    },
                    created: None,
                })
            }

            Self::SetGeometry { id, geometry } => {
                let part = stack.part_mut(id).ok_or_else(|| missing_part(id))?;
                let previous = part.geometry();
                part.set_geometry(geometry);
                Ok(Applied {
                    inverse: Self::SetGeometry {
                        id,
                        geometry: previous,
                    },
                    created: None,
                })
            }

            Self::SetProperty {
                object,
                property,
                value,
            } => {
                let target = stack
                    .object_mut(object.kind, object.id)
                    .ok_or_else(|| missing_object(object))?;
                let previous = target.property(&property);
                match value {
                    Some(value) => target.set_property(&property, value)?,
                    None => {
                        target.core_mut().properties.remove(&property);
                        target.touch();
                    }
                }
                Ok(Applied {
                    inverse: Self::SetProperty {
                        object,
                        property,
                        value: previous,
                    },
                    created: None,
                })
            }

            Self::SetScript { object, script } => {
                let target = stack
                    .object_mut(object.kind, object.id)
                    .ok_or_else(|| missing_object(object))?;
                let previous = target.script().to_string();
                target.set_script(&script);
                Ok(Applied {
                    inverse: Self::SetScript {
                        object,
                        script: previous,
                    },
                    created: None,
                })
            }

            Self::Rename { object, name } => {
                let target = stack
                    .object_mut(object.kind, object.id)
                    .ok_or_else(|| missing_object(object))?;
                let previous = target.name().to_string();
                target.set_name(&name);
                Ok(Applied {
                    inverse: Self::Rename {
                        object,
                        name: previous,
                    },
                    created: None,
                })
            }

            Self::SetStackSize { size } => {
                let previous = stack.size();
                stack.set_size(size);
                Ok(Applied {
                    inverse: Self::SetStackSize { size: previous },
                    created: None,
                })
            }

            Self::SetImage { name, image } => {
                let previous = stack.set_image(&name, image.map(|image| *image));
                Ok(Applied {
                    // Putting back exactly what was there, including nothing:
                    // undoing an import must not leave the picture behind.
                    inverse: Self::SetImage {
                        name,
                        image: previous.map(Box::new),
                    },
                    created: None,
                })
            }
        }
    }
}

/// Looks up the container a command names.
fn container_mut(stack: &mut Stack, owner: PartOwner) -> RuntimeResult<&mut dyn PartContainer> {
    match owner {
        PartOwner::Card { id } => stack
            .card_mut(id)
            .map(|card| card as &mut dyn PartContainer)
            .ok_or_else(|| {
                RuntimeError::from(StackError::NoSuchObject {
                    kind: ObjectKind::Card,
                    id,
                })
            }),
        PartOwner::Background { id } => stack
            .background_mut(id)
            .map(|background| background as &mut dyn PartContainer)
            .ok_or_else(|| {
                RuntimeError::from(StackError::NoSuchObject {
                    kind: ObjectKind::Background,
                    id,
                })
            }),
    }
}

/// Works out who owns a part, so that deleting it can be undone.
fn owner_of(stack: &Stack, part: Id) -> RuntimeResult<PartOwner> {
    match stack.locate_part(part) {
        Some(hyperlab_stack::PartLocation::Card(id)) => Ok(PartOwner::Card { id }),
        Some(hyperlab_stack::PartLocation::Background(id)) => Ok(PartOwner::Background { id }),
        None => Err(missing_part(part)),
    }
}

fn missing_part(id: Id) -> RuntimeError {
    RuntimeError::new(format!("there is no part with id {id}"))
}

fn missing_object(object: ObjectId) -> RuntimeError {
    RuntimeError::new(format!("there is no {object}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack() -> Stack {
        Stack::new("Test")
    }

    fn apply(stack: &mut Stack, command: Command) -> Applied {
        command.apply(stack).expect("the command should apply")
    }

    #[test]
    fn creating_a_card_returns_a_command_that_deletes_it() {
        let mut stack = stack();
        let applied = apply(
            &mut stack,
            Command::CreateCard {
                after: 0,
                background: None,
            },
        );
        assert_eq!(stack.card_count(), 2);

        apply(&mut stack, applied.inverse);
        assert_eq!(stack.card_count(), 1);
    }

    #[test]
    fn deleting_a_card_can_be_undone_with_everything_on_it() {
        let mut stack = stack();
        apply(
            &mut stack,
            Command::CreateCard {
                after: 0,
                background: None,
            },
        );
        let second = stack.cards()[1].id();
        apply(
            &mut stack,
            Command::CreatePart {
                owner: PartOwner::Card { id: second },
                kind: PartKind::Field,
                name: "Notes".into(),
                geometry: Rect::new(1, 2, 3, 4),
            },
        );

        let applied = apply(&mut stack, Command::DeleteCard { id: second });
        assert_eq!(stack.card_count(), 1);

        apply(&mut stack, applied.inverse);
        assert_eq!(stack.card_count(), 2);
        let restored = stack.card(second).unwrap();
        assert_eq!(restored.parts().len(), 1, "the field came back too");
    }

    #[test]
    fn parts_come_back_at_the_layer_they_left() {
        let mut stack = stack();
        let card = stack.cards()[0].id();
        let owner = PartOwner::Card { id: card };
        let first = apply(
            &mut stack,
            Command::CreatePart {
                owner,
                kind: PartKind::Button,
                name: "A".into(),
                geometry: Rect::default(),
            },
        );
        apply(
            &mut stack,
            Command::CreatePart {
                owner,
                kind: PartKind::Button,
                name: "B".into(),
                geometry: Rect::default(),
            },
        );

        let first_id = first.created.unwrap().id;
        let applied = apply(&mut stack, Command::DeletePart { id: first_id });
        apply(&mut stack, applied.inverse);

        let parts = stack.card(card).unwrap().parts();
        assert_eq!(parts[0].name(), "A", "the part returned to the back");
        assert_eq!(parts[1].name(), "B");
    }

    #[test]
    fn undoing_a_property_that_did_not_exist_removes_it_again() {
        let mut stack = stack();
        let card = ObjectId::new(ObjectKind::Card, stack.cards()[0].id());
        let applied = apply(
            &mut stack,
            Command::SetProperty {
                object: card,
                property: "colour".into(),
                value: Some(Value::text("blue")),
            },
        );
        assert_eq!(
            stack.object(card.kind, card.id).unwrap().property("colour"),
            Some(Value::text("blue"))
        );

        apply(&mut stack, applied.inverse);
        assert_eq!(
            stack.object(card.kind, card.id).unwrap().property("colour"),
            None
        );
    }

    #[test]
    fn scripts_and_names_round_trip() {
        let mut stack = stack();
        let card = ObjectId::new(ObjectKind::Card, stack.cards()[0].id());
        let applied = apply(
            &mut stack,
            Command::SetScript {
                object: card,
                script: "on openCard\nend openCard".into(),
            },
        );
        apply(&mut stack, applied.inverse);
        assert_eq!(stack.object(card.kind, card.id).unwrap().script(), "");

        let applied = apply(
            &mut stack,
            Command::Rename {
                object: card,
                name: "Home".into(),
            },
        );
        assert_eq!(stack.object(card.kind, card.id).unwrap().name(), "Home");
        apply(&mut stack, applied.inverse);
        assert_eq!(stack.object(card.kind, card.id).unwrap().name(), "Card 1");
    }

    #[test]
    fn importing_a_picture_undoes_to_no_picture_at_all() {
        let mut stack = stack();
        let mark = Image::new("mark.svg", b"<svg/>".to_vec()).unwrap();

        let applied = Command::SetImage {
            name: "mark.svg".into(),
            image: Some(Box::new(mark.clone())),
        }
        .apply(&mut stack)
        .unwrap();
        assert_eq!(stack.image("mark.svg"), Some(&mark));

        apply(&mut stack, applied.inverse);
        assert!(
            stack.image("mark.svg").is_none(),
            "undoing an import must not leave the picture behind"
        );
    }

    #[test]
    fn replacing_a_picture_undoes_to_the_old_one() {
        let mut stack = stack();
        let first = Image::new("mark.svg", b"<svg id=\"1\"/>".to_vec()).unwrap();
        let second = Image::new("mark.svg", b"<svg id=\"2\"/>".to_vec()).unwrap();
        stack.set_image("mark.svg", Some(first.clone()));

        let applied = Command::SetImage {
            name: "mark.svg".into(),
            image: Some(Box::new(second)),
        }
        .apply(&mut stack)
        .unwrap();
        apply(&mut stack, applied.inverse);
        assert_eq!(stack.image("mark.svg"), Some(&first));
    }

    #[test]
    fn commands_that_cannot_apply_report_why() {
        let mut stack = stack();
        let only = stack.cards()[0].id();
        let error = Command::DeleteCard { id: only }
            .apply(&mut stack)
            .unwrap_err();
        assert!(error.message.contains("at least one card"), "{error}");

        let error = Command::DeletePart { id: Id::new(999) }
            .apply(&mut stack)
            .unwrap_err();
        assert!(error.message.contains("999"), "{error}");
    }
}
