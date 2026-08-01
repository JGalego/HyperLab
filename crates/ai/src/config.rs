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
    /// The name of the environment variable holding the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
}

impl ProviderConfig {
    /// A configuration with only the essentials.
    pub fn new(kind: ProviderKind, model: impl Into<String>) -> Self {
        Self {
            kind,
            model: model.into(),
            base_url: None,
            api_key_env: None,
        }
    }

    /// Reads the API key from the environment, if one is named.
    ///
    /// Returns `None` when no variable is named or the variable is unset, so
    /// the caller can tell the user exactly what to set.
    #[must_use]
    pub fn api_key(&self) -> Option<String> {
        let name = self.api_key_env.as_ref()?;
        std::env::var(name).ok().filter(|key| !key.is_empty())
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

    #[test]
    fn settings_never_contain_a_key() {
        let mut settings = AiSettings::default();
        settings.providers.insert(
            "work".into(),
            ProviderConfig {
                kind: ProviderKind::Anthropic,
                model: "a-model".into(),
                base_url: None,
                api_key_env: Some("ANTHROPIC_API_KEY".into()),
            },
        );
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("apiKeyEnv"));
        assert!(
            !json.to_lowercase().contains("sk-"),
            "a settings file must never carry a secret"
        );
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
