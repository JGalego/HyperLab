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
//! What a tool *is* and how it is *delivered* stay separate problems.
//! [`ToolRegistry`] is the answer to the first and needs no transport at all;
//! [`Server`] is the answer to the second, speaking MCP over any pair of
//! streams, and [`Client`] is the same protocol pointed the other way, so a
//! stack can reach tools HyperLab did not write. Between the two sits
//! [`Policy`], which is the only thing that decides whether a call runs.
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

// Driving somebody else's MCP server means spawning a program, which a
// browser cannot do. The rest of this crate — the tool table, the policy —
// is what a page needs, and compiles for it.
#[cfg(not(target_arch = "wasm32"))]
mod client;
mod error;
pub mod jsonrpc;
mod permission;
mod registry;
mod server;
mod tools;

#[cfg(not(target_arch = "wasm32"))]
pub use client::{Client, ExternalTool, Launch, MAX_LINE, PATIENCE, SHUTDOWN_GRACE};
pub use error::{ToolError, ToolResult};
pub use permission::{
    Access, AllowAll, Approval, Approver, Consent, Decision, DenyAll, Policy, Verdict,
};
pub use registry::ToolRegistry;
pub use server::{PROTOCOL_VERSION, Server, ServerInfo, serve_stdio, serve_stdio_unattended};
pub use tools::{TOOLS, Tool};
