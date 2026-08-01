//! JSON-RPC 2.0, in the shape MCP uses it.
//!
//! MCP is JSON-RPC 2.0 over a byte stream, one JSON object per line. This
//! module is only the envelope — what a request, a response and an error look
//! like on the wire. It knows nothing about tools, and nothing about
//! HyperLab, so it can be read and tested on its own.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The only version of JSON-RPC there is any point supporting.
pub const VERSION: &str = "2.0";

/// What a peer calls itself, and which number it answers to.
///
/// JSON-RPC allows a string or a number, and a peer that sent a string must
/// get a string back, so the two cannot be collapsed into one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    /// An id that arrived as a JSON number.
    Number(i64),
    /// An id that arrived as a JSON string.
    Text(String),
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(n) => write!(f, "{n}"),
            Self::Text(s) => write!(f, "{s}"),
        }
    }
}

/// A call, which expects an answer, or a notification, which does not.
///
/// The two differ only by the presence of `id`, which is why they are one
/// type here: a peer decides which it sent, and we must not guess.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// Absent for a notification, which must not be answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,
    /// The method being called, such as `tools/call`.
    pub method: String,
    /// The arguments, whose shape depends on the method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl Request {
    /// A call that expects an answer.
    #[must_use]
    pub fn call(id: RequestId, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: VERSION.to_string(),
            id: Some(id),
            method: method.into(),
            params,
        }
    }

    /// A notification, which must never be answered — not even with an error.
    #[must_use]
    pub fn notify(method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: VERSION.to_string(),
            id: None,
            method: method.into(),
            params,
        }
    }

    /// Whether this is a notification.
    #[must_use]
    pub const fn is_notification(&self) -> bool {
        self.id.is_none()
    }
}

/// An answer to exactly one call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// The id of the call being answered.
    pub id: RequestId,
    /// Present when the call succeeded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Present when it did not. Never both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    /// A successful answer.
    #[must_use]
    pub fn success(id: RequestId, result: Value) -> Self {
        Self {
            jsonrpc: VERSION.to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// A failed answer.
    #[must_use]
    pub fn failure(id: RequestId, error: RpcError) -> Self {
        Self {
            jsonrpc: VERSION.to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// The error object JSON-RPC defines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    /// One of the codes below, or a server-defined one.
    pub code: i32,
    /// A sentence a person could read.
    pub message: String,
    /// Anything else worth saying.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    /// The JSON could not be parsed at all.
    pub const PARSE_ERROR: i32 = -32700;
    /// It parsed, but it was not a JSON-RPC request.
    pub const INVALID_REQUEST: i32 = -32600;
    /// No such method.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// The method exists; the arguments were wrong.
    pub const INVALID_PARAMS: i32 = -32602;
    /// Anything that went wrong on our side.
    pub const INTERNAL_ERROR: i32 = -32603;

    /// An error with a code and a message.
    #[must_use]
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// The JSON could not be parsed.
    #[must_use]
    pub fn parse_error(detail: impl Into<String>) -> Self {
        Self::new(Self::PARSE_ERROR, detail)
    }

    /// It parsed into something that was not a request.
    #[must_use]
    pub fn invalid_request(detail: impl Into<String>) -> Self {
        Self::new(Self::INVALID_REQUEST, detail)
    }

    /// No such method.
    #[must_use]
    pub fn method_not_found(method: &str) -> Self {
        Self::new(
            Self::METHOD_NOT_FOUND,
            format!("there is no method \"{method}\""),
        )
    }

    /// The arguments were wrong.
    #[must_use]
    pub fn invalid_params(detail: impl Into<String>) -> Self {
        Self::new(Self::INVALID_PARAMS, detail)
    }

    /// Something went wrong that was nobody's fault but ours.
    #[must_use]
    pub fn internal(detail: impl Into<String>) -> Self {
        Self::new(Self::INTERNAL_ERROR, detail)
    }
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for RpcError {}

/// Reads one line of JSON as a request.
///
/// A peer that sends rubbish gets a parse error rather than a closed
/// connection, because the next line is usually fine.
///
/// # Errors
///
/// Returns [`RpcError::parse_error`] if the line is not JSON, and
/// [`RpcError::invalid_request`] if it is JSON but not a JSON-RPC 2.0
/// request.
pub fn parse_request(line: &str) -> Result<Request, RpcError> {
    let value: Value =
        serde_json::from_str(line).map_err(|error| RpcError::parse_error(error.to_string()))?;

    // Check the version before shape, so a peer speaking some other protocol
    // is told the real problem rather than "missing field `method`".
    match value.get("jsonrpc").and_then(Value::as_str) {
        Some(VERSION) => {}
        Some(other) => {
            return Err(RpcError::invalid_request(format!(
                "this is JSON-RPC {VERSION}, and that said {other}"
            )));
        }
        None => return Err(RpcError::invalid_request("no jsonrpc version")),
    }

    serde_json::from_value(value).map_err(|error| RpcError::invalid_request(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_carries_its_id_and_a_notification_does_not() {
        let call = Request::call(RequestId::Number(1), "tools/list", None);
        assert!(!call.is_notification());

        let note = Request::notify("notifications/initialized", None);
        assert!(note.is_notification());
    }

    #[test]
    fn a_notification_serializes_without_an_id_field_at_all() {
        // A null id is a valid id in JSON-RPC, so the field must be absent
        // rather than null, or a peer would try to answer a notification.
        let json = serde_json::to_value(Request::notify("ping", None)).unwrap();
        assert!(json.get("id").is_none(), "got {json}");
        assert!(json.get("params").is_none(), "got {json}");
    }

    #[test]
    fn an_id_keeps_the_type_it_arrived_as() {
        let numeric = parse_request(r#"{"jsonrpc":"2.0","id":7,"method":"ping"}"#).unwrap();
        assert_eq!(numeric.id, Some(RequestId::Number(7)));

        let textual = parse_request(r#"{"jsonrpc":"2.0","id":"seven","method":"ping"}"#).unwrap();
        assert_eq!(textual.id, Some(RequestId::Text("seven".to_string())));
    }

    #[test]
    fn rubbish_is_a_parse_error_and_the_wrong_protocol_is_an_invalid_request() {
        let torn = parse_request("{not json").unwrap_err();
        assert_eq!(torn.code, RpcError::PARSE_ERROR);

        let elderly = parse_request(r#"{"jsonrpc":"1.0","method":"ping"}"#).unwrap_err();
        assert_eq!(elderly.code, RpcError::INVALID_REQUEST);

        let anonymous = parse_request(r#"{"method":"ping"}"#).unwrap_err();
        assert_eq!(anonymous.code, RpcError::INVALID_REQUEST);
    }

    #[test]
    fn a_response_holds_a_result_or_an_error_but_never_both() {
        let good = serde_json::to_value(Response::success(RequestId::Number(1), Value::Null))
            .expect("a response serializes");
        assert!(good.get("error").is_none());

        let bad = serde_json::to_value(Response::failure(
            RequestId::Number(1),
            RpcError::method_not_found("nope"),
        ))
        .expect("a response serializes");
        assert!(bad.get("result").is_none());
        assert_eq!(bad["error"]["code"], RpcError::METHOD_NOT_FOUND);
    }
}
