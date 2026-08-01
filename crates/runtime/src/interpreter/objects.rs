//! Resolving object references, and reading and writing containers.
//!
//! This is where a script's words become objects: `field "Name" of card 3`
//! turns into an [`ObjectId`]. Nothing here evaluates statements, and nothing
//! here mutates a stack directly — writes go out through commands.

use hyperlab_parser::ast::{
    Chunk, Container, ContainerBase, Expr, Layer, ObjectRef, Ordinal, PartKind as AstPartKind,
    Specifier,
};
use hyperlab_stack::{
    Id, Object, ObjectId, ObjectKind, Part, PartContainer, PartKind, PropertyBag, Value,
};

use super::{Interpreter, builtins};
use crate::{
    chunk::{self, Slice},
    error::{RuntimeError, RuntimeResult},
};

/// A specifier with its expressions already evaluated.
enum Resolved {
    /// `this card`, or nothing at all.
    Current,
    /// `id 12`.
    Id(Id),
    /// `card 3`: a position, counting from one.
    Number(i64),
    /// `card "Home"`.
    Name(String),
    /// `first`, `last`, …
    Ordinal(Ordinal),
}

impl Interpreter<'_> {
    /// Turns a reference written in a script into the object it names.
    pub(crate) fn resolve_object(&mut self, reference: &ObjectRef) -> RuntimeResult<ObjectId> {
        match reference {
            ObjectRef::Me => self.me(),
            ObjectRef::Target => Ok(self.frame()?.target),
            ObjectRef::Stack => Ok(ObjectId::new(ObjectKind::Stack, self.runtime.stack().id())),
            ObjectRef::Card(specifier) => Ok(ObjectId::new(
                ObjectKind::Card,
                self.resolve_card(specifier)?,
            )),
            ObjectRef::Background(specifier) => Ok(ObjectId::new(
                ObjectKind::Background,
                self.resolve_background(specifier)?,
            )),
            ObjectRef::Part {
                kind,
                layer,
                specifier,
                owner,
            } => self.resolve_part(*kind, *layer, specifier, owner.as_deref()),
        }
    }

    /// Finds a card.
    pub(crate) fn resolve_card(&mut self, specifier: &Specifier) -> RuntimeResult<Id> {
        let resolved = self.resolve_specifier(specifier)?;
        let stack = self.runtime.stack();
        let count = stack.card_count();
        match resolved {
            Resolved::Current => Ok(self.runtime.current_card()),
            Resolved::Id(id) => stack
                .card(id)
                .map(Object::id)
                .ok_or_else(|| RuntimeError::new(format!("there is no card with id {id}"))),
            Resolved::Number(position) => stack
                .card_at(usize::try_from(position - 1).unwrap_or(usize::MAX))
                .map(Object::id)
                .ok_or_else(|| RuntimeError::new(format!("there is no card {position}"))),
            Resolved::Name(name) => stack
                .card_named(&name)
                .map(Object::id)
                .ok_or_else(|| RuntimeError::new(format!("there is no card named \"{name}\""))),
            Resolved::Ordinal(ordinal) => {
                let current = self.runtime.current_card_index();
                let index = relative_index(ordinal, current, count)
                    .ok_or_else(|| RuntimeError::new("this stack does not have a card there"))?;
                Ok(self.runtime.stack().cards()[index].id())
            }
        }
    }

    /// Finds a background.
    fn resolve_background(&mut self, specifier: &Specifier) -> RuntimeResult<Id> {
        let resolved = self.resolve_specifier(specifier)?;
        let current = self.runtime.current_card();
        let stack = self.runtime.stack();
        match resolved {
            Resolved::Current => stack
                .background_of(current)
                .map(Object::id)
                .ok_or_else(|| RuntimeError::new("this card has no background")),
            Resolved::Id(id) => stack
                .background(id)
                .map(Object::id)
                .ok_or_else(|| RuntimeError::new(format!("there is no background with id {id}"))),
            Resolved::Number(position) => stack
                .backgrounds()
                .get(usize::try_from(position - 1).unwrap_or(usize::MAX))
                .map(Object::id)
                .ok_or_else(|| RuntimeError::new(format!("there is no background {position}"))),
            Resolved::Name(name) => {
                stack
                    .background_named(&name)
                    .map(Object::id)
                    .ok_or_else(|| {
                        RuntimeError::new(format!("there is no background named \"{name}\""))
                    })
            }
            Resolved::Ordinal(ordinal) => {
                let count = stack.backgrounds().len();
                let index = ordinal
                    .index(count)
                    .or_else(|| relative_index(ordinal, 0, count))
                    .ok_or_else(|| {
                        RuntimeError::new("this stack does not have a background there")
                    })?;
                Ok(self.runtime.stack().backgrounds()[index].id())
            }
        }
    }

    /// Finds a button or a field.
    ///
    /// With no explicit layer, the card is searched before its background —
    /// the rule that lets a card override one field of a shared layout.
    fn resolve_part(
        &mut self,
        kind: AstPartKind,
        layer: Layer,
        specifier: &Specifier,
        owner: Option<&ObjectRef>,
    ) -> RuntimeResult<ObjectId> {
        let kind = part_kind(kind);
        let resolved = self.resolve_specifier(specifier)?;

        // Work out which card and background to look in.
        let (card, background) = match owner {
            Some(reference) => {
                let owner = self.resolve_object(reference)?;
                match owner.kind {
                    ObjectKind::Card => (
                        Some(owner.id),
                        self.runtime.stack().background_of(owner.id).map(Object::id),
                    ),
                    ObjectKind::Background => (None, Some(owner.id)),
                    _ => {
                        return Err(RuntimeError::new(format!(
                            "{owner} does not have buttons or fields"
                        )));
                    }
                }
            }
            None => {
                let card = self.runtime.current_card();
                (
                    Some(card),
                    self.runtime.stack().background_of(card).map(Object::id),
                )
            }
        };

        let search_card = matches!(layer, Layer::Card | Layer::Unspecified);
        let search_background = matches!(layer, Layer::Background | Layer::Unspecified);

        if search_card
            && let Some(card) = card.and_then(|id| self.runtime.stack().card(id))
            && let Some(part) = find_part(card, kind, &resolved)
        {
            return Ok(ObjectId::new(kind.object_kind(), part));
        }
        if search_background
            && let Some(background) = background.and_then(|id| self.runtime.stack().background(id))
            && let Some(part) = find_part(background, kind, &resolved)
        {
            return Ok(ObjectId::new(kind.object_kind(), part));
        }
        Err(RuntimeError::new(format!(
            "I cannot find {kind_name} {description}",
            kind_name = kind.object_kind(),
            description = describe(&resolved),
        )))
    }

    /// Evaluates the expressions inside a specifier.
    fn resolve_specifier(&mut self, specifier: &Specifier) -> RuntimeResult<Resolved> {
        Ok(match specifier {
            Specifier::Current => Resolved::Current,
            Specifier::Ordinal(ordinal) => Resolved::Ordinal(*ordinal),
            Specifier::Id(expression) => {
                let value = self.evaluate(expression)?;
                let number = value
                    .as_number()
                    .ok_or_else(|| RuntimeError::new(format!("\"{value}\" is not an id")))?;
                Resolved::Id(Id::new(number.max(0.0) as u64))
            }
            Specifier::Value(expression) => {
                // A quoted string always means a name, even when it looks
                // like a number: `field "3"` is not `field 3`.
                if let Expr::Text(text) = expression {
                    Resolved::Name(text.clone())
                } else {
                    let value = self.evaluate(expression)?;
                    match value.as_number() {
                        Some(number) => Resolved::Number(number as i64),
                        None => Resolved::Name(value.as_text()),
                    }
                }
            }
        })
    }

    // ----------------------------------------------------------- containers

    /// Reads whatever a container currently holds.
    pub(crate) fn read_container(&mut self, container: &Container) -> RuntimeResult<Value> {
        let base = self.read_container_base(&container.base)?;
        if container.chunks.is_empty() {
            return Ok(base);
        }
        let slices = self.slices(&container.chunks)?;
        Ok(Value::text(chunk::extract_nested(&base.as_text(), &slices)))
    }

    fn read_container_base(&mut self, base: &ContainerBase) -> RuntimeResult<Value> {
        Ok(match base {
            ContainerBase::Variable(name) => self.variable(name).unwrap_or(Value::Empty),
            ContainerBase::It => self.variable("it").unwrap_or(Value::Empty),
            ContainerBase::MessageBox => Value::text(self.runtime.message_box()),
            ContainerBase::Object(reference) => {
                let object = self.resolve_object(reference)?;
                self.contents_of(object)?
            }
        })
    }

    /// Stores a value in a container, honouring `into`, `before` and `after`.
    pub(crate) fn write_container(
        &mut self,
        container: &Container,
        value: &Value,
        preposition: hyperlab_parser::ast::Preposition,
    ) -> RuntimeResult<()> {
        use hyperlab_parser::ast::Preposition;

        if container.chunks.is_empty() {
            let new_value = match preposition {
                Preposition::Into => value.clone(),
                Preposition::Before | Preposition::After => {
                    let existing = self.read_container_base(&container.base)?.as_text();
                    let added = value.as_text();
                    Value::text(match preposition {
                        Preposition::Before => format!("{added}{existing}"),
                        _ => format!("{existing}{added}"),
                    })
                }
            };
            return self.write_container_base(&container.base, new_value);
        }

        let slices = self.slices(&container.chunks)?;
        let base_text = self.read_container_base(&container.base)?.as_text();
        let existing = chunk::extract_nested(&base_text, &slices);
        let added = value.as_text();
        let replacement = match preposition {
            Preposition::Into => added,
            Preposition::Before => format!("{added}{existing}"),
            Preposition::After => format!("{existing}{added}"),
        };
        let updated = chunk::replace_nested(&base_text, &slices, &replacement);
        self.write_container_base(&container.base, Value::text(updated))
    }

    fn write_container_base(&mut self, base: &ContainerBase, value: Value) -> RuntimeResult<()> {
        match base {
            ContainerBase::Variable(name) => self.set_variable(name, value),
            ContainerBase::It => self.set_variable("it", value),
            ContainerBase::MessageBox => {
                self.runtime.set_message_box(value.as_text());
                Ok(())
            }
            ContainerBase::Object(reference) => {
                let object = self.resolve_object(reference)?;
                if object.kind == ObjectKind::Field {
                    self.set_property(object, "text", value)
                } else {
                    Err(RuntimeError::new(format!(
                        "I cannot put anything into {object}; only fields hold text"
                    )))
                }
            }
        }
    }

    /// Evaluates the bounds of every chunk in a chunk expression.
    pub(crate) fn slices(&mut self, chunks: &[Chunk]) -> RuntimeResult<Vec<Slice>> {
        let mut slices = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            let start = self.number(&chunk.start)? as i64;
            let end = match &chunk.end {
                Some(expression) => Some(self.number(expression)? as i64),
                None => None,
            };
            slices.push(Slice {
                kind: chunk.kind,
                start,
                end,
            });
        }
        Ok(slices)
    }

    // ----------------------------------------------------------- properties

    /// What an object is worth when used as a value: a field's text, or any
    /// other object's name.
    pub(crate) fn contents_of(&mut self, object: ObjectId) -> RuntimeResult<Value> {
        match object.kind {
            ObjectKind::Field => self.property(object, "text"),
            _ => Ok(Value::text(self.runtime.object(object)?.name())),
        }
    }

    /// Reads a property, including the few that are computed rather than
    /// stored.
    pub(crate) fn property(&mut self, object: ObjectId, name: &str) -> RuntimeResult<Value> {
        let name = PropertyBag::normalize(name);
        match name.as_str() {
            "number" => return self.number_of(object),
            "owner" => return self.owner_of(object),
            _ => {}
        }
        let target = self.runtime.object(object)?;
        if let Some(value) = target.property(&name) {
            return Ok(value);
        }
        // Fields answer to `contents` as well as `text`, as HyperCard's do.
        if name == "contents"
            && let Some(value) = target.property("text")
        {
            return Ok(value);
        }
        Err(RuntimeError::new(format!(
            "{object} has no property called \"{name}\""
        )))
    }

    /// `the number of` an object: its position among its siblings.
    fn number_of(&self, object: ObjectId) -> RuntimeResult<Value> {
        let stack = self.runtime.stack();
        let position = match object.kind {
            ObjectKind::Card => stack.card_index(object.id).map(|index| index + 1),
            ObjectKind::Background => stack
                .backgrounds()
                .iter()
                .position(|background| background.id() == object.id)
                .map(|index| index + 1),
            ObjectKind::Button | ObjectKind::Field | ObjectKind::Image => {
                match stack.locate_part(object.id) {
                    Some(hyperlab_stack::PartLocation::Card(card)) => stack
                        .card(card)
                        .and_then(|card| card.part_number(object.id)),
                    Some(hyperlab_stack::PartLocation::Background(background)) => stack
                        .background(background)
                        .and_then(|background| background.part_number(object.id)),
                    None => None,
                }
            }
            ObjectKind::Stack => Some(1),
        };
        position
            .map(Value::from)
            .ok_or_else(|| RuntimeError::new(format!("I cannot find {object}")))
    }

    /// `the owner of` an object: the card or background it belongs to.
    fn owner_of(&self, object: ObjectId) -> RuntimeResult<Value> {
        let stack = self.runtime.stack();
        let owner = match object.kind {
            ObjectKind::Button | ObjectKind::Field | ObjectKind::Image => {
                match stack.locate_part(object.id) {
                    Some(hyperlab_stack::PartLocation::Card(card)) => stack
                        .card(card)
                        .map(|card| format!("card \"{}\"", card.name())),
                    Some(hyperlab_stack::PartLocation::Background(background)) => stack
                        .background(background)
                        .map(|background| format!("background \"{}\"", background.name())),
                    None => None,
                }
            }
            ObjectKind::Card => stack
                .background_of(object.id)
                .map(|background| format!("background \"{}\"", background.name())),
            ObjectKind::Background | ObjectKind::Stack => {
                Some(format!("stack \"{}\"", stack.name()))
            }
        };
        owner
            .map(Value::text)
            .ok_or_else(|| RuntimeError::new(format!("I cannot find the owner of {object}")))
    }

    /// Whether an object reference names something that exists, for
    /// `there is a …`.
    pub(crate) fn object_exists(&mut self, reference: &ObjectRef) -> bool {
        self.resolve_object(reference).is_ok()
    }
}

/// Looks for a part in one container.
fn find_part(container: &dyn PartContainer, kind: PartKind, resolved: &Resolved) -> Option<Id> {
    let part: Option<&Part> = match resolved {
        Resolved::Current => container.parts_of_kind(kind).first().copied(),
        Resolved::Id(id) => container.part(*id).filter(|part| part.part_kind() == kind),
        Resolved::Number(position) => {
            container.part_numbered(kind, usize::try_from(*position).ok()?)
        }
        Resolved::Name(name) => container.part_named(kind, name),
        Resolved::Ordinal(ordinal) => {
            let parts = container.parts_of_kind(kind);
            let index = ordinal.index(parts.len()).or_else(|| {
                builtins::random_index(parts.len()).filter(|_| *ordinal == Ordinal::Any)
            })?;
            parts.get(index).copied()
        }
    };
    part.map(Object::id)
}

/// Resolves the ordinals that depend on where we are now.
fn relative_index(ordinal: Ordinal, current: usize, count: usize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    match ordinal {
        Ordinal::Next => Some((current + 1) % count),
        Ordinal::Previous => Some((current + count - 1) % count),
        Ordinal::Any => builtins::random_index(count),
        other => other.index(count),
    }
}

/// Describes a specifier for an error message.
fn describe(resolved: &Resolved) -> String {
    match resolved {
        Resolved::Current => "here".to_string(),
        Resolved::Id(id) => format!("id {id}"),
        Resolved::Number(position) => format!("number {position}"),
        Resolved::Name(name) => format!("\"{name}\""),
        Resolved::Ordinal(_) => "there".to_string(),
    }
}

/// The object model's part kind for the one the grammar produced.
///
/// The two enums are deliberately separate — the parser depends on nothing —
/// so something has to join them, and it should be one thing.
pub(crate) const fn part_kind(written: AstPartKind) -> PartKind {
    match written {
        AstPartKind::Button => PartKind::Button,
        AstPartKind::Field => PartKind::Field,
        AstPartKind::Image => PartKind::Image,
    }
}
