//! Extensible property storage.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Value;

/// The open-ended set of properties carried by every object.
///
/// Property names are case-insensitive (`Visible` and `visible` are the same
/// property) and are stored in a [`BTreeMap`] so that saved files have a
/// stable, diff-friendly order. Objects may carry properties the current
/// version of HyperLab knows nothing about; they round-trip untouched, which
/// is what makes plugins and future versions possible.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PropertyBag {
    entries: BTreeMap<String, Value>,
}

impl PropertyBag {
    /// Creates an empty bag.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Normalizes a property name to its canonical form.
    #[must_use]
    pub fn normalize(name: &str) -> String {
        name.trim().to_ascii_lowercase()
    }

    /// Reads a property.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.entries.get(&Self::normalize(name))
    }

    /// Writes a property, returning the previous value.
    pub fn set(&mut self, name: &str, value: impl Into<Value>) -> Option<Value> {
        self.entries.insert(Self::normalize(name), value.into())
    }

    /// Writes a property only if it is not already present.
    pub fn set_default(&mut self, name: &str, value: impl Into<Value>) {
        self.entries
            .entry(Self::normalize(name))
            .or_insert_with(|| value.into());
    }

    /// Removes a property, returning it.
    pub fn remove(&mut self, name: &str) -> Option<Value> {
        self.entries.remove(&Self::normalize(name))
    }

    /// Whether the property is present.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(&Self::normalize(name))
    }

    /// The number of properties.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the bag holds no properties.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterates over properties in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// The property names, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
}

impl<'a> IntoIterator for &'a PropertyBag {
    type Item = (&'a str, &'a Value);
    type IntoIter = Box<dyn Iterator<Item = (&'a str, &'a Value)> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_case_insensitive() {
        let mut bag = PropertyBag::new();
        bag.set("Visible", true);
        assert_eq!(bag.get("visible"), Some(&Value::Bool(true)));
        bag.set("VISIBLE", false);
        assert_eq!(bag.len(), 1);
    }

    #[test]
    fn set_default_does_not_overwrite() {
        let mut bag = PropertyBag::new();
        bag.set("style", "roundRect");
        bag.set_default("style", "rectangle");
        assert_eq!(bag.get("style"), Some(&Value::text("roundRect")));
    }

    #[test]
    fn unknown_properties_survive_a_round_trip() {
        let mut bag = PropertyBag::new();
        bag.set("somethingFromTheFuture", 42);
        let json = serde_json::to_string(&bag).unwrap();
        let restored: PropertyBag = serde_json::from_str(&json).unwrap();
        assert_eq!(
            restored.get("somethingfromthefuture"),
            Some(&Value::Number(42.0))
        );
    }
}
