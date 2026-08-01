//! The commands the window may ask for.
//!
//! This is the entire surface between the interface and the runtime. Every
//! function here does one of three things: take a snapshot, run a
//! [`Command`], or send a [`Message`]. There is no fourth kind, which is what
//! keeps the promise that the UI cannot touch stack data.
//!
//! # Why they are all `async`
//!
//! Tauri runs a synchronous command on the thread that pumps the window's
//! messages. A script that shows a dialog has to wait for an answer, and a
//! script that loops has to be interruptible by *something* — neither is
//! possible on the thread that draws the window.
//!
//! So each command hands its work to [`with_session`], which runs it on a
//! blocking thread. The window stays responsive while a script runs, which is
//! what lets [`DesktopHost`](crate::dialogs::DesktopHost) block until the
//! person answers.
//!
//! The single exception is [`dialog_reply`]: it is the message that unblocks
//! a waiting script, so it must never queue behind one.

use hyperlab_graph::Graph;
use hyperlab_persistence::{load, save};
use hyperlab_runtime::{Command, Effect, Message, PartOwner, Runtime, messages};
use hyperlab_stack::{Id, Object, ObjectId, ObjectKind, PartKind, Point, Rect, Size, Stack, Value};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    state::{AppState, Session, lock},
    view::{PropertyView, StackView, properties_of, snapshot},
};

/// What every command gives back: the new state of the world, plus anything
/// scripts asked the world to do.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// The snapshot to draw.
    pub view: StackView,
    /// Beeps, pauses and the like, in the order they happened. Dialogs are
    /// not here: they were shown while the script ran.
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

/// Runs `work` against the session, on a thread that may block.
async fn with_session<T, F>(state: &State<'_, AppState>, work: F) -> CommandResult<T>
where
    F: FnOnce(&mut Session) -> CommandResult<T> + Send + 'static,
    T: Send + 'static,
{
    let session = state.session();
    tauri::async_runtime::spawn_blocking(move || work(&mut lock(&session)))
        .await
        .map_err(|_| "the runtime stopped unexpectedly".to_string())?
}

/// Takes a snapshot after something outside this module changed the stack.
///
/// The AI sidebar locks and unlocks the session several times in one turn,
/// so it cannot use [`with_session`]; it still has to leave the window
/// looking at the truth.
pub fn snapshot_outcome(session: &mut Session) -> Outcome {
    finish(session)
}

/// Takes a snapshot, and collects whatever scripts left behind.
fn finish(session: &mut Session) -> Outcome {
    let effects = session.runtime.take_effects();
    let path = session.path_string();
    Outcome {
        view: snapshot(&session.runtime, session.dirty, path),
        effects,
    }
}

// ------------------------------------------------------------------ dialogs

/// Answers the dialog a script is waiting on. `None` means cancelled.
///
/// Returns whether anything was waiting, so a window that dismissed the same
/// dialog twice can tell.
#[tauri::command]
pub fn dialog_reply(state: State<'_, AppState>, text: Option<String>) -> bool {
    state.dialogs().reply(text)
}

// ------------------------------------------------------------------ reading

/// Returns the current state without changing anything.
#[tauri::command]
pub async fn get_view(state: State<'_, AppState>) -> CommandResult<Outcome> {
    with_session(&state, |session| Ok(finish(session))).await
}

/// Returns every property of one object, for the inspector.
#[tauri::command]
pub async fn get_properties(
    state: State<'_, AppState>,
    kind: String,
    id: u64,
) -> CommandResult<Vec<PropertyView>> {
    with_session(&state, move |session| {
        let object = object_id(&kind, id)?;
        let object = session
            .runtime
            .stack()
            .object(object.kind, object.id)
            .ok_or_else(|| format!("there is no {kind} with id {id}"))?;
        Ok(properties_of(object))
    })
    .await
}

/// Checks whether a script parses, for the editor's error line.
#[tauri::command]
pub fn check_script(source: String) -> CommandResult<()> {
    Runtime::check_script(&source).map_err(|error| error.to_string())
}

/// Reads the stack as the routes between its cards, for the map.
///
/// A pure function of the stack, computed fresh each time it is asked for.
/// Caching it would mean deciding when the cache is stale, and every script
/// that runs could make it so.
#[tauri::command]
pub async fn stack_graph(state: State<'_, AppState>) -> CommandResult<Graph> {
    with_session(&state, |session| Ok(Graph::of(session.runtime.stack()))).await
}

/// One of the stack's pictures, as a `data:` URI the renderer can draw.
///
/// Asked for by name and cached in the window, rather than sent with every
/// snapshot: a snapshot is taken after every command, and a card of artwork
/// would be re-encoded on every keystroke.
#[tauri::command]
pub async fn stack_image(state: State<'_, AppState>, name: String) -> CommandResult<String> {
    with_session(&state, move |session| {
        session
            .runtime
            .stack()
            .image(&name)
            .map(hyperlab_stack::data_uri)
            .ok_or_else(|| format!("this stack has no picture called \"{name}\""))
    })
    .await
}

/// The names of every picture the stack carries.
#[tauri::command]
pub async fn stack_images(state: State<'_, AppState>) -> CommandResult<Vec<String>> {
    with_session(&state, |session| {
        Ok(session.runtime.stack().images().keys().cloned().collect())
    })
    .await
}

// ----------------------------------------------------------------- browsing

/// Sends `mouseUp` to a part, exactly as clicking it does.
#[tauri::command]
pub async fn click_part(state: State<'_, AppState>, id: u64) -> CommandResult<Outcome> {
    with_session(&state, move |session| {
        let part = find_part(&session.runtime, Id::new(id))?;
        session
            .runtime
            .send_message(&Message::new(messages::MOUSE_UP), part)
            .map_err(|error| error.to_string())?;
        Ok(finish(session))
    })
    .await
}

/// Types into a field, which is a change like any other.
#[tauri::command]
pub async fn set_field_text(
    state: State<'_, AppState>,
    id: u64,
    text: String,
) -> CommandResult<Outcome> {
    with_session(&state, move |session| {
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
        Ok(finish(session))
    })
    .await
}

/// Goes to a card by position, counting from one, wrapping at the ends.
#[tauri::command]
pub async fn go_to_card(state: State<'_, AppState>, position: i64) -> CommandResult<Outcome> {
    with_session(&state, move |session| {
        let index = isize::try_from(position - 1).map_err(|_| "that card is out of range")?;
        session
            .runtime
            .go_to_index(index)
            .map_err(|error| error.to_string())?;
        Ok(finish(session))
    })
    .await
}

/// Runs whatever is in the message box.
#[tauri::command]
pub async fn run_message_box(state: State<'_, AppState>, source: String) -> CommandResult<Outcome> {
    with_session(&state, move |session| {
        let card = ObjectId::new(ObjectKind::Card, session.runtime.current_card());
        session
            .runtime
            .run_script(&source, card)
            .map_err(|error| error.to_string())?;
        session.touch();
        Ok(finish(session))
    })
    .await
}

// ------------------------------------------------------------------ editing

/// Adds a card after the current one.
#[tauri::command]
pub async fn new_card(state: State<'_, AppState>) -> CommandResult<Outcome> {
    with_session(&state, |session| {
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
        Ok(finish(session))
    })
    .await
}

/// Deletes the current card.
#[tauri::command]
pub async fn delete_card(state: State<'_, AppState>) -> CommandResult<Outcome> {
    with_session(&state, |session| {
        let card = session.runtime.current_card();
        session
            .runtime
            .execute(Command::DeleteCard { id: card })
            .map_err(|error| error.to_string())?;
        session.touch();
        Ok(finish(session))
    })
    .await
}

/// Adds a part to the card or its background.
#[tauri::command]
pub async fn new_part(
    state: State<'_, AppState>,
    kind: String,
    layer: Layer,
    name: Option<String>,
) -> CommandResult<Outcome> {
    with_session(&state, move |session| {
        let part_kind = match kind.as_str() {
            "button" => PartKind::Button,
            "field" => PartKind::Field,
            "image" => PartKind::Image,
            other => return Err(format!("\"{other}\" is not a kind of part")),
        };

        let card = session.runtime.current_card();
        let owner = owner_for(session, card, layer)?;

        // The cascade counts *every* part on show, not just this kind, so a
        // new button and a new field do not land exactly on top of each
        // other. The name still counts its own kind, so they are "Button 1"
        // and "Field 1".
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
        Ok(finish(session))
    })
    .await
}

/// Which container a new part on `card` belongs to.
fn owner_for(session: &Session, card: Id, layer: Layer) -> CommandResult<PartOwner> {
    Ok(match layer {
        Layer::Card => PartOwner::Card { id: card },
        Layer::Background => PartOwner::Background {
            id: session
                .runtime
                .stack()
                .background_of(card)
                .map(Object::id)
                .ok_or("this card has no background")?,
        },
    })
}

/// Brings a picture into the stack and puts an image part on the card.
///
/// The file is read here rather than in the window, so the bytes go straight
/// from disk into the model and the renderer only ever sees a picture the
/// model has already checked.
#[tauri::command]
pub async fn import_image(
    state: State<'_, AppState>,
    path: String,
    layer: Layer,
) -> CommandResult<Outcome> {
    with_session(&state, move |session| {
        let path = std::path::PathBuf::from(path);
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("that file has no name I can use")?
            .to_string();

        // Read no more than a picture may be, so pointing this at a disk
        // image does not try to hold one in memory before refusing it.
        let bytes = read_at_most(&path, hyperlab_stack::MAX_IMAGE_BYTES)?;
        let image = hyperlab_stack::Image::new(&name, bytes).map_err(|error| error.to_string())?;

        let card = session.runtime.current_card();
        let owner = owner_for(session, card, layer)?;
        let placed = count_parts(session.runtime.stack(), card, None);
        let geometry = session
            .runtime
            .stack()
            .default_part_geometry(PartKind::Image, placed);

        session
            .runtime
            .execute(Command::SetImage {
                name: name.clone(),
                image: Some(Box::new(image)),
            })
            .map_err(|error| error.to_string())?;
        let created = session
            .runtime
            .execute(Command::CreatePart {
                owner,
                kind: PartKind::Image,
                name: name.clone(),
                geometry,
            })
            .map_err(|error| error.to_string())?;

        if let Some(part) = created {
            session
                .runtime
                .execute(Command::SetProperty {
                    object: part,
                    property: "source".into(),
                    value: Some(Value::text(name)),
                })
                .map_err(|error| error.to_string())?;
        }
        session.touch();
        Ok(finish(session))
    })
    .await
}

/// Reads a file, refusing one that is bigger than `most`.
///
/// The length is checked before the read rather than after, so a file that
/// is far too big is never held in memory at all.
fn read_at_most(path: &std::path::Path, most: usize) -> CommandResult<Vec<u8>> {
    let length = std::fs::metadata(path)
        .map_err(|error| format!("could not open {}: {error}", path.display()))?
        .len();
    if length > most as u64 {
        return Err(format!(
            "that file is {} MB, and the most a stack will hold is {} MB",
            length / (1024 * 1024),
            most / (1024 * 1024)
        ));
    }
    std::fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))
}

/// Removes a part.
#[tauri::command]
pub async fn delete_part(state: State<'_, AppState>, id: u64) -> CommandResult<Outcome> {
    with_session(&state, move |session| {
        session
            .runtime
            .execute(Command::DeletePart { id: Id::new(id) })
            .map_err(|error| error.to_string())?;
        session.touch();
        Ok(finish(session))
    })
    .await
}

/// Moves or resizes a part, as dragging it does.
#[tauri::command]
pub async fn set_geometry(
    state: State<'_, AppState>,
    id: u64,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
) -> CommandResult<Outcome> {
    with_session(&state, move |session| {
        session
            .runtime
            .execute(Command::SetGeometry {
                id: Id::new(id),
                geometry: Rect::new(left, top, width, height),
            })
            .map_err(|error| error.to_string())?;
        session.touch();
        Ok(finish(session))
    })
    .await
}

/// Sets a property from the inspector.
#[tauri::command]
pub async fn set_property(
    state: State<'_, AppState>,
    kind: String,
    id: u64,
    property: String,
    value: serde_json::Value,
) -> CommandResult<Outcome> {
    with_session(&state, move |session| {
        let object = object_id(&kind, id)?;
        session
            .runtime
            .execute(Command::SetProperty {
                object,
                property,
                value: Some(json_to_value(value)),
            })
            .map_err(|error| error.to_string())?;
        session.touch();
        Ok(finish(session))
    })
    .await
}

/// Replaces an object's script.
///
/// A script that does not parse is still stored — half-written code is
/// normal, and losing it would be worse than keeping it. The editor checks
/// separately, with [`check_script`], and says so.
#[tauri::command]
pub async fn set_script(
    state: State<'_, AppState>,
    kind: String,
    id: u64,
    script: String,
) -> CommandResult<Outcome> {
    with_session(&state, move |session| {
        let object = object_id(&kind, id)?;
        session
            .runtime
            .execute(Command::SetScript { object, script })
            .map_err(|error| error.to_string())?;
        session.touch();
        Ok(finish(session))
    })
    .await
}

/// Renames an object.
#[tauri::command]
pub async fn rename(
    state: State<'_, AppState>,
    kind: String,
    id: u64,
    name: String,
) -> CommandResult<Outcome> {
    with_session(&state, move |session| {
        let object = object_id(&kind, id)?;
        session
            .runtime
            .execute(Command::Rename { object, name })
            .map_err(|error| error.to_string())?;
        session.touch();
        Ok(finish(session))
    })
    .await
}

/// Resizes every card in the stack.
#[tauri::command]
pub async fn set_stack_size(
    state: State<'_, AppState>,
    width: i32,
    height: i32,
) -> CommandResult<Outcome> {
    with_session(&state, move |session| {
        session
            .runtime
            .execute(Command::SetStackSize {
                size: Size::new(width, height),
            })
            .map_err(|error| error.to_string())?;
        session.touch();
        Ok(finish(session))
    })
    .await
}

/// Undoes the last change.
#[tauri::command]
pub async fn undo(state: State<'_, AppState>) -> CommandResult<Outcome> {
    with_session(&state, |session| {
        session.runtime.undo().map_err(|error| error.to_string())?;
        session.touch();
        Ok(finish(session))
    })
    .await
}

/// Redoes the last undone change.
#[tauri::command]
pub async fn redo(state: State<'_, AppState>) -> CommandResult<Outcome> {
    with_session(&state, |session| {
        session.runtime.redo().map_err(|error| error.to_string())?;
        session.touch();
        Ok(finish(session))
    })
    .await
}

// ------------------------------------------------------------------- files

/// Starts a new, empty stack.
#[tauri::command]
pub async fn new_stack(state: State<'_, AppState>, name: Option<String>) -> CommandResult<Outcome> {
    with_session(&state, move |session| {
        session
            .runtime
            .open(Stack::new(name.unwrap_or_else(|| "Untitled".into())));
        session.path = None;
        session.dirty = false;
        let _ = session.runtime.open_stack();
        Ok(finish(session))
    })
    .await
}

/// Opens a `.hl` bundle.
#[tauri::command]
pub async fn open_stack(state: State<'_, AppState>, path: String) -> CommandResult<Outcome> {
    with_session(&state, move |session| {
        let stack = load(&path).map_err(|error| error.to_string())?;
        session.runtime.open(stack);
        session.path = Some(path.into());
        session.dirty = false;
        // `openStack` runs on open, which is how a stack sets itself up.
        let _ = session.runtime.open_stack();
        Ok(finish(session))
    })
    .await
}

/// Saves the stack, to `path` if one is given and to where it came from
/// otherwise.
#[tauri::command]
pub async fn save_stack(
    state: State<'_, AppState>,
    path: Option<String>,
) -> CommandResult<Outcome> {
    with_session(&state, move |session| {
        let target = path
            .map(std::path::PathBuf::from)
            .or_else(|| session.path.clone())
            .ok_or("this stack has never been saved; choose where to put it")?;
        save(&target, session.runtime.stack()).map_err(|error| error.to_string())?;
        session.path = Some(target);
        session.dirty = false;
        Ok(finish(session))
    })
    .await
}

/// Which part is under a point, for hit testing done in Rust rather than in
/// the renderer.
#[tauri::command]
pub async fn part_at(state: State<'_, AppState>, x: i32, y: i32) -> CommandResult<Option<u64>> {
    use hyperlab_stack::PartContainer;
    with_session(&state, move |session| {
        let stack = session.runtime.stack();
        let card = session.runtime.current_card();
        let point = Point::new(x, y);
        Ok(stack
            .card(card)
            .and_then(|card| card.part_at(point))
            .or_else(|| {
                stack
                    .background_of(card)
                    .and_then(|background| background.part_at(point))
            })
            .map(|part| part.id().get()))
    })
    .await
}

// ------------------------------------------------------------------ helpers

fn object_id(kind: &str, id: u64) -> CommandResult<ObjectId> {
    let kind = match kind {
        "stack" => ObjectKind::Stack,
        "background" => ObjectKind::Background,
        "card" => ObjectKind::Card,
        "button" => ObjectKind::Button,
        "field" => ObjectKind::Field,
        "image" => ObjectKind::Image,
        other => return Err(format!("\"{other}\" is not a kind of object")),
    };
    Ok(ObjectId::new(kind, Id::new(id)))
}

/// Turns what the inspector sent into a property value, keeping booleans and
/// numbers as themselves rather than as their spelling.
fn json_to_value(value: serde_json::Value) -> Value {
    match value {
        serde_json::Value::Bool(flag) => Value::Bool(flag),
        serde_json::Value::Number(number) => Value::Number(number.as_f64().unwrap_or_default()),
        serde_json::Value::Null => Value::Empty,
        serde_json::Value::String(text) => Value::text(text),
        other => Value::text(other.to_string()),
    }
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
