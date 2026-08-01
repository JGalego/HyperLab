//! The HyperLab runtime.
//!
//! The runtime owns the one mutable copy of a stack and is the only way to
//! change it. Everything that edits a stack — the user interface, a
//! HyperTalk script, an MCP tool, a future AI assistant — goes through the
//! same two doors:
//!
//! * [`Runtime::execute`], which applies a [`Command`] and records its
//!   inverse for undo, and
//! * [`Runtime::send_message`], which sends a [`Message`] along the message
//!   path so scripts can respond.
//!
//! ```
//! use hyperlab_runtime::{Command, Message, PartOwner, Runtime};
//! use hyperlab_stack::{Object, ObjectId, ObjectKind, PartKind, Rect, Stack};
//!
//! let mut runtime = Runtime::new(Stack::new("Hello"));
//! let card = runtime.current_card();
//!
//! // Add a button, exactly as the editor would.
//! let button = runtime
//!     .execute(Command::CreatePart {
//!         owner: PartOwner::Card { id: card },
//!         kind: PartKind::Button,
//!         name: "Greet".into(),
//!         geometry: Rect::new(20, 20, 96, 24),
//!     })?
//!     .unwrap();
//!
//! // Give it a script, and click it.
//! runtime.execute(Command::SetScript {
//!     object: button,
//!     script: "on mouseUp\n  answer \"Hello\"\nend mouseUp".into(),
//! })?;
//! runtime.send_message(&Message::new("mouseUp"), button)?;
//!
//! assert_eq!(
//!     runtime.take_effects(),
//!     vec![hyperlab_runtime::Effect::Answer { message: "Hello".into() }]
//! );
//! # Ok::<(), hyperlab_runtime::RuntimeError>(())
//! ```

#![warn(missing_docs)]

pub mod chunk;
mod command;
mod error;
pub mod event;
mod history;
mod host;
mod interpreter;
mod runtime;

pub use command::{Applied, Command, PartOwner};
pub use error::{RuntimeError, RuntimeResult};
pub use event::{Message, message_path, messages};
pub use history::History;
pub use host::{AiIntent, AiRequest, Effect, Host, SilentHost};
pub use runtime::Runtime;
