//! Keeping track of the providers that are available.

use std::{collections::BTreeMap, sync::Arc};

use crate::provider::{AiError, AiProvider, AiResult};

/// The providers this session can use.
///
/// Providers are registered at startup — by HyperLab itself, or later by
/// plugins — and looked up by name. Nothing else in the system holds a
/// provider directly, so switching provider is a one-line change in one
/// place.
#[derive(Default, Clone)]
pub struct ProviderRegistry {
    providers: BTreeMap<String, Arc<dyn AiProvider>>,
    default: Option<String>,
}

impl ProviderRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a provider under its own name. The first one registered becomes
    /// the default until something says otherwise.
    pub fn register(&mut self, provider: Arc<dyn AiProvider>) {
        let name = provider.name().to_string();
        self.default.get_or_insert_with(|| name.clone());
        self.providers.insert(name, provider);
    }

    /// Chooses the provider to use when none is named.
    ///
    /// # Errors
    ///
    /// Returns [`AiError::NotConfigured`] if there is no such provider.
    pub fn set_default(&mut self, name: &str) -> AiResult<()> {
        if !self.providers.contains_key(name) {
            return Err(AiError::NotConfigured(format!(
                "there is no provider \"{name}\""
            )));
        }
        self.default = Some(name.to_string());
        Ok(())
    }

    /// Looks a provider up by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Arc<dyn AiProvider>> {
        self.providers.get(name).cloned()
    }

    /// The default provider.
    ///
    /// # Errors
    ///
    /// Returns [`AiError::NotConfigured`] if no provider has been registered.
    pub fn default_provider(&self) -> AiResult<Arc<dyn AiProvider>> {
        self.default
            .as_ref()
            .and_then(|name| self.providers.get(name))
            .cloned()
            .ok_or_else(|| {
                AiError::NotConfigured("no language model has been set up yet".to_string())
            })
    }

    /// The names of every registered provider, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.providers.keys().map(String::as_str)
    }

    /// Whether anything is registered at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

impl std::fmt::Debug for ProviderRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderRegistry")
            .field("providers", &self.providers.keys().collect::<Vec<_>>())
            .field("default", &self.default)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockProvider;

    #[test]
    fn the_first_provider_registered_becomes_the_default() {
        let mut registry = ProviderRegistry::new();
        assert!(registry.default_provider().is_err());

        registry.register(Arc::new(MockProvider::new("mock")));
        assert_eq!(registry.default_provider().unwrap().name(), "mock");

        registry.register(Arc::new(MockProvider::new("other")));
        assert_eq!(
            registry.default_provider().unwrap().name(),
            "mock",
            "registering a second provider must not change the user's choice"
        );
    }

    #[test]
    fn the_default_can_be_changed_only_to_something_that_exists() {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(MockProvider::new("mock")));
        assert!(registry.set_default("nowhere").is_err());
        assert!(registry.set_default("mock").is_ok());
    }
}
