//! Reaching a language model from a page, with the page's own transport.
//!
//! The native build turns a [`ProviderConfig`](hyperlab_ai::ProviderConfig)
//! into a client that owns an HTTP stack. A browser already has one —
//! `fetch` — and it is the only one a page may use, so here a
//! configuration resolves to a [`WebProvider`]:
//! the address to post to, the headers to send, and the wire protocol to
//! speak, with the actual sending left to JavaScript.
//!
//! Two promises this module keeps, and where:
//!
//! * **A key goes one way.** It is read through the lookup the caller
//!   passes in — browser storage, behind the same one-way interface as the
//!   desktop's keychain — lands in a header, and is never printed:
//!   [`WebProvider`] has no `Debug`, no `Serialize`, and no method that
//!   returns the key.
//! * **A request goes to the provider and nowhere else.** The URL is the
//!   configured provider's; the page that served HyperLab never sees it. A
//!   static host has no server worth the name, and this keeps it that way.

use hyperlab_ai::{
    AiSettings, ChatMessage, Completion, CompletionRequest, FinishReason, KeySource, ProviderKind,
    Role,
};
use hyperlab_ai_providers::{anthropic, openai};
use serde_json::Value;

/// Which shape of request a provider expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wire {
    /// Anthropic's Messages API.
    Anthropic,
    /// The OpenAI chat-completions protocol, which OpenRouter, Ollama and
    /// every "compatible" server also speak.
    OpenAiChat,
    /// The built-in mock: answered locally, nothing sent anywhere.
    Mock,
}

/// One configured provider, resolved and ready to be asked.
pub struct WebProvider {
    /// The name the user gave it.
    pub name: String,
    /// The protocol it speaks.
    pub wire: Wire,
    /// Where requests go, without a trailing slash.
    pub base_url: String,
    /// The model to ask for when a request does not name one.
    pub model: String,
    /// Whether asking it keeps the stack on this machine.
    pub local: bool,
    key: Option<String>,
}

impl std::fmt::Debug for WebProvider {
    /// Written by hand so the key cannot end up in a log: it says whether
    /// one is held, never what it is.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebProvider")
            .field("name", &self.name)
            .field("wire", &self.wire)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("local", &self.local)
            .field("key", &self.key.as_ref().map(|_| "…"))
            .finish()
    }
}

impl WebProvider {
    /// The URL a completion is posted to.
    #[must_use]
    pub fn completion_url(&self) -> String {
        match self.wire {
            Wire::Anthropic => format!("{}/messages", self.base_url),
            Wire::OpenAiChat => format!("{}/chat/completions", self.base_url),
            Wire::Mock => String::new(),
        }
    }

    /// The headers a completion is posted with. The key lands here and
    /// nowhere else.
    ///
    /// `anthropic-dangerous-direct-browser-access` is the header Anthropic
    /// requires before answering a browser at all. It is the point of this
    /// build rather than a workaround: the request really does come straight
    /// from the user's browser, with a key the user typed into it, which is
    /// exactly the situation the header exists to make explicit.
    #[must_use]
    pub fn headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
        match self.wire {
            Wire::Anthropic => {
                headers.push((
                    "anthropic-version".to_string(),
                    anthropic::API_VERSION.to_string(),
                ));
                headers.push((
                    "anthropic-dangerous-direct-browser-access".to_string(),
                    "true".to_string(),
                ));
                if let Some(key) = &self.key {
                    headers.push(("x-api-key".to_string(), key.clone()));
                }
            }
            Wire::OpenAiChat => {
                if let Some(key) = &self.key {
                    headers.push(("authorization".to_string(), format!("Bearer {key}")));
                }
            }
            Wire::Mock => {}
        }
        headers
    }

    /// The body of a completion request, in the provider's own shape.
    #[must_use]
    pub fn completion_body(&self, request: &CompletionRequest) -> Value {
        match self.wire {
            Wire::Anthropic => anthropic::completion_body(&self.model, request),
            Wire::OpenAiChat | Wire::Mock => openai::completion_body(&self.model, request),
        }
    }

    /// Reads what came back, or says what went wrong in a sentence.
    ///
    /// # Errors
    ///
    /// Returns a message for the user: the provider's own words when the
    /// reply carries any, and the status when it does not.
    pub fn decode_completion(&self, status: u16, body: &str) -> Result<Completion, String> {
        // A failed request may answer with prose — an HTML error page from a
        // proxy, say — so the body is only parsed as a courtesy.
        let parsed = serde_json::from_str::<Value>(body);
        if !(200..300).contains(&status) {
            let described = parsed.as_ref().ok().and_then(|reply| match self.wire {
                Wire::Anthropic => anthropic::describe_error(reply),
                Wire::OpenAiChat | Wire::Mock => openai::describe_error(reply),
            });
            return Err(match described {
                Some(message) => message,
                None => format!("the provider answered {status}"),
            });
        }

        let reply = parsed.map_err(|error| format!("the reply was not JSON: {error}"))?;
        match self.wire {
            Wire::Anthropic => anthropic::decode_completion(&reply),
            Wire::OpenAiChat | Wire::Mock => openai::decode_completion(&reply),
        }
        .map_err(|error| error.to_string())
    }
}

/// What the mock answers, matching `MockProvider` in `hyperlab-ai`: the last
/// thing it was told, echoed, so the plumbing can be watched with no key and
/// no network.
#[must_use]
pub fn mock_completion(request: &CompletionRequest) -> Completion {
    let last = request
        .messages
        .iter()
        .rev()
        .find(|message| message.role == Role::User)
        .map_or_else(String::new, |message: &ChatMessage| message.content.clone());
    Completion {
        content: format!("You said: {last}"),
        tool_calls: Vec::new(),
        finish_reason: FinishReason::Stop,
        usage: None,
    }
}

/// Resolves one configuration, going to `stored_key` for a key kept in the
/// browser.
///
/// The arms mirror `hyperlab_ai_providers::build` deliberately, so a
/// settings file means the same thing on both shells — with one exception a
/// page cannot avoid: an environment variable does not exist here, and the
/// error says where the key can live instead.
///
/// # Errors
///
/// Returns a sentence naming what is missing or unsupported.
pub fn resolve(
    name: &str,
    config: &hyperlab_ai::ProviderConfig,
    stored_key: &dyn Fn(&str) -> Option<String>,
) -> Result<WebProvider, String> {
    let key = match &config.key {
        None => None,
        Some(KeySource::Keychain) => Some(stored_key(name).ok_or_else(|| {
            "no key is saved for this provider in this browser yet — type one into AI ▸ Settings"
                .to_string()
        })?),
        Some(KeySource::Environment(variable)) => {
            return Err(format!(
                "a page has no environment variables, so {variable} cannot be read — \
                 keep the key in this browser instead"
            ));
        }
    };

    let (wire, default_base_url) = match &config.kind {
        ProviderKind::OpenAi => (Wire::OpenAiChat, Some(openai::DEFAULT_BASE_URL)),
        ProviderKind::Anthropic => (Wire::Anthropic, Some(anthropic::DEFAULT_BASE_URL)),
        ProviderKind::OpenRouter | ProviderKind::Ollama | ProviderKind::OpenAiCompatible => {
            (Wire::OpenAiChat, None)
        }
        ProviderKind::Mock => (Wire::Mock, Some("")),
        ProviderKind::Google | ProviderKind::Local | ProviderKind::Other(_) => {
            return Err(format!(
                "HyperLab has no client for \"{}\" yet; an OpenAI-compatible endpoint would work",
                config.kind.as_str()
            ));
        }
    };

    let base_url = match (&config.base_url, default_base_url) {
        (Some(url), _) => url.trim_end_matches('/').to_string(),
        (None, Some(url)) => url.trim_end_matches('/').to_string(),
        (None, None) => {
            return Err(format!(
                "\"{}\" needs a baseUrl, such as http://localhost:11434/v1",
                config.kind.as_str()
            ));
        }
    };

    Ok(WebProvider {
        name: name.to_string(),
        wire,
        base_url,
        model: config.model.clone(),
        local: config.kind.is_local(),
        key,
    })
}

/// Resolves every provider the settings describe.
///
/// One that cannot be resolved is left out and the reason collected, rather
/// than failing the lot — the same manners as the desktop's settings loader.
#[must_use]
pub fn resolve_all(
    settings: &AiSettings,
    stored_key: &dyn Fn(&str) -> Option<String>,
) -> (Vec<WebProvider>, Vec<String>) {
    let mut providers = Vec::new();
    let mut problems = Vec::new();
    for (name, config) in &settings.providers {
        match resolve(name, config, stored_key) {
            Ok(provider) => providers.push(provider),
            Err(reason) => problems.push(format!("{name}: {reason}")),
        }
    }
    (providers, problems)
}

#[cfg(test)]
mod tests {
    use hyperlab_ai::ProviderConfig;

    use super::*;

    fn no_key(_: &str) -> Option<String> {
        None
    }

    fn a_key(_: &str) -> Option<String> {
        Some("sk-test".into())
    }

    #[test]
    fn anthropic_resolves_to_its_own_protocol_and_address() {
        let config = ProviderConfig {
            key: Some(KeySource::Keychain),
            ..ProviderConfig::new(ProviderKind::Anthropic, "a-model")
        };
        let provider = resolve("work", &config, &a_key).unwrap();
        assert_eq!(provider.wire, Wire::Anthropic);
        assert_eq!(
            provider.completion_url(),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn a_browser_request_to_anthropic_says_out_loud_that_it_is_one() {
        let config = ProviderConfig {
            key: Some(KeySource::Keychain),
            ..ProviderConfig::new(ProviderKind::Anthropic, "a-model")
        };
        let headers = resolve("work", &config, &a_key).unwrap().headers();
        assert!(headers.iter().any(|(name, value)| name
            == "anthropic-dangerous-direct-browser-access"
            && value == "true"));
        assert!(
            headers
                .iter()
                .any(|(name, value)| name == "x-api-key" && value == "sk-test")
        );
    }

    #[test]
    fn the_chat_completions_kinds_need_an_address_and_get_bearer_auth() {
        let mut config = ProviderConfig {
            key: Some(KeySource::Keychain),
            ..ProviderConfig::new(ProviderKind::Ollama, "llama")
        };
        assert!(resolve("local", &config, &a_key).is_err(), "no baseUrl yet");

        config.base_url = Some("http://localhost:11434/v1/".into());
        let provider = resolve("local", &config, &a_key).unwrap();
        assert_eq!(provider.wire, Wire::OpenAiChat);
        assert_eq!(
            provider.completion_url(),
            "http://localhost:11434/v1/chat/completions"
        );
        assert!(
            provider
                .headers()
                .iter()
                .any(|(name, value)| name == "authorization" && value == "Bearer sk-test")
        );
    }

    #[test]
    fn an_environment_key_source_is_refused_with_directions() {
        let config =
            ProviderConfig::new(ProviderKind::OpenAi, "a-model").from_environment("OPENAI_API_KEY");
        let reason = resolve("work", &config, &a_key).unwrap_err();
        assert!(reason.contains("OPENAI_API_KEY"), "{reason}");
        assert!(reason.contains("browser"), "{reason}");
    }

    #[test]
    fn a_missing_stored_key_is_a_sentence_not_a_request_without_credentials() {
        let config = ProviderConfig {
            key: Some(KeySource::Keychain),
            ..ProviderConfig::new(ProviderKind::Anthropic, "a-model")
        };
        let reason = resolve("work", &config, &no_key).unwrap_err();
        assert!(reason.contains("Settings"), "{reason}");
    }

    #[test]
    fn one_broken_provider_does_not_take_the_others_with_it() {
        let mut settings = AiSettings::default();
        settings.providers.insert(
            "offline".into(),
            ProviderConfig::new(ProviderKind::Mock, "any"),
        );
        settings.providers.insert(
            "broken".into(),
            ProviderConfig::new(ProviderKind::Ollama, "llama"),
        );

        let (providers, problems) = resolve_all(&settings, &no_key);
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].name, "offline");
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("broken"), "{problems:?}");
    }

    #[test]
    fn the_mock_answers_locally_and_echoes() {
        let request = CompletionRequest::new(
            "any",
            vec![
                hyperlab_ai::ChatMessage::system("be brief"),
                hyperlab_ai::ChatMessage::user("hello"),
            ],
        );
        let completion = mock_completion(&request);
        assert_eq!(completion.content, "You said: hello");
        assert_eq!(completion.finish_reason, FinishReason::Stop);
    }

    #[test]
    fn an_error_reply_is_reported_in_the_providers_own_words() {
        let config = ProviderConfig::new(ProviderKind::OpenAi, "a-model");
        let provider = resolve("work", &config, &no_key).unwrap();

        let said = provider
            .decode_completion(401, r#"{"error": {"message": "bad key"}}"#)
            .unwrap_err();
        assert_eq!(said, "bad key");

        let unsaid = provider
            .decode_completion(502, "<html>bad gateway</html>")
            .unwrap_err();
        assert!(unsaid.contains("502"), "{unsaid}");
    }
}
