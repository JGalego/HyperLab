//! The tools HyperLab offers.
//!
//! Each tool is a thin wrapper around a runtime command or query. That is the
//! whole point: an assistant driving HyperLab through these tools can do
//! exactly what a person can do through the interface — no more, and with the
//! same undo history.

use hyperlab_ai::ToolDefinition;
use hyperlab_runtime::{Command, Message, PartOwner, Runtime};
use hyperlab_stack::{
    Id, Object, ObjectId, ObjectKind, PartContainer, PartKind, Rect, Stack, Value,
};
use serde_json::{Value as Json, json};

use crate::{
    error::{ToolError, ToolResult},
    permission::Access,
};

/// One tool: how to describe it, and what to do when it is called.
pub struct Tool {
    /// The name a caller uses.
    pub name: &'static str,
    /// What it does, and when to use it.
    pub description: &'static str,
    /// Whether calling it can change the stack.
    ///
    /// Declared here rather than worked out from the name, so that a
    /// [`Policy`](crate::Policy) enforcing "read only" is held in step by the
    /// compiler: a new tool cannot be added without saying which it is.
    pub access: Access,
    /// A JSON Schema for its arguments.
    pub schema: fn() -> Json,
    /// What it does.
    pub run: fn(&mut Runtime, &Json) -> ToolResult<Json>,
}

impl Tool {
    /// The tool as a model sees it.
    #[must_use]
    pub fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name, self.description, (self.schema)())
    }
}

/// Every tool HyperLab offers, in a sensible order for a reader.
pub const TOOLS: &[Tool] = &[
    Tool {
        name: "current_card",
        access: Access::Read,
        description: "Describe the card the user is looking at: its name, id, position, \
                      and the buttons and fields on it and on its background.",
        schema: || json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        run: current_card,
    },
    Tool {
        name: "list_cards",
        access: Access::Read,
        description: "List every card in the stack, in order, with its id and name.",
        schema: || json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        run: list_cards,
    },
    Tool {
        name: "read_field",
        access: Access::Read,
        description: "Read the text of a field on the current card or its background. \
                      Give either a name or an id.",
        schema: field_locator_schema,
        run: read_field,
    },
    Tool {
        name: "write_field",
        access: Access::Write,
        description: "Replace the text of a field. This is undoable, exactly as if the \
                      user had typed it.",
        schema: || {
            let mut schema = field_locator_schema();
            schema["properties"]["text"] =
                json!({ "type": "string", "description": "The new contents." });
            schema["required"] = json!(["text"]);
            schema
        },
        run: write_field,
    },
    Tool {
        name: "create_card",
        access: Access::Write,
        description: "Add a new card after the current one, on the same background.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "What to call it." }
                },
                "additionalProperties": false
            })
        },
        run: create_card,
    },
    Tool {
        name: "create_button",
        access: Access::Write,
        description: "Add a button to the current card.",
        schema: || part_schema("button"),
        run: |runtime, arguments| create_part(runtime, arguments, PartKind::Button),
    },
    Tool {
        name: "create_field",
        access: Access::Write,
        description: "Add a field to the current card.",
        schema: || part_schema("field"),
        run: |runtime, arguments| create_part(runtime, arguments, PartKind::Field),
    },
    Tool {
        name: "set_property",
        access: Access::Write,
        description: "Set a property of an object: visible, enabled, style, width, left, \
                      script, name, and so on. Use current_card to see what exists.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "description": "The object's id." },
                    "kind": {
                        "type": "string",
                        "enum": ["stack", "background", "card", "button", "field"]
                    },
                    "property": { "type": "string" },
                    "value": {
                        "description": "The new value: a string, number or boolean."
                    }
                },
                "required": ["id", "kind", "property", "value"],
                "additionalProperties": false
            })
        },
        run: set_property,
    },
    Tool {
        name: "run_script",
        access: Access::Write,
        description: "Run a fragment of HyperTalk as if it were a handler on the current \
                      card. Use this for anything the other tools do not cover.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "script": {
                        "type": "string",
                        "description": "HyperTalk statements, one per line."
                    }
                },
                "required": ["script"],
                "additionalProperties": false
            })
        },
        run: run_script,
    },
    Tool {
        name: "send_message",
        access: Access::Write,
        description: "Send a message such as mouseUp to an object, exactly as clicking it \
                      would.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string", "description": "For example, mouseUp." },
                    "id": { "type": "integer", "description": "The object's id." },
                    "kind": {
                        "type": "string",
                        "enum": ["stack", "background", "card", "button", "field"]
                    }
                },
                "required": ["message", "id", "kind"],
                "additionalProperties": false
            })
        },
        run: send_message,
    },
    Tool {
        name: "find_cards",
        access: Access::Read,
        description: "Find cards whose name, field contents or script contain some text. \
                      Returns the cards that matched and where the text was found.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "default": 20 }
                },
                "required": ["text"],
                "additionalProperties": false
            })
        },
        run: find_cards,
    },
    Tool {
        name: "go_to_card",
        access: Access::Write,
        description: "Show a different card, by id or by position.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "description": "The card's id." },
                    "position": { "type": "integer", "description": "Counting from one." }
                },
                "additionalProperties": false
            })
        },
        run: go_to_card,
    },
    Tool {
        name: "undo",
        access: Access::Write,
        description: "Undo the last change, whoever made it.",
        schema: || json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        run: |runtime, _| {
            let undone = runtime.undo()?;
            Ok(json!({ "undone": undone }))
        },
    },
];

// ------------------------------------------------------------------ schemas

fn field_locator_schema() -> Json {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string", "description": "The field's name." },
            "id": { "type": "integer", "description": "The field's id." }
        },
        "additionalProperties": false
    })
}

fn part_schema(what: &str) -> Json {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string", "description": format!("What to call the {what}.") },
            "left": { "type": "integer", "default": 20 },
            "top": { "type": "integer", "default": 20 },
            "width": { "type": "integer" },
            "height": { "type": "integer" },
            "script": { "type": "string", "description": "HyperTalk handlers, optional." }
        },
        "required": ["name"],
        "additionalProperties": false
    })
}

// -------------------------------------------------------------- the tools

fn current_card(runtime: &mut Runtime, _arguments: &Json) -> ToolResult<Json> {
    let stack = runtime.stack();
    let card_id = runtime.current_card();
    let card = stack
        .card(card_id)
        .ok_or_else(|| ToolError::Runtime("there is no current card".into()))?;

    Ok(json!({
        "stack": stack.name(),
        "card": {
            "id": card.id().get(),
            "name": card.name(),
            "position": stack.card_index(card_id).map_or(1, |index| index + 1),
            "of": stack.card_count(),
            "script": card.script(),
            "parts": describe_parts(card),
        },
        "background": stack.background_of(card_id).map(|background| json!({
            "id": background.id().get(),
            "name": background.name(),
            "script": background.script(),
            "parts": describe_parts(background),
        })),
    }))
}

fn list_cards(runtime: &mut Runtime, _arguments: &Json) -> ToolResult<Json> {
    let cards: Vec<Json> = runtime
        .stack()
        .cards()
        .iter()
        .enumerate()
        .map(|(index, card)| {
            json!({
                "position": index + 1,
                "id": card.id().get(),
                "name": card.name(),
            })
        })
        .collect();
    Ok(json!({ "cards": cards }))
}

fn read_field(runtime: &mut Runtime, arguments: &Json) -> ToolResult<Json> {
    let field = locate_field(runtime, arguments)?;
    let text = runtime
        .object(field)?
        .property("text")
        .unwrap_or(Value::Empty)
        .as_text();
    Ok(json!({ "id": field.id.get(), "text": text }))
}

fn write_field(runtime: &mut Runtime, arguments: &Json) -> ToolResult<Json> {
    // Arguments are checked before anything is looked up, so a caller that
    // forgot one is told about the argument rather than about the field.
    let text = required_string(arguments, "text")?;
    let field = locate_field(runtime, arguments)?;
    runtime.execute(Command::SetProperty {
        object: field,
        property: "text".into(),
        value: Some(Value::text(text)),
    })?;
    Ok(json!({ "id": field.id.get(), "written": true }))
}

fn create_card(runtime: &mut Runtime, arguments: &Json) -> ToolResult<Json> {
    let after = runtime.current_card_index();
    let created = runtime
        .execute(Command::CreateCard {
            after,
            background: None,
        })?
        .ok_or_else(|| ToolError::Runtime("the card was not created".into()))?;
    if let Some(name) = optional_string(arguments, "name") {
        runtime.execute(Command::Rename {
            object: created,
            name,
        })?;
    }
    Ok(json!({ "id": created.id.get(), "position": after + 2 }))
}

fn create_part(runtime: &mut Runtime, arguments: &Json, kind: PartKind) -> ToolResult<Json> {
    let name = required_string(arguments, "name")?;
    let default = kind.default_size();
    let geometry = Rect::new(
        integer(arguments, "left").unwrap_or(20),
        integer(arguments, "top").unwrap_or(20),
        integer(arguments, "width").unwrap_or(default.width),
        integer(arguments, "height").unwrap_or(default.height),
    );
    let card = runtime.current_card();
    let created = runtime
        .execute(Command::CreatePart {
            owner: PartOwner::Card { id: card },
            kind,
            name,
            geometry,
        })?
        .ok_or_else(|| ToolError::Runtime("the part was not created".into()))?;

    if let Some(script) = optional_string(arguments, "script") {
        // Refusing a script that does not parse is friendlier than storing a
        // broken one and failing at the first click.
        Runtime::check_script(&script)?;
        runtime.execute(Command::SetScript {
            object: created,
            script,
        })?;
    }
    Ok(json!({ "id": created.id.get(), "kind": created.kind.as_str() }))
}

fn set_property(runtime: &mut Runtime, arguments: &Json) -> ToolResult<Json> {
    let object = object_argument(arguments)?;
    let property = required_string(arguments, "property")?;
    let value = arguments
        .get("value")
        .ok_or_else(|| ToolError::BadArguments("give a \"value\" to set".into()))?;
    let value = match value {
        Json::Bool(flag) => Value::Bool(*flag),
        Json::Number(number) => Value::Number(number.as_f64().unwrap_or_default()),
        Json::Null => Value::Empty,
        other => Value::text(
            other
                .as_str()
                .map_or_else(|| other.to_string(), str::to_string),
        ),
    };
    if property.eq_ignore_ascii_case("script") {
        Runtime::check_script(&value.as_text())?;
    }
    runtime.execute(Command::SetProperty {
        object,
        property,
        value: Some(value),
    })?;
    Ok(json!({ "set": true }))
}

fn run_script(runtime: &mut Runtime, arguments: &Json) -> ToolResult<Json> {
    let source = required_string(arguments, "script")?;
    let card = ObjectId::new(ObjectKind::Card, runtime.current_card());
    let value = runtime.run_script(&source, card)?;
    Ok(json!({
        "result": value.as_text(),
        "messageBox": runtime.message_box(),
        "effects": runtime.take_effects(),
    }))
}

fn send_message(runtime: &mut Runtime, arguments: &Json) -> ToolResult<Json> {
    let object = object_argument(arguments)?;
    let name = required_string(arguments, "message")?;
    let value = runtime.send_message(&Message::new(name), object)?;
    Ok(json!({
        "result": value.as_text(),
        "effects": runtime.take_effects(),
    }))
}

fn find_cards(runtime: &mut Runtime, arguments: &Json) -> ToolResult<Json> {
    let needle = required_string(arguments, "text")?.to_lowercase();
    let limit = integer(arguments, "limit").unwrap_or(20).max(1) as usize;
    if needle.is_empty() {
        return Err(ToolError::BadArguments("give some text to look for".into()));
    }

    let stack = runtime.stack();
    let mut matches = Vec::new();
    for (index, card) in stack.cards().iter().enumerate() {
        let mut where_found = Vec::new();
        if card.name().to_lowercase().contains(&needle) {
            where_found.push("name".to_string());
        }
        if card.script().to_lowercase().contains(&needle) {
            where_found.push("script".to_string());
        }
        for part in card.parts() {
            if part.text().to_lowercase().contains(&needle) {
                where_found.push(format!("{} \"{}\"", part.kind(), part.name()));
            } else if part.script().to_lowercase().contains(&needle) {
                where_found.push(format!("script of {} \"{}\"", part.kind(), part.name()));
            }
        }
        if !where_found.is_empty() {
            matches.push(json!({
                "position": index + 1,
                "id": card.id().get(),
                "name": card.name(),
                "found_in": where_found,
            }));
        }
        if matches.len() >= limit {
            break;
        }
    }
    Ok(json!({ "matches": matches }))
}

fn go_to_card(runtime: &mut Runtime, arguments: &Json) -> ToolResult<Json> {
    if let Some(id) = integer(arguments, "id") {
        runtime.go_to_card(Id::new(id.max(0) as u64))?;
    } else if let Some(position) = integer(arguments, "position") {
        let index = isize::try_from(position - 1)
            .map_err(|_| ToolError::BadArguments("that position is out of range".into()))?;
        runtime.go_to_index(index)?;
    } else {
        return Err(ToolError::BadArguments(
            "give either an \"id\" or a \"position\"".into(),
        ));
    }
    Ok(json!({
        "card": runtime.current_card().get(),
        "position": runtime.current_card_index() + 1,
    }))
}

// ------------------------------------------------------------------ helpers

fn describe_parts<T: PartContainer>(container: &T) -> Vec<Json> {
    container
        .parts()
        .iter()
        .map(|part| {
            let rect = part.geometry();
            json!({
                "kind": part.kind().as_str(),
                "id": part.id().get(),
                "name": part.name(),
                "rect": [rect.left, rect.top, rect.width, rect.height],
                "script": part.script(),
            })
        })
        .collect()
}

/// Finds the field a tool call names, on the card or its background.
fn locate_field(runtime: &Runtime, arguments: &Json) -> ToolResult<ObjectId> {
    let stack = runtime.stack();
    let card_id = runtime.current_card();

    if let Some(id) = integer(arguments, "id") {
        let id = Id::new(id.max(0) as u64);
        return match stack.part(id) {
            Some(part) if part.part_kind() == PartKind::Field => {
                Ok(ObjectId::new(ObjectKind::Field, id))
            }
            _ => Err(ToolError::BadArguments(format!(
                "there is no field with id {id}"
            ))),
        };
    }

    let name = optional_string(arguments, "name").ok_or_else(|| {
        ToolError::BadArguments("give either a field \"name\" or an \"id\"".into())
    })?;
    find_field_by_name(stack, card_id, &name).ok_or_else(|| {
        ToolError::BadArguments(format!(
            "there is no field named \"{name}\" on this card; \
             call current_card to see what is here"
        ))
    })
}

fn find_field_by_name(stack: &Stack, card: Id, name: &str) -> Option<ObjectId> {
    let on_card = stack
        .card(card)
        .and_then(|card| card.part_named(PartKind::Field, name));
    let found = on_card.or_else(|| {
        stack
            .background_of(card)
            .and_then(|background| background.part_named(PartKind::Field, name))
    })?;
    Some(ObjectId::new(ObjectKind::Field, found.id()))
}

fn object_argument(arguments: &Json) -> ToolResult<ObjectId> {
    let id = integer(arguments, "id")
        .ok_or_else(|| ToolError::BadArguments("give the object's \"id\"".into()))?;
    let kind = required_string(arguments, "kind")?;
    let kind = match kind.to_lowercase().as_str() {
        "stack" => ObjectKind::Stack,
        "background" => ObjectKind::Background,
        "card" => ObjectKind::Card,
        "button" => ObjectKind::Button,
        "field" => ObjectKind::Field,
        other => {
            return Err(ToolError::BadArguments(format!(
                "\"{other}\" is not a kind of object"
            )));
        }
    };
    Ok(ObjectId::new(kind, Id::new(id.max(0) as u64)))
}

fn required_string(arguments: &Json, name: &str) -> ToolResult<String> {
    optional_string(arguments, name)
        .ok_or_else(|| ToolError::BadArguments(format!("give a \"{name}\"")))
}

fn optional_string(arguments: &Json, name: &str) -> Option<String> {
    arguments.get(name)?.as_str().map(str::to_string)
}

fn integer(arguments: &Json, name: &str) -> Option<i32> {
    let value = arguments.get(name)?;
    value
        .as_i64()
        .or_else(|| value.as_str()?.parse().ok())
        .and_then(|number| i32::try_from(number).ok())
}
