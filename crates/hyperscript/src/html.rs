//! Getting text safely into a page.
//!
//! A stack's names and field contents are the author's, and they end up in
//! markup, in attributes and inside quoted _hyperscript strings. Each of the
//! three has different teeth.

/// Text as element content or an attribute value.
///
/// Escapes both quote characters as well as the angle brackets, so one
/// function is right in both places and there is no second one to pick wrong.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Text as a single-quoted _hyperscript string literal.
///
/// Single quotes because the whole handler lives inside a double-quoted `_`
/// attribute. A newline has to become `\n` rather than a real line break: a
/// literal newline inside a string ends the statement.
pub fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for character in text.chars() {
        match character {
            '\'' => out.push_str("\\'"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            other => out.push(other),
        }
    }
    out.push('\'');
    out
}

/// A name as the tail of an HTML id: lower case, one hyphen between words.
///
/// Two different names can collide here — `My Field` and `my-field` both
/// arrive as `my-field` — so callers append a number to keep ids unique
/// rather than trusting this to.
pub fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut hyphen = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
            hyphen = false;
        } else if !out.is_empty() && !hyphen {
            out.push('-');
            hyphen = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "part".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_in_a_name_stays_text() {
        assert_eq!(escape("<script>"), "&lt;script&gt;");
        assert_eq!(escape("a & b"), "a &amp; b");
    }

    #[test]
    fn both_quotes_are_escaped_because_both_appear_in_attributes() {
        assert_eq!(escape(r#"say "hi""#), "say &quot;hi&quot;");
        assert_eq!(escape("it's"), "it&#39;s");
    }

    #[test]
    fn a_quote_cannot_end_a_generated_string_early() {
        assert_eq!(quoted("it's"), r"'it\'s'");
        assert_eq!(quoted("a\\b"), r"'a\\b'");
    }

    #[test]
    fn a_newline_becomes_an_escape_rather_than_ending_the_statement() {
        assert_eq!(quoted("one\ntwo"), r"'one\ntwo'");
        assert_eq!(quoted("one\r\ntwo"), r"'one\ntwo'");
    }

    #[test]
    fn a_slug_is_something_an_id_can_hold() {
        assert_eq!(slug("Clear Done"), "clear-done");
        assert_eq!(slug("The Mansion!"), "the-mansion");
        assert_eq!(slug("  "), "part");
        assert_eq!(slug("Billiard  Room"), "billiard-room");
    }
}
