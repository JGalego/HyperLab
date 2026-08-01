//! Where the AI settings are kept, and what is deliberately not in them.
//!
//! One JSON file next to the application's other configuration. It names
//! providers, models and base URLs — and the *name of an environment
//! variable* for each key, never a key. That is a rule enforced by
//! [`ProviderConfig`](hyperlab_ai::ProviderConfig) having nowhere to put one,
//! and it means this file can be copied into a bug report.

use std::path::Path;

use hyperlab_ai::{AiSettings, ProviderKind, ProviderRegistry};

/// What the file is called.
const FILE: &str = "ai.json";

/// Reads the settings, or the defaults if there are none yet.
///
/// A file that cannot be parsed is reported rather than replaced: silently
/// starting again would throw away configuration the user wrote by hand.
///
/// # Errors
///
/// Returns a sentence to show if the file exists and cannot be read or parsed.
pub fn load(directory: &Path) -> Result<AiSettings, String> {
    let path = directory.join(FILE);
    if !path.exists() {
        return Ok(AiSettings::default());
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("{} is not valid settings: {error}", path.display()))
}

/// Writes the settings.
///
/// # Errors
///
/// Returns a sentence to show if the file cannot be written.
pub fn save(directory: &Path, settings: &AiSettings) -> Result<(), String> {
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("could not make {}: {error}", directory.display()))?;
    let path = directory.join(FILE);
    let text = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("could not write the settings: {error}"))?;
    std::fs::write(&path, text)
        .map_err(|error| format!("could not write {}: {error}", path.display()))
}

/// Builds the providers the settings describe.
///
/// A provider that cannot be built — usually because the environment variable
/// holding its key is not set — is left out and the reason collected, rather
/// than failing the lot. Someone with three providers configured and one
/// broken should still be able to work.
#[must_use]
pub fn build(settings: &AiSettings) -> (ProviderRegistry, Vec<String>) {
    let mut registry = ProviderRegistry::new();
    let mut problems = Vec::new();

    for (name, config) in &settings.providers {
        match hyperlab_ai_providers::build(name, config) {
            Ok(provider) => registry.register(provider),
            Err(error) => problems.push(explain(name, config, &error)),
        }
    }

    if let Some(preferred) = &settings.default_provider
        && let Err(error) = registry.set_default(preferred)
    {
        problems.push(error.to_string());
    }

    (registry, problems)
}

/// Says what went wrong, and what to do about it where that is knowable.
///
/// "the environment variable X is not set" is useful. "this provider is not
/// set up" when no variable was named at all is not, so the likely one is
/// named instead.
fn explain(
    name: &str,
    config: &hyperlab_ai::ProviderConfig,
    error: &hyperlab_ai::AiError,
) -> String {
    match (&config.api_key_env, suggested_key_variable(&config.kind)) {
        (None, Some(variable)) => {
            format!("{name}: {error} — set {variable} and name it in this provider's settings")
        }
        _ => format!("{name}: {error}"),
    }
}

/// What a provider needs configured, for the settings panel to show.
///
/// Names the environment variable a key is expected in, so the panel can say
/// "set `ANTHROPIC_API_KEY`" rather than offering a box to paste one into.
#[must_use]
pub fn suggested_key_variable(kind: &ProviderKind) -> Option<&'static str> {
    match kind {
        ProviderKind::OpenAi => Some("OPENAI_API_KEY"),
        ProviderKind::Anthropic => Some("ANTHROPIC_API_KEY"),
        ProviderKind::OpenRouter => Some("OPENROUTER_API_KEY"),
        ProviderKind::Google => Some("GOOGLE_API_KEY"),
        // Something running on this machine usually wants no key at all.
        ProviderKind::Ollama
        | ProviderKind::Local
        | ProviderKind::Mock
        | ProviderKind::OpenAiCompatible
        | ProviderKind::Other(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use hyperlab_ai::ProviderConfig;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("hyperlab-settings-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn missing_settings_are_not_an_error() {
        let settings = load(&scratch("missing")).expect("no file is fine");
        assert!(settings.providers.is_empty());
    }

    #[test]
    fn settings_survive_a_round_trip() {
        let directory = scratch("round-trip");
        let mut settings = AiSettings::default();
        settings.providers.insert(
            "work".to_string(),
            ProviderConfig {
                kind: ProviderKind::Anthropic,
                model: "some-model".into(),
                base_url: None,
                api_key_env: Some("ANTHROPIC_API_KEY".into()),
            },
        );
        settings.default_provider = Some("work".into());

        save(&directory, &settings).unwrap();
        assert_eq!(load(&directory).unwrap(), settings);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_saved_provider_names_a_variable_and_has_nowhere_to_put_a_key() {
        let directory = scratch("no-keys");
        let mut settings = AiSettings::default();
        settings.providers.insert(
            "work".to_string(),
            ProviderConfig {
                kind: ProviderKind::OpenAi,
                model: "m".into(),
                base_url: None,
                api_key_env: Some("OPENAI_API_KEY".into()),
            },
        );
        save(&directory, &settings).unwrap();

        let written = std::fs::read_to_string(directory.join(FILE)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&written).unwrap();
        let provider = &parsed["providers"]["work"];

        // The name of a variable, and nothing that could hold its value. This
        // is structural — ProviderConfig has no field for a key — and the
        // test is here so that adding one is a visible decision.
        assert_eq!(provider["apiKeyEnv"], "OPENAI_API_KEY");
        let fields: Vec<&String> = provider.as_object().unwrap().keys().collect();
        assert_eq!(fields, ["apiKeyEnv", "kind", "model"]);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_broken_file_is_reported_rather_than_overwritten() {
        let directory = scratch("broken");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(FILE), "{ this is not json").unwrap();

        let error = load(&directory).expect_err("a broken file must be reported");
        assert!(error.contains("not valid settings"), "got {error}");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn one_broken_provider_does_not_take_the_others_with_it() {
        let mut settings = AiSettings::default();
        settings.providers.insert(
            "offline".to_string(),
            ProviderConfig::new(ProviderKind::Mock, "any"),
        );
        settings.providers.insert(
            "broken".to_string(),
            ProviderConfig {
                kind: ProviderKind::OpenAi,
                model: "m".into(),
                base_url: None,
                api_key_env: Some("HYPERLAB_DEFINITELY_UNSET_VARIABLE".into()),
            },
        );

        let (registry, problems) = build(&settings);
        assert!(registry.get("offline").is_some());
        assert!(registry.get("broken").is_none());
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("broken"), "got {problems:?}");
    }
}
