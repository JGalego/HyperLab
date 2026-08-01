//! Messages, and the path they travel along.
//!
//! HyperCard's central idea: a click does not call a function, it *sends a
//! message*, and the message travels outwards until something handles it.
//! Adding a new event to HyperLab means adding a name here and sending it —
//! no dispatch table, no registration, no runtime changes.

use hyperlab_stack::{Id, ObjectId, ObjectKind, Stack, Value};

/// A message, with its arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// The handler name to look for, such as `mouseUp`.
    pub name: String,
    /// Arguments, bound to the handler's parameters in order.
    pub arguments: Vec<Value>,
}

impl Message {
    /// A message with no arguments.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            arguments: Vec::new(),
        }
    }

    /// A message with arguments.
    pub fn with_arguments(name: impl Into<String>, arguments: Vec<Value>) -> Self {
        Self {
            name: name.into(),
            arguments,
        }
    }
}

/// The messages the shell sends. Scripts may send any name they like; these
/// are simply the ones HyperLab itself generates.
pub mod messages {
    /// The mouse went down on an object.
    pub const MOUSE_DOWN: &str = "mouseDown";
    /// The mouse came up on an object: the everyday "it was clicked".
    pub const MOUSE_UP: &str = "mouseUp";
    /// The pointer entered an object.
    pub const MOUSE_ENTER: &str = "mouseEnter";
    /// The pointer left an object.
    pub const MOUSE_LEAVE: &str = "mouseLeave";
    /// A card became the current card.
    pub const OPEN_CARD: &str = "openCard";
    /// A card stopped being the current card.
    pub const CLOSE_CARD: &str = "closeCard";
    /// A stack was opened.
    pub const OPEN_STACK: &str = "openStack";
    /// A stack is about to close.
    pub const CLOSE_STACK: &str = "closeStack";
    /// A key was pressed.
    pub const KEY_DOWN: &str = "keyDown";
    /// A field's contents changed.
    pub const FIELD_CHANGED: &str = "fieldChanged";
    /// A property changed.
    pub const PROPERTY_CHANGED: &str = "propertyChanged";
    /// Nothing is happening.
    pub const IDLE: &str = "idle";
}

/// The objects a message visits, in order, starting with the object it was
/// sent to.
///
/// ```text
/// button → card → background → stack
/// ```
///
/// A message that nobody handles simply reaches the end of the path and
/// stops. That is not an error: most objects ignore most messages.
#[must_use]
pub fn message_path(stack: &Stack, target: ObjectId, current_card: Id) -> Vec<ObjectId> {
    let mut path = vec![target];
    let stack_id = ObjectId::new(ObjectKind::Stack, hyperlab_stack::Object::id(stack));

    match target.kind {
        ObjectKind::Button | ObjectKind::Field | ObjectKind::Image => {
            // A part sits on either a card or a background. Either way the
            // message continues through the current card's layers.
            let card = stack
                .card(current_card)
                .map_or(current_card, hyperlab_stack::Object::id);
            path.push(ObjectId::new(ObjectKind::Card, card));
            if let Some(background) = stack.background_of(card) {
                path.push(ObjectId::new(
                    ObjectKind::Background,
                    hyperlab_stack::Object::id(background),
                ));
            }
            path.push(stack_id);
        }
        ObjectKind::Card => {
            if let Some(background) = stack.background_of(target.id) {
                path.push(ObjectId::new(
                    ObjectKind::Background,
                    hyperlab_stack::Object::id(background),
                ));
            }
            path.push(stack_id);
        }
        ObjectKind::Background => path.push(stack_id),
        ObjectKind::Stack => {}
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperlab_stack::{Object, PartContainer, PartKind, Rect};

    #[test]
    fn a_click_on_a_button_walks_out_to_the_stack() {
        let mut stack = Stack::new("Test");
        let card = stack.cards()[0].id();
        let background = stack.backgrounds()[0].id();
        let button = stack.new_part(PartKind::Button, "Go", Rect::default());
        let button_id = button.id();
        stack.card_mut(card).unwrap().add_part(button);

        let path = message_path(&stack, ObjectId::new(ObjectKind::Button, button_id), card);
        assert_eq!(
            path,
            vec![
                ObjectId::new(ObjectKind::Button, button_id),
                ObjectId::new(ObjectKind::Card, card),
                ObjectId::new(ObjectKind::Background, background),
                ObjectId::new(ObjectKind::Stack, stack.id()),
            ]
        );
    }

    #[test]
    fn a_message_to_the_stack_goes_nowhere_else() {
        let stack = Stack::new("Test");
        let card = stack.cards()[0].id();
        let path = message_path(&stack, ObjectId::new(ObjectKind::Stack, stack.id()), card);
        assert_eq!(path.len(), 1);
    }
}
