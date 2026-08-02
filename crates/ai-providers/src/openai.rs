//! A client for the OpenAI chat-completions protocol.
//!
//! Nothing here is tied to OpenAI. The protocol is the closest thing the
//! field has to a lingua franca, and OpenRouter, Ollama, LM Studio, llama.cpp
//! and vLLM all speak it, so one client with a configurable `base_url` serves
//! the lot. That is the point: HyperLab gains four kinds of provider without
//! learning that any of them exist.

#[cfg(any(feature = "native", test))]
use hyperlab_ai::Embedding;
use hyperlab_ai::{
    AiError, AiResult, ChatMessage, Completion, CompletionRequest, FinishReason, Role, ToolCall,
    ToolDefinition, Usage,
};
#[cfg(feature = "native")]
use hyperlab_ai::{AiProvider, BoxFuture, Capabilities, ProviderConfig};
use serde_json::{Map, Value, json};

#[cfg(feature = "native")]
use crate::http::Endpoint;
use crate::text::text_at;

/// Where requests go when the configuration does not say.
pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// A provider that speaks the OpenAI chat-completions protocol.
#[cfg(feature = "native")]
pub struct OpenAiProvider {
    name: String,
    model: String,
    embedding_model: Option<String>,
    local: bool,
    endpoint: Endpoint,
}

#[cfg(feature = "native")]
impl OpenAiProvider {
    /// Builds a provider with a key the caller has already found.
    ///
    /// Finding it is [`build`](crate::build)'s job, because where a key lives
    /// is a question about configuration rather than about this protocol.
    /// `None` sends no credentials at all, which is right for a server on
    /// this machine.
    ///
    /// # Errors
    ///
    /// Returns [`AiError::NotConfigured`] if the key cannot be sent as a
    /// header, which usually means it was read from a file and kept its
    /// trailing newline.
    pub fn with_api_key(
        name: impl Into<String>,
        config: &ProviderConfig,
        api_key: Option<String>,
    ) -> AiResult<Self> {
        let headers = api_key
            .map(|key| ("authorization", format!("Bearer {key}")))
            .into_iter()
            .collect();

        Ok(Self {
            name: name.into(),
            model: config.model.clone(),
            embedding_model: None,
            local: config.kind.is_local(),
            endpoint: Endpoint::new(
                config.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL),
                headers,
            )?,
        })
    }

    /// Names the model to use for embeddings.
    ///
    /// Without this the provider says it cannot embed, which is the truth:
    /// the chat model is not an embedding model, and guessing the name of one
    /// would be picking a vendor.
    #[must_use]
    pub fn with_embedding_model(mut self, model: impl Into<String>) -> Self {
        self.embedding_model = Some(model.into());
        self
    }
}

/// The body of a `/chat/completions` request.
///
/// A free function rather than a method so that a host that brings its own
/// transport — the browser — can speak the protocol without building a
/// client. `default_model` is used when the request names none.
#[must_use]
pub fn completion_body(default_model: &str, request: &CompletionRequest) -> Value {
    let mut body = Map::new();
    body.insert(
        "model".into(),
        json!(if request.model.is_empty() {
            default_model
        } else {
            &request.model
        }),
    );
    body.insert(
        "messages".into(),
        Value::Array(request.messages.iter().map(encode_message).collect()),
    );
    if !request.tools.is_empty() {
        body.insert(
            "tools".into(),
            Value::Array(request.tools.iter().map(encode_tool).collect()),
        );
    }
    if let Some(temperature) = request.temperature {
        body.insert("temperature".into(), json!(temperature));
    }
    if let Some(max_tokens) = request.max_tokens {
        body.insert("max_tokens".into(), json!(max_tokens));
    }
    Value::Object(body)
}

#[cfg(feature = "native")]
impl AiProvider for OpenAiProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            tools: true,
            embeddings: self.embedding_model.is_some(),
            local: self.local,
        }
    }

    fn complete<'a>(&'a self, request: CompletionRequest) -> BoxFuture<'a, AiResult<Completion>> {
        // Everything the request needs is worked out now, so the future owns
        // its inputs and borrows nothing.
        let endpoint = self.endpoint.clone();
        let body = completion_body(&self.model, &request);
        Box::pin(async move {
            let reply = endpoint
                .post_json("chat/completions", body, describe_error)
                .await?;
            decode_completion(&reply)
        })
    }

    fn embed<'a>(&'a self, texts: Vec<String>) -> BoxFuture<'a, AiResult<Vec<Embedding>>> {
        let endpoint = self.endpoint.clone();
        let model = self.embedding_model.clone();
        let name = self.name.clone();
        Box::pin(async move {
            let Some(model) = model else {
                return Err(AiError::Unsupported(format!(
                    "{name} has no embedding model set"
                )));
            };
            let wanted = texts.len();
            let reply = endpoint
                .post_json(
                    "embeddings",
                    json!({ "model": model, "input": texts }),
                    describe_error,
                )
                .await?;
            decode_embeddings(&reply, wanted)
        })
    }
}

/// One message, in the protocol's shape.
fn encode_message(message: &ChatMessage) -> Value {
    let mut encoded = Map::new();
    encoded.insert("role".into(), json!(role_name(message.role)));

    // An assistant turn that only asks for tools has no text, and the
    // protocol wants the field left out rather than sent empty.
    if !message.content.is_empty() || message.tool_calls.is_empty() {
        encoded.insert("content".into(), json!(message.content));
    }
    if let Some(id) = &message.tool_call_id {
        encoded.insert("tool_call_id".into(), json!(id));
    }
    if !message.tool_calls.is_empty() {
        encoded.insert(
            "tool_calls".into(),
            Value::Array(
                message
                    .tool_calls
                    .iter()
                    .map(|call| {
                        json!({
                            "id": call.id,
                            "type": "function",
                            "function": {
                                "name": call.name,
                                // Arguments travel as a string, not as JSON.
                                "arguments": call.arguments.to_string(),
                            },
                        })
                    })
                    .collect(),
            ),
        );
    }
    Value::Object(encoded)
}

/// The name the protocol uses for a role.
const fn role_name(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

/// One tool, wrapped the way the protocol wants it.
fn encode_tool(tool: &ToolDefinition) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema,
        },
    })
}

/// Reads a reply into a [`Completion`].
///
/// # Errors
///
/// Returns [`AiError::Protocol`] if the reply is not shaped like an answer.
pub fn decode_completion(reply: &Value) -> AiResult<Completion> {
    let choice = reply
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or_else(|| AiError::Protocol("the reply held no choices".into()))?;
    let message = choice
        .get("message")
        .ok_or_else(|| AiError::Protocol("the choice held no message".into()))?;

    let tool_calls = match message.get("tool_calls").and_then(Value::as_array) {
        Some(calls) => calls
            .iter()
            .map(decode_tool_call)
            .collect::<AiResult<_>>()?,
        None => Vec::new(),
    };

    Ok(Completion {
        content: message
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        finish_reason: finish_reason(
            choice.get("finish_reason").and_then(Value::as_str),
            &tool_calls,
        ),
        tool_calls,
        usage: decode_usage(reply.get("usage")),
    })
}

/// Reads one tool call. The arguments arrive as a string of JSON.
fn decode_tool_call(call: &Value) -> AiResult<ToolCall> {
    let name = call
        .get("function")
        .and_then(|function| function.get("name"))
        .and_then(Value::as_str)
        .ok_or_else(|| AiError::Protocol("a tool call had no name".into()))?;
    let arguments = call
        .get("function")
        .and_then(|function| function.get("arguments"))
        .and_then(Value::as_str)
        .unwrap_or("{}");

    Ok(ToolCall {
        id: call
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        name: name.to_string(),
        arguments: serde_json::from_str(arguments).map_err(|error| {
            AiError::Protocol(format!("the arguments for {name} were not JSON: {error}"))
        })?,
    })
}

/// Why the model stopped.
///
/// A reply with tool calls but no reason — some compatible servers omit it —
/// plainly wants its tools run.
fn finish_reason(reason: Option<&str>, tool_calls: &[ToolCall]) -> FinishReason {
    match reason {
        Some("length") => FinishReason::Length,
        Some("tool_calls" | "function_call") => FinishReason::ToolUse,
        Some("content_filter") => FinishReason::Filtered,
        _ if !tool_calls.is_empty() => FinishReason::ToolUse,
        _ => FinishReason::Stop,
    }
}

/// What the request cost, when the server says.
fn decode_usage(usage: Option<&Value>) -> Option<Usage> {
    let usage = usage?;
    Some(Usage {
        input_tokens: count(usage, "prompt_tokens"),
        output_tokens: count(usage, "completion_tokens"),
    })
}

/// A token count, which servers report as a plain number.
fn count(usage: &Value, field: &str) -> u32 {
    usage
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|count| u32::try_from(count).ok())
        .unwrap_or(0)
}

/// Reads an embeddings reply, putting the vectors back in the order the texts
/// were given in.
#[cfg(any(feature = "native", test))]
fn decode_embeddings(reply: &Value, wanted: usize) -> AiResult<Vec<Embedding>> {
    let data = reply
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| AiError::Protocol("the reply held no embeddings".into()))?;
    if data.len() != wanted {
        return Err(AiError::Protocol(format!(
            "asked for {wanted} embeddings and got {}",
            data.len()
        )));
    }

    let mut ordered: Vec<(u64, Embedding)> = data
        .iter()
        .enumerate()
        .map(|(position, entry)| {
            let values = entry
                .get("embedding")
                .and_then(Value::as_array)
                .ok_or_else(|| AiError::Protocol("an embedding held no vector".into()))?
                .iter()
                .map(|number| number.as_f64().unwrap_or_default() as f32)
                .collect();
            let index = entry
                .get("index")
                .and_then(Value::as_u64)
                .unwrap_or(position as u64);
            Ok((index, Embedding { values }))
        })
        .collect::<AiResult<_>>()?;
    ordered.sort_by_key(|(index, _)| *index);

    Ok(ordered
        .into_iter()
        .map(|(_, embedding)| embedding)
        .collect())
}

/// The server's own account of what went wrong, from an error reply's body.
#[must_use]
pub fn describe_error(reply: &Value) -> Option<String> {
    text_at(reply, &["error", "message"]).or_else(|| text_at(reply, &["message"]))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "native")]
    use hyperlab_ai::ProviderKind;

    #[cfg(feature = "native")]
    fn provider() -> OpenAiProvider {
        OpenAiProvider::with_api_key(
            "openai",
            &ProviderConfig::new(ProviderKind::OpenAi, "a-model"),
            None,
        )
        .expect("no key, nothing to go wrong")
    }

    #[cfg(feature = "native")]
    #[test]
    fn a_local_server_needs_no_key() {
        let mut config = ProviderConfig::new(ProviderKind::Ollama, "llama");
        config.base_url = Some("http://localhost:11434/v1".into());
        let provider = OpenAiProvider::with_api_key("ollama", &config, None).unwrap();
        assert!(provider.capabilities().local);
    }

    #[cfg(feature = "native")]
    #[test]
    fn embedding_is_unsupported_until_a_model_is_named() {
        assert!(!provider().capabilities().embeddings);
        assert!(
            provider()
                .with_embedding_model("an-embedding-model")
                .capabilities()
                .embeddings
        );
    }

    #[test]
    fn a_request_uses_the_configured_model_when_it_names_none() {
        let request = CompletionRequest::new("", vec![ChatMessage::user("hi")]);
        assert_eq!(completion_body("a-model", &request)["model"], "a-model");

        let request = CompletionRequest::new("another", vec![ChatMessage::user("hi")]);
        assert_eq!(completion_body("a-model", &request)["model"], "another");
    }

    #[test]
    fn a_conversation_keeps_its_shape() {
        let request = CompletionRequest::new(
            "a-model",
            vec![
                ChatMessage::system("be brief"),
                ChatMessage::user("hello"),
                ChatMessage::assistant("hi"),
            ],
        );
        let body = completion_body("a-model", &request);
        assert_eq!(
            body["messages"],
            json!([
                {"role": "system", "content": "be brief"},
                {"role": "user", "content": "hello"},
                {"role": "assistant", "content": "hi"},
            ])
        );
        assert!(body.get("tools").is_none(), "no tools, no field");
    }

    #[test]
    fn tool_arguments_are_sent_as_a_string() {
        let mut asking = ChatMessage::assistant("");
        asking.tool_calls = vec![ToolCall {
            id: "call_1".into(),
            name: "read_card".into(),
            arguments: json!({"card": 2}),
        }];
        let request = CompletionRequest::new(
            "a-model",
            vec![asking, ChatMessage::tool_result("call_1", "a card")],
        );
        let body = completion_body("a-model", &request);

        let asking = &body["messages"][0];
        assert!(asking.get("content").is_none(), "an empty turn sends none");
        assert_eq!(
            asking["tool_calls"][0]["function"]["arguments"],
            r#"{"card":2}"#
        );
        assert_eq!(body["messages"][1]["tool_call_id"], "call_1");
    }

    #[test]
    fn tools_are_wrapped_in_a_function() {
        let request =
            CompletionRequest::new("a-model", vec![ChatMessage::user("hi")]).with_tools(vec![
                ToolDefinition::new("read_card", "Reads a card.", json!({"type": "object"})),
            ]);
        assert_eq!(
            completion_body("a-model", &request)["tools"],
            json!([{
                "type": "function",
                "function": {
                    "name": "read_card",
                    "description": "Reads a card.",
                    "parameters": {"type": "object"},
                },
            }])
        );
    }

    #[test]
    fn a_plain_reply_is_read() {
        let completion = decode_completion(&json!({
            "choices": [{"message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 9, "completion_tokens": 2},
        }))
        .unwrap();
        assert_eq!(completion.content, "hi");
        assert_eq!(completion.finish_reason, FinishReason::Stop);
        assert_eq!(
            completion.usage,
            Some(Usage {
                input_tokens: 9,
                output_tokens: 2
            })
        );
    }

    #[test]
    fn a_reply_asking_for_a_tool_is_read() {
        let completion = decode_completion(&json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "read_card", "arguments": "{\"card\": 2}"},
                    }],
                },
                "finish_reason": "tool_calls",
            }],
        }))
        .unwrap();
        assert_eq!(completion.content, "");
        assert_eq!(completion.finish_reason, FinishReason::ToolUse);
        assert_eq!(completion.tool_calls[0].name, "read_card");
        assert_eq!(completion.tool_calls[0].arguments, json!({"card": 2}));
    }

    #[test]
    fn a_server_that_omits_the_reason_is_understood_anyway() {
        let completion = decode_completion(&json!({
            "choices": [{"message": {
                "tool_calls": [{"id": "1", "function": {"name": "beep", "arguments": "{}"}}],
            }}],
        }))
        .unwrap();
        assert_eq!(completion.finish_reason, FinishReason::ToolUse);
    }

    #[test]
    fn malformed_tool_arguments_say_which_tool() {
        let error = decode_completion(&json!({
            "choices": [{"message": {
                "tool_calls": [{"id": "1", "function": {"name": "read_card", "arguments": "{oops"}}],
            }}],
        }))
        .unwrap_err();
        assert!(format!("{error}").contains("read_card"), "{error}");
    }

    #[test]
    fn a_reply_with_no_choices_is_a_protocol_error() {
        assert!(matches!(
            decode_completion(&json!({"id": "nothing"})),
            Err(AiError::Protocol(_))
        ));
    }

    #[test]
    fn embeddings_come_back_in_the_order_they_were_asked_for() {
        let embeddings = decode_embeddings(
            &json!({"data": [
                {"index": 1, "embedding": [0.5]},
                {"index": 0, "embedding": [0.25]},
            ]}),
            2,
        )
        .unwrap();
        assert_eq!(embeddings[0].values, vec![0.25]);
        assert_eq!(embeddings[1].values, vec![0.5]);
    }

    #[test]
    fn a_short_embeddings_reply_is_refused_rather_than_misaligned() {
        assert!(matches!(
            decode_embeddings(&json!({"data": [{"index": 0, "embedding": [1.0]}]}), 2),
            Err(AiError::Protocol(_))
        ));
    }

    #[test]
    fn the_servers_own_message_is_preferred() {
        assert_eq!(
            describe_error(&json!({"error": {"message": "model not found"}})).as_deref(),
            Some("model not found")
        );
        assert_eq!(describe_error(&json!({"nothing": true})), None);
    }
}
