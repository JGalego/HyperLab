//! The little bit of HTTP both providers need.
//!
//! Everything vendor-specific — the path, the headers, the shape of the
//! request and the shape of the reply — belongs in the provider. What is left
//! is here: send some JSON, get some JSON back, and turn anything that went
//! wrong into an [`AiError`] a person can act on.

use hyperlab_ai::{AiError, AiResult};
use reqwest::{
    Client, StatusCode,
    header::{HeaderMap, HeaderValue},
};
use serde_json::Value;

/// A JSON endpoint.
///
/// Cloning is cheap — [`Client`] is a handle to a shared pool — which lets a
/// provider hand a copy to the future it returns instead of borrowing itself.
#[derive(Clone)]
pub(crate) struct Endpoint {
    client: Client,
    base_url: String,
    headers: HeaderMap,
}

impl Endpoint {
    /// Builds an endpoint. `base_url` may end in a slash or not.
    ///
    /// # Errors
    ///
    /// Returns [`AiError::NotConfigured`] if a header value is not something
    /// that can be sent — an API key with a newline in it, most likely,
    /// because the environment variable was set from a file.
    pub(crate) fn new(base_url: &str, headers: Vec<(&'static str, String)>) -> AiResult<Self> {
        let mut map = HeaderMap::with_capacity(headers.len() + 1);
        map.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        for (name, value) in headers {
            let value = HeaderValue::from_str(&value).map_err(|_| {
                AiError::NotConfigured(format!(
                    "the value for \"{name}\" cannot be sent as a header"
                ))
            })?;
            map.insert(name, value);
        }

        Ok(Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            headers: map,
        })
    }

    /// Posts `body` to `path` and returns the parsed reply.
    ///
    /// `describe` is given the reply of a failed request and pulls the
    /// vendor's own message out of it, so the user sees "invalid x-api-key"
    /// rather than "401".
    ///
    /// # Errors
    ///
    /// Returns [`AiError::Transport`] if the request never got an answer,
    /// [`AiError::Protocol`] if the answer was not JSON, and whatever
    /// [`classify`] decides for a refusal.
    pub(crate) async fn post_json(
        &self,
        path: &str,
        body: Value,
        describe: fn(&Value) -> Option<String>,
    ) -> AiResult<Value> {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
        let response = self
            .client
            .post(&url)
            .headers(self.headers.clone())
            .json(&body)
            .send()
            .await
            .map_err(|error| AiError::Transport(strip_url(&error.to_string(), &url)))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|error| AiError::Transport(error.to_string()))?;

        // A failed request may answer with prose — an HTML error page from a
        // proxy, say — so the body is only parsed as a courtesy.
        let parsed = serde_json::from_str::<Value>(&text);
        if !status.is_success() {
            let message = parsed
                .as_ref()
                .ok()
                .and_then(describe)
                .unwrap_or_else(|| first_line(&text));
            return Err(classify(status, &message));
        }

        parsed.map_err(|error| AiError::Protocol(format!("the reply was not JSON: {error}")))
    }
}

/// Turns a refused request into the error that best describes what to do
/// about it.
fn classify(status: StatusCode, message: &str) -> AiError {
    let message = if message.is_empty() {
        format!("the provider answered {status}")
    } else {
        message.to_string()
    };
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::PAYMENT_REQUIRED => {
            AiError::NotConfigured(message)
        }
        StatusCode::TOO_MANY_REQUESTS => AiError::Transport(message),
        _ if status.is_server_error() => AiError::Transport(message),
        _ => AiError::Protocol(message),
    }
}

/// The first line of a body, capped, so an HTML error page does not become
/// the error message.
fn first_line(body: &str) -> String {
    const LIMIT: usize = 200;
    let line = body
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    match line.char_indices().nth(LIMIT) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line.to_string(),
    }
}

/// Keeps a URL out of a message.
///
/// A `base_url` can carry a token in a query string, and transport errors get
/// logged and pasted into bug reports.
fn strip_url(message: &str, url: &str) -> String {
    message.replace(url, "the provider")
}

/// Reads a string out of a nested field, for the `describe` functions.
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
    fn a_missing_key_is_a_configuration_problem_not_a_network_one() {
        assert!(matches!(
            classify(StatusCode::UNAUTHORIZED, "invalid api key"),
            AiError::NotConfigured(_)
        ));
    }

    #[test]
    fn being_rate_limited_or_overloaded_is_worth_retrying() {
        assert!(matches!(
            classify(StatusCode::TOO_MANY_REQUESTS, "slow down"),
            AiError::Transport(_)
        ));
        assert!(matches!(
            classify(StatusCode::SERVICE_UNAVAILABLE, "overloaded"),
            AiError::Transport(_)
        ));
    }

    #[test]
    fn anything_else_is_a_bad_request() {
        assert!(matches!(
            classify(StatusCode::BAD_REQUEST, "max_tokens is required"),
            AiError::Protocol(_)
        ));
    }

    #[test]
    fn an_error_with_no_message_still_says_something() {
        let AiError::Protocol(message) = classify(StatusCode::BAD_REQUEST, "") else {
            panic!("a 400 is a protocol error");
        };
        assert!(message.contains("400"), "{message}");
    }

    #[test]
    fn an_html_error_page_does_not_become_the_message() {
        let page = format!("<html>\n<body>{}</body>\n</html>", "x".repeat(500));
        let line = first_line(&page);
        assert_eq!(line, "<html>");
    }

    #[test]
    fn a_long_line_is_cut_at_a_character_boundary() {
        let line = first_line(&"é".repeat(500));
        assert!(line.ends_with('…') && line.chars().count() == 201, "{line}");
    }

    #[test]
    fn a_url_never_appears_in_a_transport_error() {
        let message = strip_url(
            "error sending request for url (https://host/v1?key=secret)",
            "https://host/v1?key=secret",
        );
        assert!(!message.contains("secret"), "{message}");
    }

    #[test]
    fn a_key_that_cannot_be_a_header_is_reported_as_such() {
        let error = Endpoint::new(
            "https://example.test",
            vec![("x-api-key", "bad\nkey".into())],
        );
        assert!(matches!(error.err(), Some(AiError::NotConfigured(_))));
    }

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
