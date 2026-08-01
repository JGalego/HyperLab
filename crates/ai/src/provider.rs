//! The interface every language model provider implements.

use std::{fmt, future::Future, pin::Pin};

use crate::message::{Completion, CompletionRequest, Embedding};

/// A future returned by a provider.
///
/// Spelled out rather than pulled in from a crate: this is the only piece of
/// async machinery HyperLab's AI layer needs, and one type alias is cheaper
/// than a dependency.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The result of talking to a provider.
pub type AiResult<T> = Result<T, AiError>;

/// Something went wrong talking to a model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiError {
    /// The provider has not been configured — usually a missing key.
    NotConfigured(String),
    /// The provider does not do this.
    Unsupported(String),
    /// The network, or the provider, said no.
    Transport(String),
    /// The provider answered with something unexpected.
    Protocol(String),
    /// The provider refused the request.
    Refused(String),
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConfigured(what) => write!(f, "this provider is not set up: {what}"),
            Self::Unsupported(what) => write!(f, "this provider cannot {what}"),
            Self::Transport(what) => write!(f, "could not reach the provider: {what}"),
            Self::Protocol(what) => write!(f, "the provider said something unexpected: {what}"),
            Self::Refused(what) => write!(f, "the provider refused: {what}"),
        }
    }
}

impl std::error::Error for AiError {}

/// What a provider can do.
///
/// The sidebar reads these to decide what to offer, rather than keeping its
/// own list of which providers support what.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// Whether it can be given tools.
    pub tools: bool,
    /// Whether it can produce embeddings.
    pub embeddings: bool,
    /// Whether it runs on this machine, which decides whether using it sends
    /// the user's stack anywhere.
    pub local: bool,
}

/// A source of completions.
///
/// Implementations live outside this crate — one per vendor — so that adding
/// a provider never means editing HyperLab's core. Nothing in the runtime,
/// the parser or the object model knows this trait exists.
pub trait AiProvider: Send + Sync {
    /// A short name, as the user would choose it: `"anthropic"`, `"ollama"`.
    fn name(&self) -> &str;

    /// What this provider can do.
    fn capabilities(&self) -> Capabilities;

    /// Asks for a completion.
    fn complete<'a>(&'a self, request: CompletionRequest) -> BoxFuture<'a, AiResult<Completion>>;

    /// Asks for embeddings. Providers that cannot say so rather than
    /// pretending.
    fn embed<'a>(&'a self, texts: Vec<String>) -> BoxFuture<'a, AiResult<Vec<Embedding>>> {
        let _ = texts;
        let name = self.name().to_string();
        Box::pin(async move { Err(AiError::Unsupported(format!("{name} cannot embed text"))) })
    }
}
