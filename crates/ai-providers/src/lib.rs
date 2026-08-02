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
//! # #[cfg(feature = "native")] {
//! use hyperlab_ai::{NoKeychain, ProviderConfig, ProviderKind};
//! use hyperlab_ai_providers::build;
//!
//! let config = ProviderConfig::new(ProviderKind::Anthropic, "claude-opus-5")
//!     .from_environment("ANTHROPIC_API_KEY");
//!
//! // Builds a provider, or explains what is missing. Nothing is sent yet.
//! match build("anthropic", &config, &NoKeychain) {
//!     Ok(provider) => assert_eq!(provider.name(), "anthropic"),
//!     Err(why) => println!("{why}"),
//! }
//! # }
//! ```
//!
//! # Keys
//!
//! A [`ProviderConfig`] names the place its key is kept — an environment
//! variable, or the operating system's keychain — and [`build`] goes and
//! looks. Naming nowhere is allowed, because a model server on this machine
//! has no use for a key. Naming somewhere and finding it empty is an error,
//! reported while the provider is being built rather than as a puzzling
//! refusal later.
//!
//! Nothing here opens a keychain: the caller passes one in, and
//! [`NoKeychain`] means there is none. A key is never written anywhere by
//! this crate, and never appears in an error message.
//!
//! [`hyperlab-ai`]: hyperlab_ai
//! [`NoKeychain`]: hyperlab_ai::NoKeychain

#![warn(missing_docs)]

pub mod anthropic;
#[cfg(feature = "native")]
mod http;
pub mod openai;
mod text;

#[cfg(feature = "native")]
use std::sync::Arc;

#[cfg(feature = "native")]
use hyperlab_ai::{
    AiError, AiProvider, AiResult, KeySource, Keychain, MockProvider, ProviderConfig, ProviderKind,
};

#[cfg(feature = "native")]
pub use anthropic::AnthropicProvider;
#[cfg(feature = "native")]
pub use openai::OpenAiProvider;

/// Builds the provider a configuration describes.
///
/// `name` is what the user calls it, which is how the rest of HyperLab looks
/// it up and how a keychain files its key; the [`kind`](ProviderConfig::kind)
/// decides which client to use.
///
/// # Errors
///
/// Returns [`AiError::NotConfigured`] if something the client needs is
/// missing, and [`AiError::Unsupported`] for a kind no client here speaks.
#[cfg(feature = "native")]
pub fn build(
    name: &str,
    config: &ProviderConfig,
    keychain: &dyn Keychain,
) -> AiResult<Arc<dyn AiProvider>> {
    let key = configured_key(name, config, keychain)?;
    match config.kind {
        // Everything that speaks chat-completions is the same client with a
        // different address.
        ProviderKind::OpenAi => Ok(Arc::new(OpenAiProvider::with_api_key(name, config, key)?)),
        ProviderKind::OpenRouter | ProviderKind::Ollama | ProviderKind::OpenAiCompatible => {
            let config = with_required_base_url(config)?;
            Ok(Arc::new(OpenAiProvider::with_api_key(name, &config, key)?))
        }
        ProviderKind::Anthropic => Ok(Arc::new(AnthropicProvider::with_api_key(
            name, config, key,
        )?)),
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
#[cfg(feature = "native")]
fn with_required_base_url(config: &ProviderConfig) -> AiResult<ProviderConfig> {
    if config.base_url.is_none() {
        return Err(AiError::NotConfigured(format!(
            "\"{}\" needs a baseUrl, such as http://localhost:11434/v1",
            config.kind.as_str()
        )));
    }
    Ok(config.clone())
}

/// Goes and gets the key, if the configuration says where one is.
///
/// # Errors
///
/// Returns [`AiError::NotConfigured`] naming the empty place. An unset
/// variable is nearly always a typo or a shell that did not export it; an
/// empty keychain entry means nobody has typed the key in yet.
#[cfg(feature = "native")]
fn configured_key(
    name: &str,
    config: &ProviderConfig,
    keychain: &dyn Keychain,
) -> AiResult<Option<String>> {
    let Some(source) = &config.key else {
        return Ok(None);
    };
    config
        .api_key(name, keychain)
        .map(Some)
        .ok_or_else(|| AiError::NotConfigured(empty(source)))
}

/// What to say about a place that was named and found empty.
#[cfg(feature = "native")]
fn empty(source: &KeySource) -> String {
    match source {
        KeySource::Environment(variable) => {
            format!("the environment variable {variable} is not set")
        }
        KeySource::Keychain => "there is no key for this provider in the keychain".to_string(),
    }
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use hyperlab_ai::NoKeychain;

    use super::*;

    /// A keychain holding one key, for the tests below.
    struct Holding(&'static str);

    impl Keychain for Holding {
        fn key(&self, provider: &str) -> Option<String> {
            (provider == self.0).then(|| "a-key".to_string())
        }
    }

    #[test]
    fn the_chat_completions_kinds_all_build_the_same_client() {
        for kind in [
            ProviderKind::OpenRouter,
            ProviderKind::Ollama,
            ProviderKind::OpenAiCompatible,
        ] {
            let mut config = ProviderConfig::new(kind.clone(), "a-model");
            config.base_url = Some("http://localhost:1234/v1".into());
            let provider = build("mine", &config, &NoKeychain).expect("a base URL is all it needs");
            assert_eq!(provider.name(), "mine");
            assert!(provider.capabilities().tools, "{kind:?}");
        }
    }

    #[test]
    fn openai_and_anthropic_know_their_own_addresses() {
        for kind in [ProviderKind::OpenAi, ProviderKind::Anthropic] {
            let config = ProviderConfig::new(kind, "a-model");
            assert!(build("mine", &config, &NoKeychain).is_ok());
        }
    }

    #[test]
    fn a_provider_that_is_only_an_address_must_be_given_one() {
        let config = ProviderConfig::new(ProviderKind::Ollama, "llama");
        let error = build("ollama", &config, &NoKeychain)
            .err()
            .expect("no address, no client");
        assert!(matches!(error, AiError::NotConfigured(_)), "{error}");
    }

    #[test]
    fn a_kind_with_no_client_says_so_and_suggests_a_way_round_it() {
        let config = ProviderConfig::new(ProviderKind::Google, "a-model");
        let Some(AiError::Unsupported(message)) = build("google", &config, &NoKeychain).err()
        else {
            panic!("there is no Google client");
        };
        assert!(message.contains("google") && message.contains("OpenAI-compatible"));
    }

    #[test]
    fn the_mock_is_reachable_the_same_way_as_the_rest() {
        let config = ProviderConfig::new(ProviderKind::Mock, "any");
        assert_eq!(
            build("offline", &config, &NoKeychain).unwrap().name(),
            "offline"
        );
    }

    #[test]
    fn a_key_is_fetched_only_when_the_config_says_where_one_is() {
        let config = ProviderConfig::new(ProviderKind::OpenAi, "a-model");
        assert_eq!(configured_key("mine", &config, &NoKeychain).unwrap(), None);
    }

    #[test]
    fn a_named_environment_variable_that_is_unset_names_itself_in_the_error() {
        let config = ProviderConfig::new(ProviderKind::OpenAi, "a-model")
            .from_environment("HYPERLAB_TEST_KEY_THAT_IS_NOT_SET");
        let error = configured_key("mine", &config, &NoKeychain).unwrap_err();
        assert!(
            format!("{error}").contains("HYPERLAB_TEST_KEY_THAT_IS_NOT_SET"),
            "the message must name the variable to set: {error}"
        );
    }

    #[test]
    fn a_key_can_come_from_the_keychain_instead() {
        let config = ProviderConfig {
            key: Some(KeySource::Keychain),
            ..ProviderConfig::new(ProviderKind::Anthropic, "a-model")
        };

        assert_eq!(
            configured_key("work", &config, &Holding("work")).unwrap(),
            Some("a-key".to_string())
        );

        // Nothing saved yet is a thing to say, not a request sent without
        // credentials that fails later with someone else's error message.
        let error = configured_key("work", &config, &NoKeychain).unwrap_err();
        assert!(format!("{error}").contains("keychain"), "{error}");
    }

    #[test]
    fn the_key_never_appears_in_the_error() {
        let config = ProviderConfig {
            key: Some(KeySource::Keychain),
            ..ProviderConfig::new(ProviderKind::Anthropic, "a-model")
        };
        // A key that is present but a base URL that is not: the provider
        // fails to build with the key in hand, which is the moment a careless
        // message would leak it.
        let mut broken = config.clone();
        broken.kind = ProviderKind::OpenAiCompatible;
        let Err(error) = build("work", &broken, &Holding("work")) else {
            panic!("no base URL, no client");
        };
        assert!(!format!("{error}").contains("a-key"), "{error}");
    }
}
