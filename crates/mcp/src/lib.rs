//! HyperLab as a set of tools.
//!
//! The Model Context Protocol asks a question HyperLab already had an answer
//! for: *what can be done to this application, described well enough for
//! something else to do it?* HyperCard's answer was the message path and
//! HyperTalk. This crate is the same answer in a form a model can use.
//!
//! Every tool is a wrapper around a [`Command`](hyperlab_runtime::Command) or
//! a query, so:
//!
//! * an assistant can do exactly what a person can do, and nothing else;
//! * everything an assistant does is undoable;
//! * the UI updates the same way it does for any other change.
//!
//! There is deliberately no transport here — no stdio server, no sockets.
//! What a tool *is* and how it is *delivered* are separate problems, and only
//! the first one belongs next to the runtime.
//!
//! ```
//! use hyperlab_mcp::ToolRegistry;
//! use hyperlab_runtime::Runtime;
//! use hyperlab_stack::Stack;
//! use serde_json::json;
//!
//! let mut runtime = Runtime::new(Stack::new("Notes"));
//! let registry = ToolRegistry::new();
//!
//! let created = registry
//!     .call(&mut runtime, "create_field", &json!({ "name": "Title" }))
//!     .unwrap();
//! registry
//!     .call(
//!         &mut runtime,
//!         "write_field",
//!         &json!({ "name": "Title", "text": "Hello" }),
//!     )
//!     .unwrap();
//!
//! let read = registry
//!     .call(&mut runtime, "read_field", &json!({ "name": "Title" }))
//!     .unwrap();
//! assert_eq!(read["text"], "Hello");
//! assert!(created["id"].is_number());
//!
//! // And, because tools go through the command bus, it can be taken back.
//! runtime.undo().unwrap();
//! ```

#![warn(missing_docs)]

mod error;
mod registry;
mod tools;

pub use error::{ToolError, ToolResult};
pub use registry::ToolRegistry;
pub use tools::{TOOLS, Tool};
