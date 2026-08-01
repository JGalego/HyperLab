//! What the renderer is allowed to see.
//!
//! The frontend never receives a `Stack`. It receives a [`StackView`]: a flat,
//! serializable snapshot of the current card, taken after every change.
//!
//! This is the "view model" step in the architecture:
//!
//! ```text
//! Runtime → View Model → Renderer
//! ```
//!
//! It exists so that the renderer cannot hold a reference to anything the
//! runtime owns, cannot mutate anything by accident, and does not break when
//! the object model changes shape.

use hyperlab_runtime::Runtime;
use hyperlab_stack::{Object, PartContainer, PartKind, Rect, Stack, Value};
use serde::Serialize;

/// Everything the window needs to draw itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackView {
    /// The stack's name, shown in the title bar.
    pub stack_name: String,
    /// The stack's id, so the inspector can address it.
    pub stack_id: u64,
    /// The stack's script.
    pub stack_script: String,
    /// The size every card is drawn at.
    pub card_size: SizeView,
    /// How many cards there are.
    pub card_count: usize,
    /// Which one is showing, counting from one.
    pub card_number: usize,
    /// The current card.
    pub card: CardView,
    /// The background beneath it.
    pub background: Option<CardView>,
    /// The message box.
    pub message_box: String,
    /// Whether there is anything to undo, and what.
    pub undo: Option<String>,
    /// Whether there is anything to redo, and what.
    pub redo: Option<String>,
    /// Whether there are unsaved changes.
    pub dirty: bool,
    /// Where the stack is saved, if it has been saved.
    pub path: Option<String>,
}

/// A width and a height.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct SizeView {
    /// Width in card-space pixels.
    pub width: i32,
    /// Height in card-space pixels.
    pub height: i32,
}

/// A card or a background, with everything on it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardView {
    /// Its id.
    pub id: u64,
    /// `"card"` or `"background"`.
    pub kind: String,
    /// Its name.
    pub name: String,
    /// Its script.
    pub script: String,
    /// Its buttons and fields, furthest back first.
    pub parts: Vec<PartView>,
}

/// One button or field, ready to draw.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartView {
    /// Its id.
    pub id: u64,
    /// `"button"` or `"field"`.
    pub kind: String,
    /// Whether it belongs to the card or to the background, which decides
    /// whether editing it changes every card.
    pub layer: String,
    /// Its name, which is also a button's label.
    pub name: String,
    /// A field's contents.
    pub text: String,
    /// Where it sits: left, top, width, height.
    pub rect: [i32; 4],
    /// Whether it is drawn at all.
    pub visible: bool,
    /// Whether it responds to the mouse.
    pub enabled: bool,
    /// How it is drawn: `roundRect`, `rectangle`, `transparent`, …
    pub style: String,
    /// Whether a field refuses typing.
    pub locked: bool,
    /// Its script, so the inspector need not ask again.
    pub script: String,
    /// Every property, for the property editor. Values are JSON so that
    /// booleans stay booleans.
    pub properties: Vec<PropertyView>,
}

/// One row of the property editor.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyView {
    /// The property's name.
    pub name: String,
    /// Its value.
    pub value: serde_json::Value,
    /// Whether the editor should refuse to change it.
    pub read_only: bool,
}

/// Takes a snapshot of the runtime.
#[must_use]
pub fn snapshot(runtime: &Runtime, dirty: bool, path: Option<String>) -> StackView {
    let stack = runtime.stack();
    let card_id = runtime.current_card();
    let card = stack.card(card_id);

    StackView {
        stack_name: stack.name().to_string(),
        stack_id: stack.id().get(),
        stack_script: stack.script().to_string(),
        card_size: SizeView {
            width: stack.size().width,
            height: stack.size().height,
        },
        card_count: stack.card_count(),
        card_number: runtime.current_card_index() + 1,
        card: card.map_or_else(
            || CardView {
                id: 0,
                kind: "card".into(),
                name: String::new(),
                script: String::new(),
                parts: Vec::new(),
            },
            |card| container_view(stack, card, "card"),
        ),
        background: stack
            .background_of(card_id)
            .map(|background| container_view(stack, background, "background")),
        message_box: runtime.message_box().to_string(),
        undo: runtime.history().undo_label().map(str::to_string),
        redo: runtime.history().redo_label().map(str::to_string),
        dirty,
        path,
    }
}

fn container_view<T>(_stack: &Stack, container: &T, kind: &str) -> CardView
where
    T: Object + PartContainer,
{
    CardView {
        id: container.id().get(),
        kind: kind.to_string(),
        name: container.name().to_string(),
        script: container.script().to_string(),
        parts: container
            .parts()
            .iter()
            .map(|part| part_view(part, kind))
            .collect(),
    }
}

fn part_view(part: &hyperlab_stack::Part, layer: &str) -> PartView {
    let rect: Rect = part.geometry();
    let flag = |name: &str, fallback: bool| {
        part.property(name)
            .and_then(|value| value.as_bool())
            .unwrap_or(fallback)
    };
    PartView {
        id: part.id().get(),
        kind: part.kind().as_str().to_string(),
        layer: layer.to_string(),
        name: part.name().to_string(),
        text: if part.part_kind() == PartKind::Field {
            part.text()
        } else {
            String::new()
        },
        rect: [rect.left, rect.top, rect.width, rect.height],
        visible: flag("visible", true),
        enabled: flag("enabled", true),
        style: part
            .property("style")
            .unwrap_or(Value::Empty)
            .as_text(),
        locked: flag("locked", false),
        script: part.script().to_string(),
        properties: properties_of(part),
    }
}

/// Lists an object's properties for the inspector.
///
/// The inspector shows whatever is there, including properties this version
/// of HyperLab has never heard of, which is what makes the object model
/// extensible in practice rather than only in principle.
#[must_use]
pub fn properties_of(object: &dyn Object) -> Vec<PropertyView> {
    object
        .property_names()
        .into_iter()
        .filter(|name| name != "script")
        .filter_map(|name| {
            let value = object.property(&name)?;
            Some(PropertyView {
                read_only: name == "id",
                value: match value {
                    Value::Bool(flag) => serde_json::Value::Bool(flag),
                    Value::Number(number) => serde_json::json!(number),
                    other => serde_json::Value::String(other.as_text()),
                },
                name,
            })
        })
        .collect()
}
