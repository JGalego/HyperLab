//! HyperLab's AI layer: what to say, what came back, and what to do about it.
//!
//! The other AI crates each hold one piece — [`hyperlab_ai`] the interface and
//! the message types, `hyperlab-ai-providers` the clients, [`hyperlab_mcp`]
//! the tools. This one is the conversation that uses them, and it is the only
//! place that knows what a prompt looks like.
//!
//! # Why a turn is in two halves
//!
//! Asking a model is slow and needs no stack. Running a tool is instant and
//! needs `&mut Runtime`. If they were one function it would hold the session
//! locked across a network request, and every other command — including the
//! dialog a script is waiting on — would queue behind it.
//!
//! So a caller drives the turn:
//!
//! ```text
//!   ┌──────────────────────────────────────────────────────┐
//!   │  lock:   Briefing::about, Conversation::ask          │
//!   │  unlock: provider.complete(...).await                │
//!   │  lock:   tools::run, Conversation::record_tool       │
//!   └────────────── repeat while it asks for tools ────────┘
//! ```
//!
//! [`Conversation`] holds the state between those steps, so neither half has
//! to know the other exists.
//!
//! # What this promises
//!
//! * **Nothing changes a stack except a command.** Tools go through
//!   [`hyperlab_mcp::ToolRegistry`], so an assistant's edit is undoable and
//!   indistinguishable from a person's.
//! * **Nothing runs that the user did not allow.** Every call crosses a
//!   [`Policy`](hyperlab_mcp::Policy) first.
//! * **Nothing leaves without being written down.** [`Briefing`] is both the
//!   thing that is sent and the thing that is shown, so the two cannot drift.
//!
//! ```
//! use hyperlab_ai::ContextOptions;
//! use hyperlab_assistant::{Briefing, Conversation};
//! use hyperlab_mcp::ToolRegistry;
//! use hyperlab_runtime::Runtime;
//! use hyperlab_stack::Stack;
//!
//! let runtime = Runtime::new(Stack::new("Notes"));
//! let mut conversation = Conversation::new();
//!
//! // What is about to be sent, recorded as it is assembled.
//! let briefing = Briefing::about(&runtime, ContextOptions::default());
//! assert!(briefing.context.contains("Notes"));
//! conversation.ask("What is on this card?", briefing);
//!
//! // Which is now a request, tools and all, ready for any provider.
//! let request = conversation.request("some-model", ToolRegistry::new().definitions());
//! assert!(!request.tools.is_empty());
//! ```

#![warn(missing_docs)]

mod briefing;
mod conversation;
pub mod tools;

pub use briefing::{Briefing, SYSTEM_PROMPT};
pub use conversation::{Conversation, Entry, MAX_ROUNDS};
pub use tools::{MAX_RESULT, ToolOutcome};
