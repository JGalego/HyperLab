//! One helper both protocols share, kept apart from [`http`](crate::http)
//! so that the wire protocol compiles without a transport.

use serde_json::Value;

/// Reads a string out of a nested field, for the `describe_error` functions.
pub(crate) fn text_at(value: &Value, path: &[&str]) -> Option<String> {
    let mut here = value;
    for key in path {
        here = here.get(key)?;
    }
    here.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_text_is_found_or_missed_quietly() {
        let value = serde_json::json!({"error": {"message": "no"}});
        assert_eq!(
            text_at(&value, &["error", "message"]).as_deref(),
            Some("no")
        );
        assert_eq!(text_at(&value, &["error", "detail"]), None);
        assert_eq!(text_at(&value, &["nope", "message"]), None);
    }
}
