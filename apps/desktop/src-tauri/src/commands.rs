//! The commands the window may ask for.
//!
//! This is the entire surface between the interface and the runtime. Every
//! function here does one of three things: take a snapshot, run a
//! [`Command`], or send a [`Message`]. There is no fourth kind, which is what
//! keeps the promise that the UI cannot touch stack data.

use hyperlab_persistence::{load, save};
use hyperlab_runtime::{Command, Effect, Message, PartOwner, Runtime, messages};
use hyperlab_stack::{
    Id, Object, ObjectId, ObjectKind, PartKind, Point, Rect, Size, Stack, Value,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    state::{AppState, Session},
    view::{PropertyView, StackView, properties_of, snapshot},
};

/// What every command gives back: the new state of the world, plus anything
/// scripts asked the world to do.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// The snapshot to draw.
    pub view: StackView,
    /// Dialogs, beeps and the like, in the order they happened.
    pub effects: Vec<Effect>,
}

/// The result of a command, with an error the window can show.
pub type CommandResult<T> = Result<T, String>;

/// Which layer a new part belongs to.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Layer {
    /// This card only.
    Card,
    /// Every card that shares this background.
    Background,
}

fn finish(session: &mut Session) -> Outcome {
    let effects = session.runtime.take_effects();
    let path = session.path_string();
    Outcome {
        view: snapshot(&session.runtime, session.dirty, path),
        effects,
    }
}

// ------------------------------------------------------------------ reading

/// Returns the current state without changing anything.
#[tauri::command]
pub fn get_view(state: State<'_, AppState>) -> Outcome {
    let mut session = state.session();
    finish(&mut session)
}

/// Returns every property of one object, for the inspector.
#[tauri::command]
pub fn get_properties(
    state: State<'_, AppState>,
    kind: String,
    id: u64,
) -> CommandResult<Vec<PropertyView>> {
    let session = state.session();
    let object = object_id(&kind, id)?;
    let object = session
        .runtime
        .stack()
        .object(object.kind, object.id)
        .ok_or_else(|| format!("there is no {kind} with id {id}"))?;
    Ok(properties_of(object))
}

/// Checks whether a script parses, for the editor's error line.
#[tauri::command]
pub fn check_script(source: String) -> CommandResult<()> {
    Runtime::check_script(&source).map_err(|error| error.to_string())
}

// ----------------------------------------------------------------- browsing

/// Sends `mouseUp` to a part, exactly as clicking it does.
#[tauri::command]
pub fn click_part(state: State<'_, AppState>, id: u64) -> CommandResult<Outcome> {
    let mut session = state.session();
    let part = find_part(&session.runtime, Id::new(id))?;
    session
        .runtime
        .send_message(&Message::new(messages::MOUSE_UP), part)
        .map_err(|error| error.to_string())?;
    Ok(finish(&mut session))
}

/// Types into a field, which is a change like any other.
#[tauri::command]
pub fn set_field_text(state: State<'_, AppState>, id: u64, text: String) -> CommandResult<Outcome> {
    let mut session = state.session();
    let field = ObjectId::new(ObjectKind::Field, Id::new(id));
    session
        .runtime
        .execute(Command::SetProperty {
            object: field,
            property: "text".into(),
            value: Some(Value::text(text)),
        })
        .map_err(|error| error.to_string())?;
    session.touch();
    let _ = session
        .runtime
        .send_message(&Message::new(messages::FIELD_CHANGED), field);
    Ok(finish(&mut session))
}

/// Goes to a card by position, counting from one, wrapping at the ends.
#[tauri::command]
pub fn go_to_card(state: State<'_, AppState>, position: i64) -> CommandResult<Outcome> {
    let mut session = state.session();
    let index = isize::try_from(position - 1).map_err(|_| "that card is out of range")?;
    session
        .runtime
        .go_to_index(index)
        .map_err(|error| error.to_string())?;
    Ok(finish(&mut session))
}

/// Runs whatever is in the message box.
#[tauri::command]
pub fn run_message_box(state: State<'_, AppState>, source: String) -> CommandResult<Outcome> {
    let mut session = state.session();
    let card = ObjectId::new(ObjectKind::Card, session.runtime.current_card());
    let result = session.runtime.run_script(&source, card);
    session.touch();
    match result {
        Ok(_) => Ok(finish(&mut session)),
        Err(error) => Err(error.to_string()),
    }
}

// ------------------------------------------------------------------ editing

/// Adds a card after the current one.
#[tauri::command]
pub fn new_card(state: State<'_, AppState>) -> CommandResult<Outcome> {
    let mut session = state.session();
    let after = session.runtime.current_card_index();
    let created = session
        .runtime
        .execute(Command::CreateCard {
            after,
            background: None,
        })
        .map_err(|error| error.to_string())?;
    session.touch();
    if let Some(card) = created {
        session
            .runtime
            .go_to_card(card.id)
            .map_err(|error| error.to_string())?;
    }
    Ok(finish(&mut session))
}

/// Deletes the current card.
#[tauri::command]
pub fn delete_card(state: State<'_, AppState>) -> CommandResult<Outcome> {
    let mut session = state.session();
    let card = session.runtime.current_card();
    session
        .runtime
        .execute(Command::DeleteCard { id: card })
        .map_err(|error| error.to_string())?;
    session.touch();
    Ok(finish(&mut session))
}

/// Adds a button or a field to the card or its background.
#[tauri::command]
pub fn new_part(
    state: State<'_, AppState>,
    kind: String,
    layer: Layer,
    name: Option<String>,
) -> CommandResult<Outcome> {
    let mut session = state.session();
    let part_kind = match kind.as_str() {
        "button" => PartKind::Button,
        "field" => PartKind::Field,
        other => return Err(format!("\"{other}\" is not a kind of part")),
    };

    let card = session.runtime.current_card();
    let owner = match layer {
        Layer::Card => PartOwner::Card { id: card },
        Layer::Background => PartOwner::Background {
            id: session
                .runtime
                .stack()
                .background_of(card)
                .map(Object::id)
                .ok_or("this card has no background")?,
        },
    };

    // The cascade counts *every* part on show, not just this kind, so a new
    // button and a new field do not land exactly on top of each other. The
    // name still counts its own kind, so they are "Button 1" and "Field 1".
    let placed = count_parts(session.runtime.stack(), card, None);
    let same_kind = count_parts(session.runtime.stack(), card, Some(part_kind));
    let geometry = session
        .runtime
        .stack()
        .default_part_geometry(part_kind, placed);
    let name = name.unwrap_or_else(|| format!("{} {}", title_case(&kind), same_kind + 1));

    session
        .runtime
        .execute(Command::CreatePart {
            owner,
            kind: part_kind,
            name,
            geometry,
        })
        .map_err(|error| error.to_string())?;
    session.touch();
    Ok(finish(&mut session))
}

/// Removes a part.
#[tauri::command]
pub fn delete_part(state: State<'_, AppState>, id: u64) -> CommandResult<Outcome> {
    let mut session = state.session();
    session
        .runtime
        .execute(Command::DeletePart { id: Id::new(id) })
        .map_err(|error| error.to_string())?;
    session.touch();
    Ok(finish(&mut session))
}

/// Moves or resizes a part, as dragging it does.
#[tauri::command]
pub fn set_geometry(
    state: State<'_, AppState>,
    id: u64,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
) -> CommandResult<Outcome> {
    let mut session = state.session();
    session
        .runtime
        .execute(Command::SetGeometry {
            id: Id::new(id),
            geometry: Rect::new(left, top, width, height),
        })
        .map_err(|error| error.to_string())?;
    session.touch();
    Ok(finish(&mut session))
}

/// Sets a property from the inspector.
#[tauri::command]
pub fn set_property(
    state: State<'_, AppState>,
    kind: String,
    id: u64,
    property: String,
    value: serde_json::Value,
) -> CommandResult<Outcome> {
    let mut session = state.session();
    let object = object_id(&kind, id)?;
    let value = match value {
        serde_json::Value::Bool(flag) => Value::Bool(flag),
        serde_json::Value::Number(number) => Value::Number(number.as_f64().unwrap_or_default()),
        serde_json::Value::Null => Value::Empty,
        serde_json::Value::String(text) => Value::text(text),
        other => Value::text(other.to_string()),
    };
    session
        .runtime
        .execute(Command::SetProperty {
            object,
            property,
            value: Some(value),
        })
        .map_err(|error| error.to_string())?;
    session.touch();
    Ok(finish(&mut session))
}

/// Replaces an object's script.
#[tauri::command]
pub fn set_script(
    state: State<'_, AppState>,
    kind: String,
    id: u64,
    script: String,
) -> CommandResult<Outcome> {
    // A script that does not parse is still worth keeping — half-written code
    // is normal — but the editor is told about it so it can say so.
    let mut session = state.session();
    let object = object_id(&kind, id)?;
    session
        .runtime
        .execute(Command::SetScript { object, script })
        .map_err(|error| error.to_string())?;
    session.touch();
    Ok(finish(&mut session))
}

/// Renames an object.
#[tauri::command]
pub fn rename(
    state: State<'_, AppState>,
    kind: String,
    id: u64,
    name: String,
) -> CommandResult<Outcome> {
    let mut session = state.session();
    let object = object_id(&kind, id)?;
    session
        .runtime
        .execute(Command::Rename { object, name })
        .map_err(|error| error.to_string())?;
    session.touch();
    Ok(finish(&mut session))
}

/// Resizes every card in the stack.
#[tauri::command]
pub fn set_stack_size(
    state: State<'_, AppState>,
    width: i32,
    height: i32,
) -> CommandResult<Outcome> {
    let mut session = state.session();
    session
        .runtime
        .execute(Command::SetStackSize {
            size: Size::new(width, height),
        })
        .map_err(|error| error.to_string())?;
    session.touch();
    Ok(finish(&mut session))
}

/// Undoes the last change.
#[tauri::command]
pub fn undo(state: State<'_, AppState>) -> CommandResult<Outcome> {
    let mut session = state.session();
    session.runtime.undo().map_err(|error| error.to_string())?;
    session.touch();
    Ok(finish(&mut session))
}

/// Redoes the last undone change.
#[tauri::command]
pub fn redo(state: State<'_, AppState>) -> CommandResult<Outcome> {
    let mut session = state.session();
    session.runtime.redo().map_err(|error| error.to_string())?;
    session.touch();
    Ok(finish(&mut session))
}

// ------------------------------------------------------------------- files

/// Starts a new, empty stack.
#[tauri::command]
pub fn new_stack(state: State<'_, AppState>, name: Option<String>) -> Outcome {
    let mut session = state.session();
    session
        .runtime
        .open(Stack::new(name.unwrap_or_else(|| "Untitled".into())));
    session.path = None;
    session.dirty = false;
    let _ = session.runtime.open_stack();
    finish(&mut session)
}

/// Opens a `.hl` bundle.
#[tauri::command]
pub fn open_stack(state: State<'_, AppState>, path: String) -> CommandResult<Outcome> {
    let stack = load(&path).map_err(|error| error.to_string())?;
    let mut session = state.session();
    session.runtime.open(stack);
    session.path = Some(path.into());
    session.dirty = false;
    // `openStack` runs on open, which is how a stack sets itself up.
    let _ = session.runtime.open_stack();
    Ok(finish(&mut session))
}

/// Saves the stack, to `path` if one is given and to where it came from
/// otherwise.
#[tauri::command]
pub fn save_stack(state: State<'_, AppState>, path: Option<String>) -> CommandResult<Outcome> {
    let mut session = state.session();
    let target = match path.map(std::path::PathBuf::from).or_else(|| session.path.clone()) {
        Some(target) => target,
        None => return Err("this stack has never been saved; choose where to put it".into()),
    };
    save(&target, session.runtime.stack()).map_err(|error| error.to_string())?;
    session.path = Some(target);
    session.dirty = false;
    Ok(finish(&mut session))
}

// ------------------------------------------------------------------ helpers

fn object_id(kind: &str, id: u64) -> CommandResult<ObjectId> {
    let kind = match kind {
        "stack" => ObjectKind::Stack,
        "background" => ObjectKind::Background,
        "card" => ObjectKind::Card,
        "button" => ObjectKind::Button,
        "field" => ObjectKind::Field,
        other => return Err(format!("\"{other}\" is not a kind of object")),
    };
    Ok(ObjectId::new(kind, Id::new(id)))
}

/// Works out whether an id belongs to a button or a field.
fn find_part(runtime: &Runtime, id: Id) -> CommandResult<ObjectId> {
    runtime
        .stack()
        .part(id)
        .map(|part| ObjectId::new(part.kind(), id))
        .ok_or_else(|| format!("there is no part with id {id}"))
}

/// How many parts the current card shows, including its background's.
///
/// With `kind`, only that kind is counted; without, everything is.
fn count_parts(stack: &Stack, card: Id, kind: Option<PartKind>) -> usize {
    use hyperlab_stack::PartContainer;
    let count = |container: &dyn PartContainer| match kind {
        Some(kind) => container.parts_of_kind(kind).len(),
        None => container.parts().len(),
    };
    let on_card = stack.card(card).map_or(0, |card| count(card));
    let on_background = stack
        .background_of(card)
        .map_or(0, |background| count(background));
    on_card + on_background
}

fn title_case(word: &str) -> String {
    let mut characters = word.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + characters.as_str()
    })
}

/// Which part is under a point, for hit testing done in Rust rather than in
/// the renderer.
#[tauri::command]
pub fn part_at(state: State<'_, AppState>, x: i32, y: i32) -> Option<u64> {
    use hyperlab_stack::PartContainer;
    let session = state.session();
    let stack = session.runtime.stack();
    let card = session.runtime.current_card();
    let point = Point::new(x, y);
    stack
        .card(card)
        .and_then(|card| card.part_at(point))
        .or_else(|| {
            stack
                .background_of(card)
                .and_then(|background| background.part_at(point))
        })
        .map(|part| part.id().get())
}
