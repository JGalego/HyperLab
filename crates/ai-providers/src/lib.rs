//! Providers you can actually talk to.
//!
//! [`hyperlab-ai`] defines what a provider *is* and deliberately contains no
//! HTTP client and no vendor knowledge. This crate is where that knowledge is
//! allowed to live, and it is the only crate in HyperLab that has any: the
//! runtime, the parser and the object model never learn that OpenAI or
//! Anthropic exist.
//!
//! Two clients cover most of the field:
//!
//! * [`OpenAiProvider`] speaks the chat-completions protocol, which OpenAI,
//!   OpenRouter, Ollama, LM Studio, llama.cpp and vLLM all accept. Point its
//!   `baseUrl` somewhere and it is a different provider.
//! * [`AnthropicProvider`] speaks Anthropic's Messages API.
//!
//! [`build`] turns a [`ProviderConfig`] into whichever one applies. It is the
//! single place in HyperLab that maps a name to an implementation, so adding
//! a provider means adding a module and one arm.
//!
//! ```
//! use hyperlab_ai::{ProviderConfig, ProviderKind};
//! use hyperlab_ai_providers::build;
//!
//! let mut config = ProviderConfig::new(ProviderKind::Anthropic, "claude-opus-5");
//! config.api_key_env = Some("ANTHROPIC_API_KEY".into());
//!
//! // Builds a provider, or explains what is missing. Nothing is sent yet.
//! match build("anthropic", &config) {
//!     Ok(provider) => assert_eq!(provider.name(), "anthropic"),
//!     Err(why) => println!("{why}"),
//! }
//! ```
//!
//! # Keys
//!
//! A key is read from the environment variable a [`ProviderConfig`] names,
//! and is never written anywhere. Naming no variable is allowed — a model
//! server on this machine has no use for a key — but naming one that is not
//! set is an error, reported when the provider is built rather than as a
//! puzzling refusal later.
//!
//! The environment is the simplest place to keep a key, not the best one.
//! [`OpenAiProvider::with_api_key`] and [`AnthropicProvider::with_api_key`]
//! take one directly, so an embedder that reads the operating system's
//! keychain never has to put it in a variable at all.
//!
//! [`hyperlab-ai`]: hyperlab_ai

#![warn(missing_docs)]

pub mod anthropic;
mod http;
pub mod openai;

use std::sync::Arc;

use hyperlab_ai::{AiError, AiProvider, AiResult, MockProvider, ProviderConfig, ProviderKind};

pub use anthropic::AnthropicProvider;
pub use openai::OpenAiProvider;

/// Builds the provider a configuration describes.
///
/// `name` is what the user calls it, which is how the rest of HyperLab looks
/// it up; the [`kind`](ProviderConfig::kind) decides which client to use.
///
/// # Errors
///
/// Returns [`AiError::NotConfigured`] if something the client needs is
/// missing, and [`AiError::Unsupported`] for a kind no client here speaks.
pub fn build(name: &str, config: &ProviderConfig) -> AiResult<Arc<dyn AiProvider>> {
    match config.kind {
        // Everything that speaks chat-completions is the same client with a
        // different address.
        ProviderKind::OpenAi => Ok(Arc::new(OpenAiProvider::new(name, config)?)),
        ProviderKind::OpenRouter | ProviderKind::Ollama | ProviderKind::OpenAiCompatible => {
            let config = with_required_base_url(config)?;
            Ok(Arc::new(OpenAiProvider::new(name, &config)?))
        }
        ProviderKind::Anthropic => Ok(Arc::new(AnthropicProvider::new(name, config)?)),
        ProviderKind::Mock => Ok(Arc::new(MockProvider::new(name))),
        ProviderKind::Google | ProviderKind::Local | ProviderKind::Other(_) => {
            Err(AiError::Unsupported(format!(
                "HyperLab has no client for \"{}\" yet; an OpenAI-compatible endpoint would work",
                config.kind.as_str()
            )))
        }
    }
}

/// Checks that a configuration says where to send requests.
///
/// A provider that is defined by its address — a local server, a gateway —
/// cannot have a sensible default, and guessing a port is worse than asking.
fn with_required_base_url(config: &ProviderConfig) -> AiResult<ProviderConfig> {
    if config.base_url.is_none() {
        return Err(AiError::NotConfigured(format!(
            "\"{}\" needs a baseUrl, such as http://localhost:11434/v1",
            config.kind.as_str()
        )));
    }
    Ok(config.clone())
}

/// The API key a configuration names, if it names one.
///
/// # Errors
///
/// Returns [`AiError::NotConfigured`] if a variable is named but unset, which
/// is nearly always a typo in the variable's name or a shell that did not
/// export it.
fn configured_key(config: &ProviderConfig) -> AiResult<Option<String>> {
    let Some(variable) = &config.api_key_env else {
        return Ok(None);
    };
    config.api_key().map(Some).ok_or_else(|| {
        AiError::NotConfigured(format!("the environment variable {variable} is not set"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chat_completions_kinds_all_build_the_same_client() {
        for kind in [
            ProviderKind::OpenRouter,
            ProviderKind::Ollama,
            ProviderKind::OpenAiCompatible,
        ] {
            let mut config = ProviderConfig::new(kind.clone(), "a-model");
            config.base_url = Some("http://localhost:1234/v1".into());
            let provider = build("mine", &config).expect("a base URL is all it needs");
            assert_eq!(provider.name(), "mine");
            assert!(provider.capabilities().tools, "{kind:?}");
        }
    }

    #[test]
    fn openai_and_anthropic_know_their_own_addresses() {
        for kind in [ProviderKind::OpenAi, ProviderKind::Anthropic] {
            let config = ProviderConfig::new(kind, "a-model");
            assert!(build("mine", &config).is_ok());
        }
    }

    #[test]
    fn a_provider_that_is_only_an_address_must_be_given_one() {
        let config = ProviderConfig::new(ProviderKind::Ollama, "llama");
        let error = build("ollama", &config)
            .err()
            .expect("no address, no client");
        assert!(matches!(error, AiError::NotConfigured(_)), "{error}");
    }

    #[test]
    fn a_kind_with_no_client_says_so_and_suggests_a_way_round_it() {
        let config = ProviderConfig::new(ProviderKind::Google, "a-model");
        let Some(AiError::Unsupported(message)) = build("google", &config).err() else {
            panic!("there is no Google client");
        };
        assert!(message.contains("google") && message.contains("OpenAI-compatible"));
    }

    #[test]
    fn the_mock_is_reachable_the_same_way_as_the_rest() {
        let config = ProviderConfig::new(ProviderKind::Mock, "any");
        assert_eq!(build("offline", &config).unwrap().name(), "offline");
    }

    #[test]
    fn a_key_is_read_from_the_environment_only_when_one_is_named() {
        let mut config = ProviderConfig::new(ProviderKind::OpenAi, "a-model");
        assert_eq!(configured_key(&config).unwrap(), None);

        config.api_key_env = Some("HYPERLAB_TEST_KEY_THAT_IS_NOT_SET".into());
        let error = configured_key(&config).unwrap_err();
        assert!(
            format!("{error}").contains("HYPERLAB_TEST_KEY_THAT_IS_NOT_SET"),
            "the message must name the variable to set: {error}"
        );
    }
}
