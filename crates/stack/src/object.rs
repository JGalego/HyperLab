//! What every object has in common.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Id, PropertyBag, StackError, StackResult, Value, time::now_millis};

/// The kinds of object HyperLab knows about.
///
/// This list is deliberately short. New *behaviour* should arrive as new
/// properties or scripts; new *kinds* should be rare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectKind {
    /// The document itself.
    Stack,
    /// A layer shared by many cards.
    Background,
    /// A single card.
    Card,
    /// A clickable part.
    Button,
    /// An editable or displayed text part.
    Field,
    /// A picture.
    Image,
}

impl ObjectKind {
    /// The name used in scripts and saved files (`"card"`, `"button"`, ...).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stack => "stack",
            Self::Background => "background",
            Self::Card => "card",
            Self::Button => "button",
            Self::Field => "field",
            Self::Image => "image",
        }
    }
}

impl fmt::Display for ObjectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A fully qualified reference to one object: its kind plus its id.
///
/// Commands, the inspector and MCP tools all address objects this way, which
/// keeps them independent of where the object currently sits in the stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ObjectId {
    /// What kind of object this is.
    pub kind: ObjectKind,
    /// Its id within the stack.
    pub id: Id,
}

impl ObjectId {
    /// Creates a reference.
    #[must_use]
    pub const fn new(kind: ObjectKind, id: Id) -> Self {
        Self { kind, id }
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} id {}", self.kind, self.id)
    }
}

/// The state every object carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectCore {
    /// Unique within the stack, never reused.
    pub id: Id,
    /// A human-readable name. Names need not be unique, but scripts that
    /// address objects by name are clearer when they are.
    pub name: String,
    /// The object's HyperTalk source. Empty means "no handlers".
    pub script: String,
    /// Everything else.
    pub properties: PropertyBag,
    /// Creation time, in milliseconds since the Unix epoch.
    pub created_at: u64,
    /// Last modification time, in milliseconds since the Unix epoch.
    pub updated_at: u64,
}

impl ObjectCore {
    /// Creates a core with an empty script and no properties.
    pub fn new(id: Id, name: impl Into<String>) -> Self {
        let now = now_millis();
        Self {
            id,
            name: name.into(),
            script: String::new(),
            properties: PropertyBag::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

/// Behaviour shared by stacks, backgrounds, cards and parts.
///
/// Implementors provide access to their [`ObjectCore`] and their
/// [`ObjectKind`]; everything else is derived. Types that own geometry or
/// other intrinsic state override [`Object::intrinsic_property`] and
/// [`Object::set_intrinsic_property`] so that scripts can read and write it
/// through the same interface as ordinary properties.
pub trait Object {
    /// The shared state.
    fn core(&self) -> &ObjectCore;

    /// The shared state, mutably. Prefer the helpers below, which also stamp
    /// [`ObjectCore::updated_at`].
    fn core_mut(&mut self) -> &mut ObjectCore;

    /// What kind of object this is.
    fn kind(&self) -> ObjectKind;

    /// This object's id.
    fn id(&self) -> Id {
        self.core().id
    }

    /// A reference to this object.
    fn object_id(&self) -> ObjectId {
        ObjectId::new(self.kind(), self.id())
    }

    /// This object's name.
    fn name(&self) -> &str {
        &self.core().name
    }

    /// Renames the object.
    fn set_name(&mut self, name: &str) {
        self.core_mut().name = name.to_string();
        self.touch();
    }

    /// This object's HyperTalk source.
    fn script(&self) -> &str {
        &self.core().script
    }

    /// Replaces this object's HyperTalk source.
    fn set_script(&mut self, script: &str) {
        self.core_mut().script = script.to_string();
        self.touch();
    }

    /// Records that the object changed.
    fn touch(&mut self) {
        self.core_mut().updated_at = now_millis();
    }

    /// A property that is stored outside the [`PropertyBag`], such as
    /// geometry. Returns `None` for names the object does not treat specially.
    fn intrinsic_property(&self, _name: &str) -> Option<Value> {
        None
    }

    /// Writes an intrinsic property. Returns `Ok(false)` if the name is not
    /// intrinsic, in which case the caller falls back to the property bag.
    fn set_intrinsic_property(&mut self, _name: &str, _value: &Value) -> StackResult<bool> {
        Ok(false)
    }

    /// Reads any property: intrinsic, universal (`id`, `name`, `script`) or
    /// from the bag.
    fn property(&self, name: &str) -> Option<Value> {
        let name = PropertyBag::normalize(name);
        match name.as_str() {
            "id" => Some(Value::from(self.core().id.get() as i64)),
            "name" => Some(Value::text(self.name())),
            "script" => Some(Value::text(self.script())),
            _ => self
                .intrinsic_property(&name)
                .or_else(|| self.core().properties.get(&name).cloned()),
        }
    }

    /// Writes any property.
    ///
    /// The value is a [`Value`] rather than an `impl Into<Value>` so that the
    /// trait stays usable as `dyn Object`; call sites can write
    /// `set_property("visible", true.into())`.
    ///
    /// # Errors
    ///
    /// Returns [`StackError::ReadOnlyProperty`] for `id`, and
    /// [`StackError::InvalidPropertyValue`] when an intrinsic property rejects
    /// the value.
    fn set_property(&mut self, name: &str, value: Value) -> StackResult<()> {
        let name = PropertyBag::normalize(name);
        match name.as_str() {
            "id" => return Err(StackError::ReadOnlyProperty(name)),
            "name" => {
                self.core_mut().name = value.as_text();
            }
            "script" => {
                self.core_mut().script = value.as_text();
            }
            _ => {
                if !self.set_intrinsic_property(&name, &value)? {
                    self.core_mut().properties.set(&name, value);
                }
            }
        }
        self.touch();
        Ok(())
    }

    /// The names of every readable property, intrinsic ones first.
    ///
    /// The inspector uses this to build its property list without knowing
    /// anything about specific object kinds.
    fn property_names(&self) -> Vec<String> {
        let mut names = vec!["id".to_string(), "name".to_string()];
        names.extend(
            self.intrinsic_property_names()
                .into_iter()
                .map(String::from),
        );
        names.extend(self.core().properties.names().map(String::from));
        names.dedup();
        names
    }

    /// The intrinsic property names this object supports.
    fn intrinsic_property_names(&self) -> Vec<&'static str> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Thing(ObjectCore);

    impl Object for Thing {
        fn core(&self) -> &ObjectCore {
            &self.0
        }
        fn core_mut(&mut self) -> &mut ObjectCore {
            &mut self.0
        }
        fn kind(&self) -> ObjectKind {
            ObjectKind::Card
        }
    }

    fn thing() -> Thing {
        Thing(ObjectCore::new(Id::new(3), "Thing"))
    }

    #[test]
    fn universal_properties_are_readable() {
        let mut thing = thing();
        thing.set_script("on mouseUp\nend mouseUp");
        assert_eq!(thing.property("id"), Some(Value::Number(3.0)));
        assert_eq!(thing.property("NAME"), Some(Value::text("Thing")));
        assert!(
            thing
                .property("script")
                .unwrap()
                .as_text()
                .contains("mouseUp")
        );
    }

    #[test]
    fn the_id_cannot_be_written() {
        let mut thing = thing();
        assert_eq!(
            thing.set_property("id", 9.into()),
            Err(StackError::ReadOnlyProperty("id".into()))
        );
    }

    #[test]
    fn unknown_properties_land_in_the_bag() {
        let mut thing = thing();
        thing.set_property("hilite", true.into()).unwrap();
        assert_eq!(thing.property("hilite"), Some(Value::Bool(true)));
        assert_eq!(thing.property("nope"), None);
    }
}
