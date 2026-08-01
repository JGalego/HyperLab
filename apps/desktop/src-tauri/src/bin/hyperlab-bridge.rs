//! A development bridge, so a browser can drive a real HyperLab.
//!
//! HyperLab's window is a WebKit view owned by Tauri, and nothing in the
//! Playwright family can attach to one. That would leave a demo or a
//! screenshot test with only two options: film nothing, or film a mock.
//!
//! This is the third. It puts the *real* runtime — the real command bus, the
//! real assistant, the real MCP tools — behind the same `invoke` calls the
//! window makes, over HTTP. Point a browser at the Vite dev server with a
//! shim installed (see `demo/shim.js`) and the interface cannot tell the
//! difference, because there is none: the same React, the same snapshot, the
//! same `Runtime`.
//!
//! ```sh
//! hyperlab-bridge --port 7878 [--stack "examples/Recipe Box.hl"]
//! ```
//!
//! Not part of the application, and not a way to run HyperLab. It binds to
//! loopback only, speaks to whatever asks, and has no authentication — which
//! is exactly why it is a separate binary you have to start on purpose.

use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex, PoisonError,
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    time::Duration,
};

use hyperlab_desktop::{
    assistant::AiState,
    dialogs::DialogRequest,
    state::{AppState, lock},
    view,
};
use hyperlab_runtime::{AiRequest, Command, Effect, Host, Message, PartOwner, Runtime};
use hyperlab_stack::{Id, Object, ObjectId, ObjectKind, PartKind, Rect, Value};
use serde_json::{Value as Json, json};

/// How long a script may wait for a dialog nobody answers.
const PATIENCE: Duration = Duration::from_secs(60);

/// The largest request body worth reading from a local browser.
const MAX_BODY: usize = 4 * 1024 * 1024;

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(reason) => {
            eprintln!("hyperlab-bridge: {reason}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut port = 7878u16;
    let mut stack = None;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--port" => {
                port = arguments
                    .next()
                    .ok_or("--port needs a number")?
                    .parse()
                    .map_err(|_| "--port needs a number")?;
            }
            "--stack" => stack = Some(arguments.next().ok_or("--stack needs a path")?),
            other => return Err(format!("I do not understand \"{other}\"")),
        }
    }

    let app = AppState::new();
    if let Some(path) = &stack {
        let opened = hyperlab_persistence::load(path)
            .map_err(|error| format!("could not open {path}: {error}"))?;
        let session = app.session();
        let mut held = lock(&session);
        held.runtime.open(opened);
        // The window sends these on opening, so the bridge has to as well:
        // a stack whose `openStack` or `openCard` handler sets something up
        // is a different stack without them, and the whole point of this
        // binary is that the interface cannot tell the difference.
        let _ = held.runtime.open_stack();
    }

    // No settings file: the demo passes providers in over the wire, so a
    // machine that has HyperLab configured does not have its own settings
    // quietly picked up by a script.
    let ai = AiState::new(Default::default(), Default::default(), Vec::new());

    let dialogs = Arc::new(Pending::default());
    app.install_host(Box::new(BridgeHost {
        pending: Arc::clone(&dialogs),
        assistant: ai.handle(),
    }));

    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("could not listen on {port}: {error}"))?;
    eprintln!("hyperlab-bridge listening on http://127.0.0.1:{port}");

    let shared = Arc::new(Bridge { app, ai, dialogs });
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let shared = Arc::clone(&shared);
        // A thread each, so a script blocked on a dialog does not stop the
        // reply to that dialog arriving.
        std::thread::spawn(move || {
            if let Err(error) = serve(&shared, stream) {
                eprintln!("hyperlab-bridge: {error}");
            }
        });
    }
    Ok(())
}

/// Everything one bridge holds.
struct Bridge {
    app: AppState,
    ai: AiState,
    dialogs: Arc<Pending>,
}

// ------------------------------------------------------------------- dialogs

/// Dialogs waiting to be shown, and the script waiting for an answer.
#[derive(Default)]
struct Pending {
    queue: Mutex<VecDeque<DialogRequest>>,
    waiting: Mutex<Option<SyncSender<Option<String>>>>,
}

impl Pending {
    fn show(&self, request: DialogRequest) -> Receiver<Option<String>> {
        let (sender, receiver) = sync_channel(1);
        if let Some(stale) = self.lock_waiting().replace(sender) {
            let _ = stale.send(None);
        }
        self.lock_queue().push_back(request);
        receiver
    }

    fn take(&self) -> Vec<DialogRequest> {
        self.lock_queue().drain(..).collect()
    }

    fn reply(&self, text: Option<String>) -> bool {
        match self.lock_waiting().take() {
            Some(sender) => sender.send(text).is_ok(),
            None => false,
        }
    }

    fn lock_queue(&self) -> std::sync::MutexGuard<'_, VecDeque<DialogRequest>> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_waiting(&self) -> std::sync::MutexGuard<'_, Option<SyncSender<Option<String>>>> {
        self.waiting.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The same bargain [`DesktopHost`](hyperlab_desktop::DesktopHost) makes, with
/// a queue where the window would be: the script blocks, the browser polls,
/// the answer comes back.
struct BridgeHost {
    pending: Arc<Pending>,
    assistant: AiState,
}

impl BridgeHost {
    fn show(&self, request: DialogRequest) -> Option<String> {
        self.pending
            .show(request)
            .recv_timeout(PATIENCE)
            .unwrap_or(None)
    }
}

impl Host for BridgeHost {
    fn answer(&mut self, message: &str) {
        self.show(DialogRequest::Answer {
            message: message.to_string(),
        });
    }

    fn ask(&mut self, prompt: &str, default: &str) -> Option<String> {
        self.show(DialogRequest::Ask {
            prompt: prompt.to_string(),
            default: default.to_string(),
        })
    }

    fn ai(&mut self, request: &AiRequest) -> Result<String, String> {
        // On a thread with a deadline, as the window does it: a provider
        // that never answers must not hold the session lock for ever. A
        // script asking the assistant is a thing stacks do, so a bridge that
        // refused it would be filming a different application.
        let (sender, receiver) = sync_channel(1);
        let assistant = self.assistant.clone();
        let prompt = request.prompt.clone();
        std::thread::spawn(move || {
            let _ = sender.send(assistant.answer(&prompt));
        });
        receiver
            .recv_timeout(PATIENCE)
            .unwrap_or_else(|_| Err("the assistant did not answer in time".to_string()))
    }
}

// ---------------------------------------------------------------------- HTTP

fn serve(bridge: &Bridge, mut stream: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);

    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();

    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 {
            break;
        }
        if header.trim().is_empty() {
            break;
        }
        if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
    }

    if length > MAX_BODY {
        return respond(&mut stream, 413, &json!({ "error": "that is too much" }));
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;

    // The browser is on a different origin from this port, and this is a
    // loopback development tool, so it says yes to all of them.
    if method == "OPTIONS" {
        return respond(&mut stream, 204, &Json::Null);
    }

    let answer = if path == "/events" {
        json!({ "ok": bridge.dialogs.take() })
    } else if let Some(command) = path.strip_prefix("/invoke/") {
        let arguments: Json = serde_json::from_slice(&body).unwrap_or_else(|_| json!({}));
        match dispatch(bridge, command, &arguments) {
            Ok(value) => json!({ "ok": value }),
            Err(reason) => json!({ "error": reason }),
        }
    } else {
        json!({ "error": format!("there is no {path}") })
    };

    respond(&mut stream, 200, &answer)
}

fn respond(stream: &mut TcpStream, status: u16, body: &Json) -> std::io::Result<()> {
    let text = if body.is_null() {
        String::new()
    } else {
        body.to_string()
    };
    write!(
        stream,
        "HTTP/1.1 {status} OK\r\n\
         content-type: application/json\r\n\
         content-length: {}\r\n\
         access-control-allow-origin: *\r\n\
         access-control-allow-headers: content-type\r\n\
         access-control-allow-methods: GET, POST, OPTIONS\r\n\
         connection: close\r\n\r\n{text}",
        text.len()
    )?;
    stream.flush()
}

// ------------------------------------------------------------------ commands

/// The commands the demo needs, against the same runtime the window uses.
///
/// Deliberately a subset. This is not a second front end, and a command that
/// is missing here is a command the film does not use.
fn dispatch(bridge: &Bridge, command: &str, arguments: &Json) -> Result<Json, String> {
    let session = bridge.app.session();

    let text = |key: &str| -> Result<String, String> {
        arguments
            .get(key)
            .and_then(Json::as_str)
            .map(String::from)
            .ok_or_else(|| format!("{command} needs {key}"))
    };
    let number = |key: &str| -> Result<i64, String> {
        arguments
            .get(key)
            .and_then(Json::as_i64)
            .ok_or_else(|| format!("{command} needs {key}"))
    };

    // Events are answered without the runtime, since a script may be holding
    // it while it waits for exactly this.
    match command {
        "plugin:event|listen" | "plugin:event|unlisten" => return Ok(Json::Null),
        "dialog_reply" => {
            let reply = arguments
                .get("text")
                .and_then(Json::as_str)
                .map(String::from);
            return Ok(json!(bridge.dialogs.reply(reply)));
        }
        "ai_ask" => {
            // Locks and unlocks the session several times, so it must not be
            // called with it held.
            let asked = bridge.ai.ask(&session, &text("question")?);
            let outcome = finish(&mut lock(&session));
            return asked.map(|()| outcome);
        }
        _ => {}
    }

    let mut held = lock(&session);
    let runtime = &mut held.runtime;

    let value = match command {
        "get_view" => finish(&mut held),
        "check_script" => {
            Runtime::check_script(&text("source")?).map_err(|error| error.to_string())?;
            Json::Null
        }
        "click_part" => {
            let id = Id::new(number("id")? as u64);
            let part = runtime
                .stack()
                .part(id)
                .ok_or_else(|| format!("there is no part {id}"))?;
            let clicked = ObjectId::new(part.kind(), id);
            runtime
                .send_message(&Message::new("mouseUp"), clicked)
                .map_err(|error| error.to_string())?;
            finish(&mut held)
        }
        "set_field_text" => {
            let id = Id::new(number("id")? as u64);
            runtime
                .execute(Command::SetProperty {
                    object: ObjectId::new(ObjectKind::Field, id),
                    property: "text".into(),
                    value: Some(Value::text(text("text")?)),
                })
                .map_err(|error| error.to_string())?;
            held.touch();
            finish(&mut held)
        }
        "go_to_card" => {
            let position = number("position")?;
            runtime
                .go_to_index(position as isize - 1)
                .map_err(|error| error.to_string())?;
            finish(&mut held)
        }
        "new_card" => {
            let after = runtime.current_card_index();
            runtime
                .execute(Command::CreateCard {
                    after,
                    background: None,
                })
                .map_err(|error| error.to_string())?;
            runtime
                .go_to_index(after as isize + 1)
                .map_err(|error| error.to_string())?;
            held.touch();
            finish(&mut held)
        }
        "new_part" => {
            let kind = match text("kind")?.as_str() {
                "button" => PartKind::Button,
                _ => PartKind::Field,
            };
            let owner = PartOwner::Card {
                id: runtime.current_card(),
            };
            let name = arguments
                .get("name")
                .and_then(Json::as_str)
                .map_or_else(|| "New".to_string(), String::from);
            runtime
                .execute(Command::CreatePart {
                    owner,
                    kind,
                    name,
                    geometry: Rect::new(40, 40, 120, 24),
                })
                .map_err(|error| error.to_string())?;
            held.touch();
            finish(&mut held)
        }
        "set_script" => {
            let object = object_id(&text("kind")?, number("id")? as u64)?;
            runtime
                .execute(Command::SetScript {
                    object,
                    script: text("script")?,
                })
                .map_err(|error| error.to_string())?;
            held.touch();
            finish(&mut held)
        }
        "set_geometry" => {
            let id = Id::new(number("id")? as u64);
            let rectangle = Rect::new(
                number("left")? as i32,
                number("top")? as i32,
                number("width")? as i32,
                number("height")? as i32,
            );
            runtime
                .execute(Command::SetGeometry {
                    id,
                    geometry: rectangle,
                })
                .map_err(|error| error.to_string())?;
            held.touch();
            finish(&mut held)
        }
        "run_message_box" => {
            let me = ObjectId::new(ObjectKind::Card, runtime.current_card());
            match runtime.run_script(&text("source")?, me) {
                Ok(_) => {}
                Err(error) => runtime.set_message_box(error.to_string()),
            }
            held.touch();
            finish(&mut held)
        }
        "undo" => {
            runtime.undo().map_err(|error| error.to_string())?;
            held.touch();
            finish(&mut held)
        }
        "redo" => {
            runtime.redo().map_err(|error| error.to_string())?;
            held.touch();
            finish(&mut held)
        }
        "stack_graph" => serde_json::to_value(hyperlab_graph::Graph::of(runtime.stack()))
            .map_err(|error| error.to_string())?,
        "stack_image" => {
            let name = text("name")?;
            let uri = runtime
                .stack()
                .image(&name)
                .map(hyperlab_stack::data_uri)
                .ok_or_else(|| format!("this stack has no picture called \"{name}\""))?;
            Json::String(uri)
        }
        "stack_images" => {
            serde_json::to_value(runtime.stack().images().keys().cloned().collect::<Vec<_>>())
                .map_err(|error| error.to_string())?
        }
        "get_properties" => {
            let object = object_id(&text("kind")?, number("id")? as u64)?;
            let described = runtime.object(object).map_err(|error| error.to_string())?;
            serde_json::to_value(view::properties_of(described))
                .map_err(|error| error.to_string())?
        }
        // The assistant's own state needs no runtime, but arrives here so
        // that one match holds the whole command list.
        "ai_view" => serde_json::to_value(bridge.ai.view()).map_err(|error| error.to_string())?,
        "ai_clear" => {
            bridge.ai.clear();
            serde_json::to_value(bridge.ai.view()).map_err(|error| error.to_string())?
        }
        "ai_set_may_edit" => {
            bridge
                .ai
                .set_may_edit(arguments["editing"].as_bool().unwrap_or(false));
            serde_json::to_value(bridge.ai.view()).map_err(|error| error.to_string())?
        }
        "ai_set_sends_field_text" => {
            bridge
                .ai
                .set_sends_field_text(arguments["sending"].as_bool().unwrap_or(false));
            serde_json::to_value(bridge.ai.view()).map_err(|error| error.to_string())?
        }
        "ai_settings" => {
            serde_json::to_value(bridge.ai.settings()).map_err(|error| error.to_string())?
        }
        "ai_save_settings" => {
            let settings: hyperlab_ai::AiSettings =
                serde_json::from_value(arguments["settings"].clone())
                    .map_err(|error| error.to_string())?;
            let (registry, problems) = hyperlab_desktop::settings::build(&settings);
            bridge.ai.reconfigure(settings, registry, problems);
            serde_json::to_value(bridge.ai.view()).map_err(|error| error.to_string())?
        }
        // The settings panel asks on the way up, so a film that opens it
        // would draw nothing without these three.
        "export_pdf" => {
            // `held` above already has the session; locking it again here is a
            // deadlock, not a second reader.
            let pdf = hyperlab_export::to_pdf(runtime.stack()).map_err(|error| error.to_string())?;
            let path = text("path")?;
            std::fs::write(&path, pdf).map_err(|error| error.to_string())?;
            json!(path)
        }
        "export_png" => {
            let path = text("path")?;
            let bytes: Vec<u8> = serde_json::from_value(arguments["bytes"].clone())
                .map_err(|error| error.to_string())?;
            std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
            json!(path)
        }
        "export_web" | "export_deck" => {
            let (source, notes) = if command == "export_web" {
                let page = hyperlab_hyperscript::page(runtime.stack());
                (page.source, page.notes)
            } else {
                let deck = hyperlab_decker::deck(runtime.stack());
                (deck.source, deck.notes)
            };
            let path = text("path")?;
            std::fs::write(&path, source).map_err(|error| error.to_string())?;
            json!({ "path": path, "notes": notes })
        }
        "ai_keychain" => keychain(bridge),
        "ai_set_key" => {
            hyperlab_desktop::keys::set(&text("provider")?, &text("key")?)?;
            keychain(bridge)
        }
        "ai_forget_key" => {
            hyperlab_desktop::keys::forget(&text("provider")?)?;
            keychain(bridge)
        }
        other => return Err(format!("the bridge does not carry \"{other}\"")),
    };
    Ok(value)
}

/// The same summary the Tauri command returns: whether there is a keychain,
/// and which providers have a key in it. Never a key.
fn keychain(bridge: &Bridge) -> serde_json::Value {
    let problem = hyperlab_desktop::keys::available().err();
    let holding: Vec<String> = match problem {
        Some(_) => Vec::new(),
        None => bridge
            .ai
            .settings()
            .providers
            .into_keys()
            .filter(|name| hyperlab_desktop::keys::holds(name))
            .collect(),
    };
    serde_json::json!({
        "available": problem.is_none(),
        "problem": problem,
        "holding": holding,
    })
}

fn object_id(kind: &str, id: u64) -> Result<ObjectId, String> {
    let kind = match kind {
        "stack" => ObjectKind::Stack,
        "background" => ObjectKind::Background,
        "card" => ObjectKind::Card,
        "button" => ObjectKind::Button,
        "field" => ObjectKind::Field,
        other => return Err(format!("there is no such thing as a {other}")),
    };
    Ok(ObjectId::new(kind, Id::new(id)))
}

/// The same shape every command in the window gives back.
fn finish(session: &mut hyperlab_desktop::state::Session) -> Json {
    let effects: Vec<Effect> = session.runtime.take_effects();
    let path = session.path_string();
    json!({
        "view": view::snapshot(&session.runtime, session.dirty, path),
        "effects": effects,
    })
}
