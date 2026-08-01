//! Choosing and configuring providers.

use serde::{Deserialize, Serialize};

/// The provider families HyperLab expects to meet.
///
/// This list exists so that settings files and the UI have stable names to
/// use. It is *not* a list of what is implemented, and nothing in the runtime
/// switches on it: a provider is reached through [`AiProvider`], never
/// through this enum.
///
/// [`AiProvider`]: crate::AiProvider
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    /// OpenAI's hosted models.
    OpenAi,
    /// Anthropic's hosted models.
    Anthropic,
    /// Google's hosted models.
    Google,
    /// A local Ollama server.
    Ollama,
    /// OpenRouter, which fronts many providers.
    OpenRouter,
    /// Any other OpenAI-compatible endpoint, including local servers.
    OpenAiCompatible,
    /// A model running in this process.
    Local,
    /// The built-in [`MockProvider`](crate::MockProvider), for tests and for
    /// trying things out with no network at all.
    Mock,
    /// Something HyperLab has not heard of, named by a plugin.
    Other(String),
}

impl ProviderKind {
    /// The name used in settings files.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
            Self::Google => "google",
            Self::Ollama => "ollama",
            Self::OpenRouter => "openrouter",
            Self::OpenAiCompatible => "openai-compatible",
            Self::Local => "local",
            Self::Mock => "mock",
            Self::Other(name) => name,
        }
    }

    /// Whether using this provider keeps the user's stack on their machine.
    ///
    /// HyperLab is local-first, so the UI says out loud when a request is
    /// about to leave the building.
    #[must_use]
    pub const fn is_local(&self) -> bool {
        matches!(self, Self::Ollama | Self::Local | Self::Mock)
    }
}

/// Where a provider's key is kept.
///
/// A directory, never a value. Both arms name a place to go and look, which
/// is why a settings file holding one can still be copied into a bug report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "in", content = "name")]
pub enum KeySource {
    /// An environment variable, named here.
    ///
    /// The simplest place to keep a key: it survives nothing, which on a
    /// shared machine is the point.
    Environment(String),
    /// The operating system's keychain, under the provider's own name.
    ///
    /// Nothing in this crate can open one — see [`Keychain`].
    Keychain,
}

/// Somewhere the operating system keeps secrets.
///
/// The trait is here because [`ProviderConfig`] has to be able to point at a
/// keychain; every implementation is somewhere else, because opening one
/// means platform code and this crate has none.
pub trait Keychain: Send + Sync {
    /// The key saved for `provider`, if there is one.
    fn key(&self, provider: &str) -> Option<String>;
}

/// A keychain with nothing in it.
///
/// For tests, for hosts that have no keychain, and for callers that only ever
/// configure providers from the environment.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoKeychain;

impl Keychain for NoKeychain {
    fn key(&self, _provider: &str) -> Option<String> {
        None
    }
}

/// How to reach one provider.
///
/// There is no field for an API key, and there never should be: a key belongs
/// in the operating system's keychain or in an environment variable, not in a
/// settings file that gets copied into a bug report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    /// Which family.
    pub kind: ProviderKind,
    /// The model to use by default.
    pub model: String,
    /// Where to send requests. `None` means the provider's usual endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Where to find the key. `None` for a provider that needs none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<KeySource>,
}

impl ProviderConfig {
    /// A configuration with only the essentials.
    pub fn new(kind: ProviderKind, model: impl Into<String>) -> Self {
        Self {
            kind,
            model: model.into(),
            base_url: None,
            key: None,
        }
    }

    /// Names an environment variable as the place to find the key.
    #[must_use]
    pub fn from_environment(mut self, variable: impl Into<String>) -> Self {
        self.key = Some(KeySource::Environment(variable.into()));
        self
    }

    /// Fetches the API key from wherever the configuration says it is.
    ///
    /// `provider` is the name the user gave this provider, which is what a
    /// keychain files the key under. Returns `None` when no place is named,
    /// and when the named place is empty — the caller can tell those apart by
    /// looking at [`key`](Self::key), and says which so the user knows what to
    /// go and fix.
    #[must_use]
    pub fn api_key(&self, provider: &str, keychain: &dyn Keychain) -> Option<String> {
        match self.key.as_ref()? {
            KeySource::Environment(variable) => std::env::var(variable).ok(),
            KeySource::Keychain => keychain.key(provider),
        }
        .filter(|key| !key.is_empty())
    }
}

/// Every provider the user has set up, and which one to use by default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettings {
    /// The name of the provider to use when nothing says otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_provider: Option<String>,
    /// Configurations, by the name the user gave them.
    #[serde(default)]
    pub providers: std::collections::BTreeMap<String, ProviderConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A keychain holding one key, for the tests below.
    struct OneKey(&'static str, &'static str);

    impl Keychain for OneKey {
        fn key(&self, provider: &str) -> Option<String> {
            (provider == self.0).then(|| self.1.to_string())
        }
    }

    #[test]
    fn settings_never_contain_a_key() {
        let mut settings = AiSettings::default();
        settings.providers.insert(
            "work".into(),
            ProviderConfig::new(ProviderKind::Anthropic, "a-model")
                .from_environment("ANTHROPIC_API_KEY"),
        );
        settings.providers.insert(
            "home".into(),
            ProviderConfig {
                key: Some(KeySource::Keychain),
                ..ProviderConfig::new(ProviderKind::OpenAi, "a-model")
            },
        );

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains(r#"{"in":"environment","name":"ANTHROPIC_API_KEY"}"#));
        // The keychain arm carries nothing but the word: no value, and no
        // place to put one.
        assert!(json.contains(r#"{"in":"keychain"}"#));
        assert!(
            !json.to_lowercase().contains("sk-"),
            "a settings file must never carry a secret"
        );
    }

    #[test]
    fn a_key_comes_from_wherever_the_config_points() {
        let keychain = OneKey("home", "from-the-keychain");

        let from_env = ProviderConfig::new(ProviderKind::OpenAi, "m")
            .from_environment("HYPERLAB_TEST_KEY_THAT_IS_NOT_SET");
        assert_eq!(from_env.api_key("work", &keychain), None);

        let from_keychain = ProviderConfig {
            key: Some(KeySource::Keychain),
            ..ProviderConfig::new(ProviderKind::OpenAi, "m")
        };
        assert_eq!(
            from_keychain.api_key("home", &keychain).as_deref(),
            Some("from-the-keychain")
        );
        // Filed under the provider's name, so another provider's key is not
        // quietly used instead.
        assert_eq!(from_keychain.api_key("work", &keychain), None);
    }

    #[test]
    fn a_provider_that_names_nowhere_asks_nowhere() {
        let config = ProviderConfig::new(ProviderKind::Ollama, "llama");
        assert_eq!(config.api_key("local", &OneKey("local", "unused")), None);
    }

    #[test]
    fn local_providers_are_marked_as_such() {
        assert!(ProviderKind::Ollama.is_local());
        assert!(!ProviderKind::OpenAi.is_local());
    }

    #[test]
    fn unknown_providers_keep_their_name() {
        let kind = ProviderKind::Other("my-plugin".into());
        assert_eq!(kind.as_str(), "my-plugin");
    }
}
