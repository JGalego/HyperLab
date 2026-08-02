//! The commands the page may ask for.
//!
//! A twin of the desktop shell's `commands.rs` and `ai_commands.rs`, one
//! exported function per command, so the same renderer works behind either.
//! Every function does one of three things: take a snapshot, run a
//! [`Command`], or send a [`Message`]. There is no fourth kind, which is
//! what keeps the promise that the UI cannot touch stack data.
//!
//! # The boundary
//!
//! Arguments arrive as one JSON string and answers leave as one, because a
//! page already speaks JSON and a single shape keeps the JavaScript side to
//! a one-line dispatcher. The exceptions are bytes — a picture crosses as
//! the bytes it is — and [`init`], which is handed the host object itself.
//!
//! # Who blocks where
//!
//! WebAssembly in a browser has one thread, so the desktop's arrangement —
//! block the script's thread while the window answers — is recreated one
//! level up: this module runs in a Web Worker, and the [`JsHost`] the page
//! provides blocks that worker (`Atomics.wait`) while the page, still
//! responsive, shows the dialog. Nothing in this crate knows that; it calls
//! [`Host::ask`] and waits for its answer, exactly as the desktop runtime
//! does.
//!
//! # State
//!
//! The document and the AI conversation live in two separate cells, exactly
//! as the desktop keeps two locks: a script that calls `ai("…")` runs while
//! the document is borrowed, and must still be able to reach the provider
//! configuration.

use std::cell::RefCell;

use hyperlab_ai::{AiSettings, CompletionRequest, ContextOptions};
use hyperlab_assistant::{Briefing, Conversation, SYSTEM_PROMPT, tools};
use hyperlab_mcp::{Access, Approval, Approver, Policy, ToolRegistry};
use hyperlab_runtime::{AiRequest, Command, Effect, Host, Message, PartOwner, Runtime, messages};
use hyperlab_stack::{Id, Object, ObjectId, ObjectKind, PartKind, Point, Rect, Size, Stack, Value};
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::providers::{self, WebProvider, Wire};
use crate::view::{StackView, properties_of, snapshot};

// ------------------------------------------------------------------- state

/// One open document, plus whether it has changed since it was opened.
struct Doc {
    runtime: Runtime,
    dirty: bool,
}

/// Everything the AI sidebar needs between turns.
struct Ai {
    conversation: Conversation,
    settings: AiSettings,
    providers: Vec<WebProvider>,
    /// Why a provider named in the settings is not available.
    problems: Vec<String>,
    policy: Policy,
    context: ContextOptions,
    /// Whether a turn is already running. A second one would interleave
    /// tool calls with the first and corrupt both transcripts.
    busy: bool,
}

thread_local! {
    static DOC: RefCell<Option<Doc>> = const { RefCell::new(None) };
    static AI: RefCell<Option<Ai>> = const { RefCell::new(None) };
    static HOST: RefCell<Option<JsValue>> = const { RefCell::new(None) };
}

/// Where the provider settings live in browser storage.
const SETTINGS_KEY: &str = "hyperlab.ai.settings";

/// Where a provider's key lives in browser storage. One entry per provider,
/// written by [`ai_set_key`] and read only when a request is built — there
/// is no command that returns one.
fn key_slot(provider: &str) -> String {
    format!("hyperlab.ai.key.{provider}")
}

// ---------------------------------------------------------------- the host

#[wasm_bindgen]
extern "C" {
    /// The page's side of the runtime's questions: dialogs, sounds, storage,
    /// and the one HTTP transport a browser has. See the worker in
    /// `apps/web` for the implementation this signature is a contract with.
    pub type JsHost;

    /// `answer "…"`: shows a message and waits for it to be dismissed.
    #[wasm_bindgen(method, structural)]
    fn answer(this: &JsHost, message: &str);

    /// `ask "…"`: shows a question and waits. `null` means cancelled.
    #[wasm_bindgen(method, structural)]
    fn ask(this: &JsHost, prompt: &str, default: &str) -> Option<String>;

    /// `beep`.
    #[wasm_bindgen(method, structural)]
    fn beep(this: &JsHost);

    /// Posts JSON and waits for the reply, for a script blocked on
    /// `ai("…")`. Returns `{"status": …, "body": …}` as JSON; throws when
    /// the request never got an answer.
    #[wasm_bindgen(method, structural, catch, js_name = completeSync)]
    fn complete_sync(
        this: &JsHost,
        url: &str,
        headers: &str,
        body: &str,
    ) -> Result<String, JsValue>;

    /// The same request without the waiting, for the sidebar: the returned
    /// promise resolves to the same JSON shape.
    #[wasm_bindgen(method, structural, js_name = complete)]
    fn complete(this: &JsHost, url: &str, headers: &str, body: &str) -> js_sys::Promise;

    /// Reads one value from browser storage.
    #[wasm_bindgen(method, structural, js_name = storageGet)]
    fn storage_get(this: &JsHost, key: &str) -> Option<String>;

    /// Writes one value to browser storage; `None` removes it.
    #[wasm_bindgen(method, structural, js_name = storageSet)]
    fn storage_set(this: &JsHost, key: &str, value: Option<String>);
}

/// Runs `work` with the host the page handed to [`init`].
fn with_host<R>(work: impl FnOnce(&JsHost) -> R) -> R {
    HOST.with(|cell| {
        let held = cell.borrow();
        let value = held.as_ref().expect("init() has not been called");
        work(value.unchecked_ref::<JsHost>())
    })
}

/// A [`Host`] that puts the runtime's questions to the page.
///
/// It holds nothing itself — the JavaScript object lives in a cell of its
/// own — so it is `Send` the way the trait asks, and the single-threaded
/// truth of the matter is the cell's business.
struct WebHost;

impl Host for WebHost {
    fn answer(&mut self, message: &str) {
        with_host(|host| host.answer(message));
    }

    fn ask(&mut self, prompt: &str, default: &str) -> Option<String> {
        with_host(|host| host.ask(prompt, default))
    }

    fn beep(&mut self) {
        with_host(|host| host.beep());
    }

    /// What `ai("…")` and `ask assistant` reach: one question, no tools, no
    /// briefing — the script says what to send. The same deliberate limits
    /// as the desktop, for the same reasons: the interpreter is mid-handler,
    /// and an assistant restructuring the stack under it would pull the
    /// ground away.
    fn ai(&mut self, request: &AiRequest) -> Result<String, String> {
        let completion_request = |model: &str| {
            CompletionRequest::new(
                model,
                vec![
                    hyperlab_ai::ChatMessage::system(SYSTEM_PROMPT),
                    hyperlab_ai::ChatMessage::user(request.prompt.clone()),
                ],
            )
        };

        AI.with(|cell| {
            let held = cell.borrow();
            let ai = held.as_ref().ok_or("no assistant is set up")?;
            let provider = ai.chosen_provider()?;
            let request = completion_request(&provider.model);

            if provider.wire == Wire::Mock {
                return Ok(providers::mock_completion(&request).content);
            }

            let body = provider.completion_body(&request).to_string();
            let headers = headers_json(provider);
            let reply = with_host(|host| {
                host.complete_sync(&provider.completion_url(), &headers, &body)
            })
            .map_err(|thrown| format!("could not reach the provider: {}", text_of(&thrown)))?;
            let (status, reply_body) = split_reply(&reply)?;
            provider
                .decode_completion(status, &reply_body)
                .map(|completion| completion.content)
        })
    }
}

/// The person at the keyboard, who is already driving the sidebar. The
/// panel's switch is what sets the policy; asking again per tool would be a
/// dialog every few seconds saying what the switch already said.
struct SidebarUser;

impl Approver for SidebarUser {
    fn approve(&mut self, _request: &Approval<'_>) -> bool {
        true
    }
}

// ------------------------------------------------------------- housekeeping

/// The result of a command: JSON out, or a sentence the page can show.
type ApiResult = Result<String, JsValue>;

/// Turns anything printable into the error arm.
fn fail(message: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&message.to_string())
}

/// Serializes a command's answer.
fn reply<T: Serialize>(value: &T) -> ApiResult {
    serde_json::to_string(value).map_err(fail)
}

/// Runs `work` against the open document.
fn with_doc<R>(work: impl FnOnce(&mut Doc) -> Result<R, String>) -> Result<R, JsValue> {
    DOC.with(|cell| {
        let mut held = cell.borrow_mut();
        let doc = held.as_mut().ok_or_else(|| fail("init() has not run"))?;
        work(doc).map_err(fail)
    })
}

/// Runs `work` against the AI state.
fn with_ai<R>(work: impl FnOnce(&mut Ai) -> Result<R, String>) -> Result<R, JsValue> {
    AI.with(|cell| {
        let mut held = cell.borrow_mut();
        let ai = held.as_mut().ok_or_else(|| fail("init() has not run"))?;
        work(ai).map_err(fail)
    })
}

/// What every command gives back: the new state of the world, plus anything
/// scripts asked the world to do.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Outcome {
    view: StackView,
    effects: Vec<Effect>,
}

/// Takes a snapshot, and collects whatever scripts left behind.
fn finish(doc: &mut Doc) -> Outcome {
    let effects = doc.runtime.take_effects();
    Outcome {
        view: snapshot(&doc.runtime, doc.dirty, None),
        effects,
    }
}

fn outcome(doc: &mut Doc) -> Result<String, String> {
    serde_json::to_string(&finish(doc)).map_err(|error| error.to_string())
}

fn parse_args<'a, T: Deserialize<'a>>(json: &'a str) -> Result<T, JsValue> {
    serde_json::from_str(json).map_err(|error| fail(format!("bad arguments: {error}")))
}

// -------------------------------------------------------------------- init

/// Wakes the module up: keeps the host, reads the settings, and opens an
/// untitled stack so the window always has something in it. HyperCard
/// opened on a stack; so does HyperLab, on either shell.
#[wasm_bindgen]
pub fn init(host: JsValue) {
    // A panic in here would otherwise reach the page as "unreachable", which
    // helps nobody; this turns it into the message and a stack trace.
    console_error_panic_hook::set_once();
    // The platform has no clock, so objects are stamped by the browser's.
    hyperlab_stack::set_clock(|| js_sys::Date::now() as u64);
    HOST.with(|cell| *cell.borrow_mut() = Some(host));

    let (settings, mut problems) = match with_host(|h| h.storage_get(SETTINGS_KEY)) {
        None => (AiSettings::default(), Vec::new()),
        Some(text) => match serde_json::from_str(&text) {
            Ok(settings) => (settings, Vec::new()),
            // Reported rather than replaced: silently starting again would
            // throw away configuration the user made by hand.
            Err(error) => (
                AiSettings::default(),
                vec![format!("the saved AI settings could not be read: {error}")],
            ),
        },
    };
    let (resolved, mut resolution_problems) = resolve_providers(&settings);
    problems.append(&mut resolution_problems);

    AI.with(|cell| {
        *cell.borrow_mut() = Some(Ai {
            conversation: Conversation::new(),
            settings,
            providers: resolved,
            problems,
            // The user is sitting in front of the sidebar driving it, so
            // there is nobody to ask that they have not already told.
            policy: Policy::trusted(),
            context: ContextOptions::default(),
            busy: false,
        });
    });

    let mut runtime = Runtime::new(Stack::new("Untitled"));
    runtime.set_host(Box::new(WebHost));
    let _ = runtime.open_stack();
    DOC.with(|cell| {
        *cell.borrow_mut() = Some(Doc {
            runtime,
            dirty: false,
        });
    });
}

/// Resolves the providers the settings describe, going to browser storage
/// for keys.
fn resolve_providers(settings: &AiSettings) -> (Vec<WebProvider>, Vec<String>) {
    providers::resolve_all(settings, &|name| {
        with_host(|host| host.storage_get(&key_slot(name)))
    })
}

// ------------------------------------------------------------------ reading

/// Returns the current state without changing anything.
#[wasm_bindgen]
pub fn get_view(_args: &str) -> ApiResult {
    with_doc(outcome)
}

#[derive(Deserialize)]
struct ObjectArgs {
    kind: String,
    id: u64,
}

/// Returns every property of one object, for the inspector.
#[wasm_bindgen]
pub fn get_properties(args: &str) -> ApiResult {
    let args: ObjectArgs = parse_args(args)?;
    let object = object_id(&args.kind, args.id).map_err(fail)?;
    with_doc(|doc| {
        let object = doc
            .runtime
            .stack()
            .object(object.kind, object.id)
            .ok_or_else(|| format!("there is no {} with id {}", args.kind, args.id))?;
        serde_json::to_string(&properties_of(object)).map_err(|error| error.to_string())
    })
}

#[derive(Deserialize)]
struct SourceArgs {
    source: String,
}

/// Checks whether a script parses, for the editor's error line.
#[wasm_bindgen]
pub fn check_script(args: &str) -> ApiResult {
    let args: SourceArgs = parse_args(args)?;
    Runtime::check_script(&args.source).map_err(fail)?;
    reply(&())
}

/// Reads the stack as the routes between its cards, for the map.
#[wasm_bindgen]
pub fn stack_graph(_args: &str) -> ApiResult {
    with_doc(|doc| {
        serde_json::to_string(&hyperlab_graph::Graph::of(doc.runtime.stack()))
            .map_err(|error| error.to_string())
    })
}

#[derive(Deserialize)]
struct NameArgs {
    name: String,
}

/// One of the stack's pictures, as a `data:` URI the renderer can draw.
#[wasm_bindgen]
pub fn stack_image(args: &str) -> ApiResult {
    let args: NameArgs = parse_args(args)?;
    with_doc(|doc| {
        doc.runtime
            .stack()
            .image(&args.name)
            .map(hyperlab_stack::data_uri)
            .ok_or_else(|| format!("this stack has no picture called \"{}\"", args.name))
            .and_then(|uri| serde_json::to_string(&uri).map_err(|error| error.to_string()))
    })
}

/// The names of every picture the stack carries.
#[wasm_bindgen]
pub fn stack_images(_args: &str) -> ApiResult {
    with_doc(|doc| {
        let names: Vec<String> = doc.runtime.stack().images().keys().cloned().collect();
        serde_json::to_string(&names).map_err(|error| error.to_string())
    })
}

#[derive(Deserialize)]
struct PointArgs {
    x: i32,
    y: i32,
}

/// Which part is under a point, for hit testing done here rather than in
/// the renderer.
#[wasm_bindgen]
pub fn part_at(args: &str) -> ApiResult {
    use hyperlab_stack::PartContainer;
    let args: PointArgs = parse_args(args)?;
    with_doc(|doc| {
        let stack = doc.runtime.stack();
        let card = doc.runtime.current_card();
        let point = Point::new(args.x, args.y);
        let found = stack
            .card(card)
            .and_then(|card| card.part_at(point))
            .or_else(|| {
                stack
                    .background_of(card)
                    .and_then(|background| background.part_at(point))
            })
            .map(|part| part.id().get());
        serde_json::to_string(&found).map_err(|error| error.to_string())
    })
}

// ----------------------------------------------------------------- browsing

#[derive(Deserialize)]
struct IdArgs {
    id: u64,
}

/// Sends `mouseUp` to a part, exactly as clicking it does.
#[wasm_bindgen]
pub fn click_part(args: &str) -> ApiResult {
    let args: IdArgs = parse_args(args)?;
    with_doc(|doc| {
        let part = find_part(&doc.runtime, Id::new(args.id))?;
        doc.runtime
            .send_message(&Message::new(messages::MOUSE_UP), part)
            .map_err(|error| error.to_string())?;
        outcome(doc)
    })
}

#[derive(Deserialize)]
struct FieldTextArgs {
    id: u64,
    text: String,
}

/// Types into a field, which is a change like any other.
#[wasm_bindgen]
pub fn set_field_text(args: &str) -> ApiResult {
    let args: FieldTextArgs = parse_args(args)?;
    with_doc(|doc| {
        let field = ObjectId::new(ObjectKind::Field, Id::new(args.id));
        doc.runtime
            .execute(Command::SetProperty {
                object: field,
                property: "text".into(),
                value: Some(Value::text(args.text)),
            })
            .map_err(|error| error.to_string())?;
        doc.dirty = true;
        let _ = doc
            .runtime
            .send_message(&Message::new(messages::FIELD_CHANGED), field);
        outcome(doc)
    })
}

#[derive(Deserialize)]
struct PositionArgs {
    position: i64,
}

/// Goes to a card by position, counting from one, wrapping at the ends.
#[wasm_bindgen]
pub fn go_to_card(args: &str) -> ApiResult {
    let args: PositionArgs = parse_args(args)?;
    with_doc(|doc| {
        let index = isize::try_from(args.position - 1).map_err(|_| "that card is out of range")?;
        doc.runtime
            .go_to_index(index)
            .map_err(|error| error.to_string())?;
        outcome(doc)
    })
}

/// Runs whatever is in the message box.
#[wasm_bindgen]
pub fn run_message_box(args: &str) -> ApiResult {
    let args: SourceArgs = parse_args(args)?;
    with_doc(|doc| {
        let card = ObjectId::new(ObjectKind::Card, doc.runtime.current_card());
        doc.runtime
            .run_script(&args.source, card)
            .map_err(|error| error.to_string())?;
        doc.dirty = true;
        outcome(doc)
    })
}

// ------------------------------------------------------------------ editing

/// Adds a card after the current one.
#[wasm_bindgen]
pub fn new_card(_args: &str) -> ApiResult {
    with_doc(|doc| {
        let after = doc.runtime.current_card_index();
        let created = doc
            .runtime
            .execute(Command::CreateCard {
                after,
                background: None,
            })
            .map_err(|error| error.to_string())?;
        doc.dirty = true;
        if let Some(card) = created {
            doc.runtime
                .go_to_card(card.id)
                .map_err(|error| error.to_string())?;
        }
        outcome(doc)
    })
}

/// Deletes the current card.
#[wasm_bindgen]
pub fn delete_card(_args: &str) -> ApiResult {
    with_doc(|doc| {
        let card = doc.runtime.current_card();
        doc.runtime
            .execute(Command::DeleteCard { id: card })
            .map_err(|error| error.to_string())?;
        doc.dirty = true;
        outcome(doc)
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewPartArgs {
    kind: String,
    layer: Layer,
    name: Option<String>,
}

/// Which layer a new part belongs to.
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Layer {
    Card,
    Background,
}

/// Adds a part to the card or its background.
#[wasm_bindgen]
pub fn new_part(args: &str) -> ApiResult {
    let args: NewPartArgs = parse_args(args)?;
    with_doc(|doc| {
        let part_kind = match args.kind.as_str() {
            "button" => PartKind::Button,
            "field" => PartKind::Field,
            "image" => PartKind::Image,
            other => return Err(format!("\"{other}\" is not a kind of part")),
        };

        let card = doc.runtime.current_card();
        let owner = owner_for(doc, card, args.layer)?;

        // The cascade counts *every* part on show, not just this kind, so a
        // new button and a new field do not land exactly on top of each
        // other. The name still counts its own kind, so they are "Button 1"
        // and "Field 1".
        let placed = count_parts(doc.runtime.stack(), card, None);
        let same_kind = count_parts(doc.runtime.stack(), card, Some(part_kind));
        let geometry = doc.runtime.stack().default_part_geometry(part_kind, placed);
        let name = args
            .name
            .unwrap_or_else(|| format!("{} {}", title_case(&args.kind), same_kind + 1));

        doc.runtime
            .execute(Command::CreatePart {
                owner,
                kind: part_kind,
                name,
                geometry,
            })
            .map_err(|error| error.to_string())?;
        doc.dirty = true;
        outcome(doc)
    })
}

#[derive(Deserialize)]
struct ImportImageArgs {
    name: String,
    layer: Layer,
}

/// Brings a picture into the stack and puts an image part on the card.
///
/// The bytes arrive from the page — a file the user picked — and go straight
/// into the model, which checks them before the renderer ever sees them.
#[wasm_bindgen]
pub fn import_image_bytes(args: &str, bytes: Vec<u8>) -> ApiResult {
    let args: ImportImageArgs = parse_args(args)?;
    if bytes.len() > hyperlab_stack::MAX_IMAGE_BYTES {
        return Err(fail(format!(
            "that file is {} MB, and the most a stack will hold is {} MB",
            bytes.len() / (1024 * 1024),
            hyperlab_stack::MAX_IMAGE_BYTES / (1024 * 1024)
        )));
    }
    let image =
        hyperlab_stack::Image::new(&args.name, bytes).map_err(|error| fail(error.to_string()))?;

    with_doc(|doc| {
        let card = doc.runtime.current_card();
        let owner = owner_for(doc, card, args.layer)?;
        let placed = count_parts(doc.runtime.stack(), card, None);
        let geometry = doc
            .runtime
            .stack()
            .default_part_geometry(PartKind::Image, placed);

        doc.runtime
            .execute(Command::SetImage {
                name: args.name.clone(),
                image: Some(Box::new(image)),
            })
            .map_err(|error| error.to_string())?;
        let created = doc
            .runtime
            .execute(Command::CreatePart {
                owner,
                kind: PartKind::Image,
                name: args.name.clone(),
                geometry,
            })
            .map_err(|error| error.to_string())?;

        if let Some(part) = created {
            doc.runtime
                .execute(Command::SetProperty {
                    object: part,
                    property: "source".into(),
                    value: Some(Value::text(args.name.clone())),
                })
                .map_err(|error| error.to_string())?;
        }
        doc.dirty = true;
        outcome(doc)
    })
}

/// Removes a part.
#[wasm_bindgen]
pub fn delete_part(args: &str) -> ApiResult {
    let args: IdArgs = parse_args(args)?;
    with_doc(|doc| {
        doc.runtime
            .execute(Command::DeletePart {
                id: Id::new(args.id),
            })
            .map_err(|error| error.to_string())?;
        doc.dirty = true;
        outcome(doc)
    })
}

#[derive(Deserialize)]
struct GeometryArgs {
    id: u64,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

/// Moves or resizes a part, as dragging it does.
#[wasm_bindgen]
pub fn set_geometry(args: &str) -> ApiResult {
    let args: GeometryArgs = parse_args(args)?;
    with_doc(|doc| {
        doc.runtime
            .execute(Command::SetGeometry {
                id: Id::new(args.id),
                geometry: Rect::new(args.left, args.top, args.width, args.height),
            })
            .map_err(|error| error.to_string())?;
        doc.dirty = true;
        outcome(doc)
    })
}

#[derive(Deserialize)]
struct SetPropertyArgs {
    kind: String,
    id: u64,
    property: String,
    value: serde_json::Value,
}

/// Sets a property from the inspector.
#[wasm_bindgen]
pub fn set_property(args: &str) -> ApiResult {
    let args: SetPropertyArgs = parse_args(args)?;
    let object = object_id(&args.kind, args.id).map_err(fail)?;
    with_doc(|doc| {
        doc.runtime
            .execute(Command::SetProperty {
                object,
                property: args.property.clone(),
                value: Some(json_to_value(args.value.clone())),
            })
            .map_err(|error| error.to_string())?;
        doc.dirty = true;
        outcome(doc)
    })
}

#[derive(Deserialize)]
struct SetScriptArgs {
    kind: String,
    id: u64,
    script: String,
}

/// Replaces an object's script.
///
/// A script that does not parse is still stored — half-written code is
/// normal, and losing it would be worse than keeping it. The editor checks
/// separately, with [`check_script`], and says so.
#[wasm_bindgen]
pub fn set_script(args: &str) -> ApiResult {
    let args: SetScriptArgs = parse_args(args)?;
    let object = object_id(&args.kind, args.id).map_err(fail)?;
    with_doc(|doc| {
        doc.runtime
            .execute(Command::SetScript {
                object,
                script: args.script.clone(),
            })
            .map_err(|error| error.to_string())?;
        doc.dirty = true;
        outcome(doc)
    })
}

#[derive(Deserialize)]
struct RenameArgs {
    kind: String,
    id: u64,
    name: String,
}

/// Renames an object.
#[wasm_bindgen]
pub fn rename(args: &str) -> ApiResult {
    let args: RenameArgs = parse_args(args)?;
    let object = object_id(&args.kind, args.id).map_err(fail)?;
    with_doc(|doc| {
        doc.runtime
            .execute(Command::Rename {
                object,
                name: args.name.clone(),
            })
            .map_err(|error| error.to_string())?;
        doc.dirty = true;
        outcome(doc)
    })
}

#[derive(Deserialize)]
struct SizeArgs {
    width: i32,
    height: i32,
}

/// Resizes every card in the stack.
#[wasm_bindgen]
pub fn set_stack_size(args: &str) -> ApiResult {
    let args: SizeArgs = parse_args(args)?;
    with_doc(|doc| {
        doc.runtime
            .execute(Command::SetStackSize {
                size: Size::new(args.width, args.height),
            })
            .map_err(|error| error.to_string())?;
        doc.dirty = true;
        outcome(doc)
    })
}

/// Undoes the last change.
#[wasm_bindgen]
pub fn undo(_args: &str) -> ApiResult {
    with_doc(|doc| {
        doc.runtime.undo().map_err(|error| error.to_string())?;
        doc.dirty = true;
        outcome(doc)
    })
}

/// Redoes the last undone change.
#[wasm_bindgen]
pub fn redo(_args: &str) -> ApiResult {
    with_doc(|doc| {
        doc.runtime.redo().map_err(|error| error.to_string())?;
        doc.dirty = true;
        outcome(doc)
    })
}

// ------------------------------------------------------------------- files

#[derive(Deserialize)]
struct NewStackArgs {
    name: Option<String>,
}

/// Starts a new, empty stack.
#[wasm_bindgen]
pub fn new_stack(args: &str) -> ApiResult {
    let args: NewStackArgs = parse_args(args)?;
    with_doc(|doc| {
        doc.runtime
            .open(Stack::new(args.name.unwrap_or_else(|| "Untitled".into())));
        doc.dirty = false;
        // `openStack` runs on open, which is how a stack sets itself up.
        let _ = doc.runtime.open_stack();
        outcome(doc)
    })
}

#[derive(Deserialize)]
struct OpenStackArgs {
    text: String,
}

/// Opens a stack from the single-file JSON the desktop's
/// "Save As…" and this module's [`save_stack_json`] both write.
#[wasm_bindgen]
pub fn open_stack_json(args: &str) -> ApiResult {
    let args: OpenStackArgs = parse_args(args)?;
    let stack = hyperlab_persistence::stack_from_single_file(&args.text)
        .map_err(|error| fail(error.to_string()))?;
    with_doc(|doc| {
        doc.runtime.open(stack);
        doc.dirty = false;
        let _ = doc.runtime.open_stack();
        outcome(doc)
    })
}

/// The whole stack as single-file JSON, for the page to hand the user as a
/// download. Marks the document clean: the copy the user holds *is* the
/// save.
#[wasm_bindgen]
pub fn save_stack_json(_args: &str) -> ApiResult {
    #[derive(Serialize)]
    struct Saved {
        name: String,
        text: String,
    }
    with_doc(|doc| {
        let text = hyperlab_persistence::single_file_string(doc.runtime.stack())
            .map_err(|error| error.to_string())?;
        doc.dirty = false;
        serde_json::to_string(&Saved {
            name: doc.runtime.stack().name().to_string(),
            text,
        })
        .map_err(|error| error.to_string())
    })
}

/// Writes the stack as a web page driven by _hyperscript, with a line for
/// anything that had no equivalent on a page.
#[wasm_bindgen]
pub fn export_web(_args: &str) -> ApiResult {
    #[derive(Serialize)]
    struct Exported {
        source: String,
        notes: Vec<String>,
    }
    with_doc(|doc| {
        let translated = hyperlab_hyperscript::page(doc.runtime.stack());
        serde_json::to_string(&Exported {
            source: translated.source,
            notes: translated.notes,
        })
        .map_err(|error| error.to_string())
    })
}

// ------------------------------------------------------------------ helpers

fn object_id(kind: &str, id: u64) -> Result<ObjectId, String> {
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
fn find_part(runtime: &Runtime, id: Id) -> Result<ObjectId, String> {
    runtime
        .stack()
        .part(id)
        .map(|part| ObjectId::new(part.kind(), id))
        .ok_or_else(|| format!("there is no part with id {id}"))
}

/// Which container a new part on `card` belongs to.
fn owner_for(doc: &Doc, card: Id, layer: Layer) -> Result<PartOwner, String> {
    Ok(match layer {
        Layer::Card => PartOwner::Card { id: card },
        Layer::Background => PartOwner::Background {
            id: doc
                .runtime
                .stack()
                .background_of(card)
                .map(Object::id)
                .ok_or("this card has no background")?,
        },
    })
}

/// How many parts the current card shows, including its background's.
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

/// The provider's headers as the JSON object the host expects.
fn headers_json(provider: &WebProvider) -> String {
    let map: std::collections::BTreeMap<String, String> = provider.headers().into_iter().collect();
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
}

/// Reads the host's `{"status": …, "body": …}` reply.
fn split_reply(reply: &str) -> Result<(u16, String), String> {
    #[derive(Deserialize)]
    struct Reply {
        status: u16,
        body: String,
    }
    serde_json::from_str::<Reply>(reply)
        .map(|reply| (reply.status, reply.body))
        .map_err(|error| format!("the transport answered something unexpected: {error}"))
}

/// A thrown JavaScript value as a sentence.
fn text_of(thrown: &JsValue) -> String {
    thrown.as_string().unwrap_or_else(|| {
        js_sys::Reflect::get(thrown, &JsValue::from_str("message"))
            .ok()
            .and_then(|message| message.as_string())
            .unwrap_or_else(|| "an unknown error".to_string())
    })
}

// ------------------------------------------------------------------- the AI

/// What the sidebar shows. A twin of the desktop's `AiView`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AiView {
    entries: Vec<hyperlab_assistant::Entry>,
    providers: Vec<String>,
    provider: Option<String>,
    problems: Vec<String>,
    sends_field_text: bool,
    may_edit: bool,
    busy: bool,
}

impl Ai {
    fn view(&self) -> AiView {
        AiView {
            entries: self.conversation.entries().to_vec(),
            providers: self
                .providers
                .iter()
                .map(|provider| provider.name.clone())
                .collect(),
            provider: self.chosen(),
            problems: self.problems.clone(),
            sends_field_text: self.context.include_field_text,
            may_edit: !self
                .policy
                .would_always_refuse("write_field", Access::Write),
            busy: self.busy,
        }
    }

    fn chosen(&self) -> Option<String> {
        let named = |name: &String| self.providers.iter().any(|provider| &provider.name == name);
        self.settings
            .default_provider
            .clone()
            .filter(named)
            .or_else(|| self.providers.first().map(|provider| provider.name.clone()))
    }

    fn chosen_provider(&self) -> Result<&WebProvider, String> {
        let name = self
            .chosen()
            .ok_or("no language model is set up yet — add one in AI ▸ Settings")?;
        self.providers
            .iter()
            .find(|provider| provider.name == name)
            .ok_or_else(|| format!("the provider \"{name}\" is no longer available"))
    }
}

/// What the sidebar should draw.
#[wasm_bindgen]
pub fn ai_view(_args: &str) -> ApiResult {
    with_ai(|ai| serde_json::to_string(&ai.view()).map_err(|error| error.to_string()))
}

/// Forgets the conversation.
#[wasm_bindgen]
pub fn ai_clear(_args: &str) -> ApiResult {
    with_ai(|ai| {
        ai.conversation.clear();
        serde_json::to_string(&ai.view()).map_err(|error| error.to_string())
    })
}

#[derive(Deserialize)]
struct SendingArgs {
    sending: bool,
}

/// Chooses whether the contents of fields are sent with a question.
#[wasm_bindgen]
pub fn ai_set_sends_field_text(args: &str) -> ApiResult {
    let args: SendingArgs = parse_args(args)?;
    with_ai(|ai| {
        ai.context = if args.sending {
            ContextOptions::everything()
        } else {
            ContextOptions::default()
        };
        serde_json::to_string(&ai.view()).map_err(|error| error.to_string())
    })
}

#[derive(Deserialize)]
struct EditingArgs {
    editing: bool,
}

/// Chooses whether the assistant may change the stack.
#[wasm_bindgen]
pub fn ai_set_may_edit(args: &str) -> ApiResult {
    let args: EditingArgs = parse_args(args)?;
    with_ai(|ai| {
        ai.policy = if args.editing {
            Policy::trusted()
        } else {
            Policy::new()
        };
        serde_json::to_string(&ai.view()).map_err(|error| error.to_string())
    })
}

/// The provider settings, for the settings panel.
#[wasm_bindgen]
pub fn ai_settings(_args: &str) -> ApiResult {
    with_ai(|ai| serde_json::to_string(&ai.settings).map_err(|error| error.to_string()))
}

#[derive(Deserialize)]
struct SaveSettingsArgs {
    settings: AiSettings,
}

/// Writes new provider settings to browser storage and rebuilds the
/// providers.
#[wasm_bindgen]
pub fn ai_save_settings(args: &str) -> ApiResult {
    let args: SaveSettingsArgs = parse_args(args)?;
    let text = serde_json::to_string(&args.settings).map_err(fail)?;
    with_host(|host| host.storage_set(SETTINGS_KEY, Some(text)));

    let (resolved, problems) = resolve_providers(&args.settings);
    with_ai(|ai| {
        ai.settings = args.settings.clone();
        ai.providers = resolved;
        ai.problems = problems;
        serde_json::to_string(&ai.view()).map_err(|error| error.to_string())
    })
}

/// What the settings panel may say about keys: which providers have one
/// saved in this browser, and never which key.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KeychainView {
    available: bool,
    problem: Option<String>,
    holding: Vec<String>,
}

fn keychain_view(ai: &Ai) -> Result<String, String> {
    let holding = ai
        .settings
        .providers
        .keys()
        .filter(|name| with_host(|host| host.storage_get(&key_slot(name))).is_some())
        .cloned()
        .collect();
    serde_json::to_string(&KeychainView {
        available: true,
        problem: None,
        holding,
    })
    .map_err(|error| error.to_string())
}

/// Which providers have a key saved in this browser.
#[wasm_bindgen]
pub fn ai_keychain(_args: &str) -> ApiResult {
    with_ai(|ai| keychain_view(ai))
}

#[derive(Deserialize)]
struct SetKeyArgs {
    provider: String,
    key: String,
}

/// Saves a provider's key in this browser's storage.
///
/// The key arrives, goes into storage under the provider's name, and is not
/// returned, logged, or sent anywhere except the provider itself when a
/// request is built. There is no command that reads one back out.
#[wasm_bindgen]
pub fn ai_set_key(args: &str) -> ApiResult {
    let args: SetKeyArgs = parse_args(args)?;
    with_host(|host| {
        host.storage_set(
            &key_slot(args.provider.trim()),
            Some(args.key.trim().to_string()),
        );
    });
    // A provider that failed to build for want of this key can build now.
    let settings = with_ai(|ai| Ok(ai.settings.clone()))?;
    let (resolved, problems) = resolve_providers(&settings);
    with_ai(|ai| {
        ai.providers = resolved;
        ai.problems = problems;
        keychain_view(ai)
    })
}

#[derive(Deserialize)]
struct ForgetKeyArgs {
    provider: String,
}

/// Removes a provider's key from this browser's storage.
#[wasm_bindgen]
pub fn ai_forget_key(args: &str) -> ApiResult {
    let args: ForgetKeyArgs = parse_args(args)?;
    with_host(|host| host.storage_set(&key_slot(args.provider.trim()), None));
    let settings = with_ai(|ai| Ok(ai.settings.clone()))?;
    let (resolved, problems) = resolve_providers(&settings);
    with_ai(|ai| {
        ai.providers = resolved;
        ai.problems = problems;
        keychain_view(ai)
    })
}

#[derive(Deserialize)]
struct AskArgs {
    question: String,
}

/// What one round of the turn loop needs from the AI cell while unlocked:
/// everything already turned into strings.
struct PreparedRound {
    wire: Wire,
    url: String,
    headers: String,
    body: String,
    request: CompletionRequest,
}

/// Asks the assistant something, and runs whatever tools it asks for.
///
/// A twin of the desktop's turn loop, with the same shape for the same
/// reason: the borrow of the document is taken for the tools and dropped
/// for the network, so the page stays live while the model thinks.
#[wasm_bindgen]
pub async fn ai_ask(args: String) -> Result<String, JsValue> {
    let args: AskArgs = parse_args(&args)?;
    if args.question.trim().is_empty() {
        return Err(fail("there is nothing to ask"));
    }

    // Whatever happens from here, the sidebar must not be left stuck.
    let turn = run_turn(&args.question).await;
    with_ai(|ai| {
        ai.busy = false;
        Ok(())
    })?;

    if let Err(reason) = &turn {
        let reason = reason
            .as_string()
            .unwrap_or_else(|| "the turn failed".to_string());
        with_ai(|ai| {
            ai.conversation.record_failure(reason.clone());
            Ok(())
        })?;
        return Err(fail(reason));
    }

    // The stack may have changed even when the turn failed part-way, so the
    // window is refreshed either way.
    with_doc(outcome)
}

async fn run_turn(question: &str) -> Result<(), JsValue> {
    let tools = ToolRegistry::new();

    let model = with_ai(|ai| {
        if ai.busy {
            return Err("the assistant is still working on the last question".to_string());
        }
        let provider = ai.chosen_provider()?;
        let model = provider.model.clone();
        ai.busy = true;
        Ok(model)
    })?;

    // Borrowed: describing the stack reads it.
    let briefing = with_doc(|doc| Ok(Briefing::about(&doc.runtime, ai_context()?)))?;
    with_ai(|ai| {
        ai.conversation.ask(question, briefing);
        Ok(())
    })?;

    loop {
        let prepared = with_ai(|ai| {
            if !ai.conversation.begin_round() {
                return Err("the assistant kept using tools without answering".to_string());
            }
            let provider = ai.chosen_provider()?;
            let request = ai.conversation.request(&model, tools.definitions());
            Ok(PreparedRound {
                wire: provider.wire,
                url: provider.completion_url(),
                headers: headers_json(provider),
                body: provider.completion_body(&request).to_string(),
                request,
            })
        })?;

        // Unborrowed: this is the slow part, and it needs no stack.
        let completion = if prepared.wire == Wire::Mock {
            providers::mock_completion(&prepared.request)
        } else {
            let promise =
                with_host(|host| host.complete(&prepared.url, &prepared.headers, &prepared.body));
            let reply = wasm_bindgen_futures::JsFuture::from(promise)
                .await
                .map_err(|thrown| {
                    fail(format!(
                        "could not reach the provider: {}",
                        text_of(&thrown)
                    ))
                })?;
            let reply = reply
                .as_string()
                .ok_or_else(|| fail("the transport answered something unexpected"))?;
            let (status, body) = split_reply(&reply).map_err(fail)?;
            with_ai(|ai| ai.chosen_provider()?.decode_completion(status, &body))?
        };

        let done = with_ai(|ai| {
            ai.conversation.record_reply(&completion);
            Ok(completion.tool_calls.is_empty())
        })?;
        if done {
            return Ok(());
        }

        // Borrowed again: tools go through the command bus like anyone else.
        with_doc(|doc| {
            AI.with(|cell| {
                let mut held = cell.borrow_mut();
                let ai = held.as_mut().ok_or("init() has not run")?;
                let outcomes = tools::run(
                    &mut doc.runtime,
                    &tools,
                    &mut ai.policy,
                    &mut SidebarUser,
                    &completion.tool_calls,
                );
                if outcomes.iter().any(|outcome| outcome.allowed) {
                    doc.dirty = true;
                }
                for outcome in &outcomes {
                    ai.conversation.record_tool(outcome);
                }
                Ok(())
            })
        })?;
    }
}

/// The context options as they stand, read without holding the borrow.
fn ai_context() -> Result<ContextOptions, String> {
    AI.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|ai| ai.context)
            .ok_or_else(|| "init() has not run".to_string())
    })
}
