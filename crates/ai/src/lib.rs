//! Language models, without picking one.
//!
//! This crate contains interfaces and one mock implementation. It contains no
//! HTTP client, no vendor SDK and no API keys, and it never will: a provider
//! is a thing that implements [`AiProvider`], and HyperLab's core must work
//! identically whichever one is plugged in — including none at all.
//!
//! Three rules keep it that way.
//!
//! 1. **No provider is special.** Nothing switches on
//!    [`ProviderKind`]; it exists only so that settings files have stable
//!    names.
//! 2. **No secrets live here.** A [`ProviderConfig`] names the environment
//!    variable holding a key. It never holds the key.
//! 3. **The runtime does not know about any of this.** Context is built by
//!    reading the object model ([`context`]), and changes are made by
//!    calling MCP tools — never by reaching into a stack.
//!
//! ```
//! use hyperlab_ai::{AiProvider, ChatMessage, CompletionRequest, MockProvider};
//!
//! let provider = MockProvider::new("mock");
//! let request = CompletionRequest::new("any-model", vec![ChatMessage::user("hello")]);
//! // `complete` returns a future; a real caller awaits it.
//! let _future = provider.complete(request);
//! ```

#![warn(missing_docs)]

mod config;
pub mod context;
mod message;
mod mock;
mod provider;
mod registry;
mod tool;

pub use config::{AiSettings, ProviderConfig, ProviderKind};
pub use context::{ContextOptions, describe_card, describe_stack_outline};
pub use message::{
    ChatMessage, Completion, CompletionRequest, Embedding, FinishReason, Role, ToolCall, Usage,
};
pub use mock::MockProvider;
pub use provider::{AiError, AiProvider, AiResult, BoxFuture, Capabilities};
pub use registry::ProviderRegistry;
pub use tool::ToolDefinition;
