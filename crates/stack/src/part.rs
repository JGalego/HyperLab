//! Buttons and fields.

use serde::{Deserialize, Serialize};

use crate::{
    Id, Object, ObjectCore, ObjectKind, Rect, StackError, StackResult, Value, geometry::Size,
};

/// Which kind of part this is.
///
/// Buttons and fields share one type because they share almost everything:
/// geometry, properties, a script and a place in the message path. Only their
/// defaults and their rendering differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PartKind {
    /// Something to click.
    Button,
    /// Something to read or type into.
    Field,
}

impl PartKind {
    /// The matching [`ObjectKind`].
    #[must_use]
    pub const fn object_kind(self) -> ObjectKind {
        match self {
            Self::Button => ObjectKind::Button,
            Self::Field => ObjectKind::Field,
        }
    }

    /// The default size of a newly created part of this kind.
    #[must_use]
    pub const fn default_size(self) -> Size {
        match self {
            Self::Button => Size::new(96, 24),
            Self::Field => Size::new(160, 22),
        }
    }
}

/// A button or a field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Part {
    #[serde(flatten)]
    core: ObjectCore,
    kind: PartKind,
    geometry: Rect,
}

impl Part {
    /// Creates a part with the standard defaults for its kind.
    pub fn new(id: Id, kind: PartKind, name: impl Into<String>, geometry: Rect) -> Self {
        let mut part = Self {
            core: ObjectCore::new(id, name),
            kind,
            geometry,
        };
        part.apply_defaults();
        part
    }

    /// Whether this part is a button or a field.
    #[must_use]
    pub const fn part_kind(&self) -> PartKind {
        self.kind
    }

    /// Where the part sits on its card.
    #[must_use]
    pub const fn geometry(&self) -> Rect {
        self.geometry
    }

    /// Moves and resizes the part.
    pub fn set_geometry(&mut self, geometry: Rect) {
        self.geometry = geometry;
        self.touch();
    }

    /// The text a field holds, or the label a button shows.
    #[must_use]
    pub fn text(&self) -> String {
        match self.kind {
            PartKind::Field => self.property("text").unwrap_or(Value::Empty).as_text(),
            PartKind::Button => self.core.name.clone(),
        }
    }

    /// Fills in the properties every part of this kind is expected to have.
    ///
    /// Existing values are kept, so this is safe to call on parts loaded from
    /// an older file: it upgrades them to the current property set.
    pub fn apply_defaults(&mut self) {
        let bag = &mut self.core.properties;
        bag.set_default("visible", true);
        bag.set_default("enabled", true);
        match self.kind {
            PartKind::Button => {
                bag.set_default("style", "roundRect");
                bag.set_default("showName", true);
                bag.set_default("hilite", false);
                bag.set_default("autoHilite", true);
            }
            PartKind::Field => {
                bag.set_default("style", "rectangle");
                bag.set_default("text", Value::Empty);
                bag.set_default("locked", false);
                bag.set_default("wrap", true);
            }
        }
    }
}

impl Object for Part {
    fn core(&self) -> &ObjectCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ObjectCore {
        &mut self.core
    }

    fn kind(&self) -> ObjectKind {
        self.kind.object_kind()
    }

    fn intrinsic_property(&self, name: &str) -> Option<Value> {
        let rect = self.geometry;
        match name {
            "left" => Some(rect.left.into()),
            "top" => Some(rect.top.into()),
            "width" => Some(rect.width.into()),
            "height" => Some(rect.height.into()),
            "right" => Some(rect.right().into()),
            "bottom" => Some(rect.bottom().into()),
            "rect" | "rectangle" => Some(Value::text(format!(
                "{},{},{},{}",
                rect.left,
                rect.top,
                rect.right(),
                rect.bottom()
            ))),
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
        let mut rect = self.geometry;
        match name {
            "left" => rect.left = number()?,
            "top" => rect.top = number()?,
            "width" => rect = rect.resized(Size::new(number()?, rect.height)),
            "height" => rect = rect.resized(Size::new(rect.width, number()?)),
            "right" => rect.left = number()? - rect.width,
            "bottom" => rect.top = number()? - rect.height,
            "rect" | "rectangle" => rect = parse_rect(name, value)?,
            _ => return Ok(false),
        }
        self.geometry = rect;
        Ok(true)
    }

    fn intrinsic_property_names(&self) -> Vec<&'static str> {
        vec!["left", "top", "width", "height", "right", "bottom"]
    }
}

/// Parses `"left,top,right,bottom"`, the form HyperTalk uses for rectangles.
fn parse_rect(property: &str, value: &Value) -> StackResult<Rect> {
    let text = value.as_text();
    let numbers: Vec<i32> = text
        .split(',')
        .filter_map(|part| part.trim().parse().ok())
        .collect();
    if numbers.len() == 4 {
        Ok(Rect::new(
            numbers[0],
            numbers[1],
            numbers[2] - numbers[0],
            numbers[3] - numbers[1],
        ))
    } else {
        Err(StackError::InvalidPropertyValue {
            property: property.to_string(),
            reason: format!("expected \"left,top,right,bottom\", got \"{text}\""),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button() -> Part {
        Part::new(
            Id::new(1),
            PartKind::Button,
            "Go",
            Rect::new(10, 20, 100, 30),
        )
    }

    #[test]
    fn defaults_depend_on_the_kind() {
        let button = button();
        assert_eq!(button.property("style"), Some(Value::text("roundRect")));
        assert_eq!(button.property("text"), None);

        let field = Part::new(Id::new(2), PartKind::Field, "Notes", Rect::default());
        assert_eq!(field.property("style"), Some(Value::text("rectangle")));
        assert_eq!(field.property("text"), Some(Value::Empty));
    }

    #[test]
    fn geometry_is_reachable_as_properties() {
        let mut button = button();
        assert_eq!(button.property("left"), Some(Value::Number(10.0)));
        assert_eq!(button.property("right"), Some(Value::Number(110.0)));

        button.set_property("width", 50.into()).unwrap();
        assert_eq!(button.geometry(), Rect::new(10, 20, 50, 30));

        button.set_property("rect", "0,0,20,10".into()).unwrap();
        assert_eq!(button.geometry(), Rect::new(0, 0, 20, 10));
    }

    #[test]
    fn moving_by_the_right_edge_preserves_the_width() {
        let mut button = button();
        button.set_property("right", 200.into()).unwrap();
        assert_eq!(button.geometry(), Rect::new(100, 20, 100, 30));
    }

    #[test]
    fn geometry_rejects_nonsense() {
        let mut button = button();
        assert!(button.set_property("width", "wide".into()).is_err());
        assert!(button.set_property("rect", "1,2,3".into()).is_err());
    }

    #[test]
    fn applying_defaults_twice_keeps_edits() {
        let mut field = Part::new(Id::new(2), PartKind::Field, "Notes", Rect::default());
        field.set_property("text", "hello".into()).unwrap();
        field.apply_defaults();
        assert_eq!(field.text(), "hello");
    }
}
