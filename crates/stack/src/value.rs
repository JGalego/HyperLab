//! The single value type shared by properties, variables and scripts.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A HyperTalk value.
///
/// HyperTalk is famously "everything is text": `"3" + 4` is `7` and
/// `3 & 4` is `"34"`. `Value` keeps the richer representation when it has one
/// so that saved files stay readable (`"visible": true` rather than
/// `"visible": "true"`), while [`Value::as_number`] and [`Value::as_text`]
/// provide the loose conversions scripts expect.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    /// A boolean, written `true` or `false` in scripts.
    Bool(bool),
    /// A number. HyperTalk does not distinguish integers from reals.
    Number(f64),
    /// Text.
    Text(String),
    /// The absence of a value, written `empty` in scripts. Serializes as
    /// `null` and compares equal to the empty string.
    Empty,
}

impl Value {
    /// Builds a text value.
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

    /// The value rendered as text, the way a script would see it.
    ///
    /// Whole numbers lose their fractional part: `5.0` renders as `"5"`.
    #[must_use]
    pub fn as_text(&self) -> String {
        match self {
            Self::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
            Self::Number(n) => format_number(*n),
            Self::Text(s) => s.clone(),
            Self::Empty => String::new(),
        }
    }

    /// The value as a number, if it can be read as one.
    #[must_use]
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(*n),
            Self::Bool(_) | Self::Empty => None,
            Self::Text(s) => s.trim().parse().ok(),
        }
    }

    /// The value as a boolean, if it can be read as one.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            Self::Text(s) => match s.trim().to_ascii_lowercase().as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            },
            Self::Number(_) | Self::Empty => None,
        }
    }

    /// Whether the value renders as the empty string.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty) || self.as_text().is_empty()
    }

    /// Case-insensitive comparison, the way HyperTalk's `=` operator works.
    ///
    /// Two values are equal if they are numerically equal, or if their text
    /// forms match ignoring case.
    #[must_use]
    pub fn loosely_equals(&self, other: &Self) -> bool {
        match (self.as_number(), other.as_number()) {
            (Some(a), Some(b)) => (a - b).abs() < f64::EPSILON,
            _ => self.as_text().eq_ignore_ascii_case(&other.as_text()),
        }
    }
}

/// Renders a number the way HyperTalk does: without a trailing `.0`.
fn format_number(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_text())
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        self.loosely_equals(other)
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self::Number(value as f64)
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Self::Number(f64::from(value))
    }
}

impl From<usize> for Value {
    fn from(value: usize) -> Self {
        Self::Number(value as f64)
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_numbers_render_without_decimals() {
        assert_eq!(Value::Number(5.0).as_text(), "5");
        assert_eq!(Value::Number(2.5).as_text(), "2.5");
        assert_eq!(Value::Number(-0.5).as_text(), "-0.5");
    }

    #[test]
    fn text_converts_to_number_when_it_looks_like_one() {
        assert_eq!(Value::text(" 42 ").as_number(), Some(42.0));
        assert_eq!(Value::text("forty two").as_number(), None);
    }

    #[test]
    fn equality_ignores_case_and_representation() {
        assert_eq!(Value::text("Hello"), Value::text("hello"));
        assert_eq!(Value::text("7"), Value::Number(7.0));
        assert_eq!(Value::Empty, Value::text(""));
        assert_ne!(Value::text("a"), Value::text("b"));
    }

    #[test]
    fn booleans_round_trip_through_text() {
        assert_eq!(Value::text("TRUE").as_bool(), Some(true));
        assert_eq!(Value::Bool(false).as_text(), "false");
        assert_eq!(Value::text("yes").as_bool(), None);
    }

    #[test]
    fn json_uses_natural_representations() {
        let json = serde_json::to_string(&vec![
            Value::Bool(true),
            Value::Number(3.0),
            Value::text("hi"),
            Value::Empty,
        ])
        .unwrap();
        assert_eq!(json, r#"[true,3.0,"hi",null]"#);
    }
}
