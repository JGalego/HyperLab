//! A client for Anthropic's Messages API.
//!
//! The protocol differs from the chat-completions one in three ways that
//! matter here, and each is handled in its own function below:
//!
//! * system prompts are a field on the request, not a role in the
//!   conversation (`hoist_system`);
//! * content is a list of typed blocks rather than a string, and a tool
//!   result is a block inside a *user* turn (`encode_messages`);
//! * `max_tokens` is required (see [`DEFAULT_MAX_TOKENS`]).

use hyperlab_ai::{
    AiError, AiProvider, AiResult, BoxFuture, Capabilities, ChatMessage, Completion,
    CompletionRequest, FinishReason, ProviderConfig, Role, ToolCall, ToolDefinition, Usage,
};
use serde_json::{Map, Value, json};

use crate::http::{Endpoint, text_at};

/// Where requests go when the configuration does not say.
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com/v1";

/// The version of the protocol this client speaks.
///
/// Anthropic dates its breaking changes and asks every request to name the
/// version it was written against, so old clients keep working.
pub const API_VERSION: &str = "2023-06-01";

/// The ceiling used when a request does not set one.
///
/// The API requires the field, so something has to be chosen. This is roomy
/// enough for anything HyperLab asks for — a script, an explanation — and
/// small enough to bound the bill on a runaway answer.
pub const DEFAULT_MAX_TOKENS: u32 = 4096;

/// A provider that speaks Anthropic's Messages API.
pub struct AnthropicProvider {
    name: String,
    model: String,
    local: bool,
    endpoint: Endpoint,
}

impl AnthropicProvider {
    /// Builds a provider with a key the caller has already found.
    ///
    /// Finding it is [`build`](crate::build)'s job, because where a key lives
    /// is a question about configuration rather than about this protocol.
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
        let mut headers = vec![("anthropic-version", API_VERSION.to_string())];
        if let Some(key) = api_key {
            headers.push(("x-api-key", key));
        }

        Ok(Self {
            name: name.into(),
            model: config.model.clone(),
            local: config.kind.is_local(),
            endpoint: Endpoint::new(
                config.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL),
                headers,
            )?,
        })
    }

    /// The body of a `/messages` request.
    fn completion_body(&self, request: &CompletionRequest) -> Value {
        let mut body = Map::new();
        body.insert(
            "model".into(),
            json!(if request.model.is_empty() {
                &self.model
            } else {
                &request.model
            }),
        );
        body.insert(
            "max_tokens".into(),
            json!(request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS)),
        );
        if let Some(system) = hoist_system(&request.messages) {
            body.insert("system".into(), json!(system));
        }
        body.insert("messages".into(), encode_messages(&request.messages));
        if !request.tools.is_empty() {
            body.insert(
                "tools".into(),
                Value::Array(request.tools.iter().map(encode_tool).collect()),
            );
        }
        if let Some(temperature) = request.temperature {
            body.insert("temperature".into(), json!(temperature));
        }
        Value::Object(body)
    }
}

impl AiProvider for AnthropicProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            tools: true,
            // There is no embeddings endpoint, so `embed` keeps the trait's
            // default and says so rather than pretending.
            embeddings: false,
            local: self.local,
        }
    }

    fn complete<'a>(&'a self, request: CompletionRequest) -> BoxFuture<'a, AiResult<Completion>> {
        let endpoint = self.endpoint.clone();
        let body = self.completion_body(&request);
        Box::pin(async move {
            let reply = endpoint.post_json("messages", body, describe_error).await?;
            decode_completion(&reply)
        })
    }
}

/// Collects the system messages into the one field the API has for them.
///
/// Several are joined with a blank line, which is what a person writing one
/// long prompt would have done.
fn hoist_system(messages: &[ChatMessage]) -> Option<String> {
    let system: Vec<&str> = messages
        .iter()
        .filter(|message| message.role == Role::System)
        .map(|message| message.content.as_str())
        .filter(|content| !content.is_empty())
        .collect();
    (!system.is_empty()).then(|| system.join("\n\n"))
}

/// The conversation, as turns of typed content blocks.
///
/// Tool results are user content here, so a run of them collapses into a
/// single turn — as does any other run of same-role messages, which the API
/// would otherwise reject.
fn encode_messages(messages: &[ChatMessage]) -> Value {
    let mut turns: Vec<(&'static str, Vec<Value>)> = Vec::new();
    for message in messages {
        let Some((role, blocks)) = encode_message(message) else {
            continue;
        };
        match turns.last_mut() {
            Some((last, existing)) if *last == role => existing.extend(blocks),
            _ => turns.push((role, blocks)),
        }
    }

    Value::Array(
        turns
            .into_iter()
            .map(|(role, content)| json!({"role": role, "content": content}))
            .collect(),
    )
}

/// One message as a role and its content blocks, or `None` for a message the
/// conversation does not carry — a system prompt, which was hoisted, or an
/// empty turn, which would be rejected.
fn encode_message(message: &ChatMessage) -> Option<(&'static str, Vec<Value>)> {
    let mut blocks = Vec::new();
    if !message.content.is_empty() {
        let block = match message.role {
            // A tool result is a block that points back at the request.
            Role::Tool => json!({
                "type": "tool_result",
                "tool_use_id": message.tool_call_id.clone().unwrap_or_default(),
                "content": message.content,
            }),
            _ => json!({"type": "text", "text": message.content}),
        };
        blocks.push(block);
    }
    blocks.extend(message.tool_calls.iter().map(|call| {
        json!({
            "type": "tool_use",
            "id": call.id,
            "name": call.name,
            "input": call.arguments,
        })
    }));

    match message.role {
        Role::System => None,
        Role::Assistant if !blocks.is_empty() => Some(("assistant", blocks)),
        // A tool result belongs to the person's side of the conversation.
        Role::User | Role::Tool if !blocks.is_empty() => Some(("user", blocks)),
        _ => None,
    }
}

/// One tool. The API takes the schema flat, not wrapped in a function.
fn encode_tool(tool: &ToolDefinition) -> Value {
    json!({
        "name": tool.name,
        "description": tool.description,
        "input_schema": tool.input_schema,
    })
}

/// Reads a reply into a [`Completion`].
fn decode_completion(reply: &Value) -> AiResult<Completion> {
    let blocks = reply
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| AiError::Protocol("the reply held no content".into()))?;

    let mut text = String::new();
    let mut tool_calls = Vec::new();
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => text.push_str(block.get("text").and_then(Value::as_str).unwrap_or("")),
            Some("tool_use") => tool_calls.push(decode_tool_use(block)?),
            // Thinking blocks and anything added later: HyperLab has no use
            // for them, and skipping is better than failing.
            _ => {}
        }
    }

    Ok(Completion {
        content: text,
        finish_reason: finish_reason(
            reply.get("stop_reason").and_then(Value::as_str),
            &tool_calls,
        ),
        tool_calls,
        usage: decode_usage(reply.get("usage")),
    })
}

/// Reads one `tool_use` block. Unlike the other protocol, the input is
/// already JSON.
fn decode_tool_use(block: &Value) -> AiResult<ToolCall> {
    Ok(ToolCall {
        id: block
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        name: block
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| AiError::Protocol("a tool call had no name".into()))?
            .to_string(),
        arguments: block.get("input").cloned().unwrap_or_else(|| json!({})),
    })
}

/// Why the model stopped.
///
/// `pause_turn` means a long turn was interrupted and may be continued;
/// HyperLab has no notion of continuing, so it reads as a normal stop. An
/// unfamiliar reason does too — the content is there either way, and a new
/// name for stopping should not lose it.
fn finish_reason(reason: Option<&str>, tool_calls: &[ToolCall]) -> FinishReason {
    match reason {
        Some("max_tokens") => FinishReason::Length,
        Some("tool_use") => FinishReason::ToolUse,
        Some("refusal") => FinishReason::Filtered,
        _ if !tool_calls.is_empty() => FinishReason::ToolUse,
        _ => FinishReason::Stop,
    }
}

/// What the request cost.
fn decode_usage(usage: Option<&Value>) -> Option<Usage> {
    let usage = usage?;
    Some(Usage {
        input_tokens: count(usage, "input_tokens"),
        output_tokens: count(usage, "output_tokens"),
    })
}

/// A token count.
fn count(usage: &Value, field: &str) -> u32 {
    usage
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|count| u32::try_from(count).ok())
        .unwrap_or(0)
}

/// The API's own account of what went wrong.
fn describe_error(reply: &Value) -> Option<String> {
    text_at(reply, &["error", "message"])
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperlab_ai::ProviderKind;

    fn provider() -> AnthropicProvider {
        AnthropicProvider::with_api_key(
            "anthropic",
            &ProviderConfig::new(ProviderKind::Anthropic, "a-model"),
            None,
        )
        .expect("no key, nothing to go wrong")
    }

    #[test]
    fn max_tokens_is_always_sent_because_the_api_insists() {
        let mut request = CompletionRequest::new("a-model", vec![ChatMessage::user("hi")]);
        assert_eq!(
            provider().completion_body(&request)["max_tokens"],
            DEFAULT_MAX_TOKENS
        );

        request.max_tokens = Some(64);
        assert_eq!(provider().completion_body(&request)["max_tokens"], 64);
    }

    #[test]
    fn system_messages_leave_the_conversation_for_their_own_field() {
        let request = CompletionRequest::new(
            "a-model",
            vec![
                ChatMessage::system("be brief"),
                ChatMessage::user("hello"),
                ChatMessage::system("and kind"),
            ],
        );
        let body = provider().completion_body(&request);
        assert_eq!(body["system"], "be brief\n\nand kind");
        assert_eq!(
            body["messages"],
            json!([{"role": "user", "content": [{"type": "text", "text": "hello"}]}])
        );
    }

    #[test]
    fn a_conversation_with_no_system_prompt_sends_no_field() {
        let request = CompletionRequest::new("a-model", vec![ChatMessage::user("hello")]);
        assert!(provider().completion_body(&request).get("system").is_none());
    }

    #[test]
    fn a_tool_call_and_its_result_become_two_turns() {
        let mut asking = ChatMessage::assistant("Let me look.");
        asking.tool_calls = vec![ToolCall {
            id: "toolu_1".into(),
            name: "read_card".into(),
            arguments: json!({"card": 2}),
        }];
        let request = CompletionRequest::new(
            "a-model",
            vec![
                ChatMessage::user("what is on card 2?"),
                asking,
                ChatMessage::tool_result("toolu_1", "a recipe"),
            ],
        );

        assert_eq!(
            provider().completion_body(&request)["messages"],
            json!([
                {"role": "user", "content": [{"type": "text", "text": "what is on card 2?"}]},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "Let me look."},
                    {"type": "tool_use", "id": "toolu_1", "name": "read_card", "input": {"card": 2}},
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "toolu_1", "content": "a recipe"},
                ]},
            ])
        );
    }

    #[test]
    fn several_tool_results_collapse_into_one_turn() {
        let request = CompletionRequest::new(
            "a-model",
            vec![
                ChatMessage::tool_result("toolu_1", "one"),
                ChatMessage::tool_result("toolu_2", "two"),
            ],
        );
        let messages = provider().completion_body(&request)["messages"].clone();
        assert_eq!(messages.as_array().map(Vec::len), Some(1));
        assert_eq!(messages[0]["content"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn an_empty_turn_is_left_out_rather_than_refused() {
        let request = CompletionRequest::new(
            "a-model",
            vec![ChatMessage::user("hi"), ChatMessage::assistant("")],
        );
        let messages = provider().completion_body(&request)["messages"].clone();
        assert_eq!(messages.as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn tools_are_declared_flat() {
        let request =
            CompletionRequest::new("a-model", vec![ChatMessage::user("hi")]).with_tools(vec![
                ToolDefinition::new("read_card", "Reads a card.", json!({"type": "object"})),
            ]);
        assert_eq!(
            provider().completion_body(&request)["tools"],
            json!([{
                "name": "read_card",
                "description": "Reads a card.",
                "input_schema": {"type": "object"},
            }])
        );
    }

    #[test]
    fn text_blocks_are_joined_into_one_reply() {
        let completion = decode_completion(&json!({
            "content": [
                {"type": "thinking", "thinking": "hmm"},
                {"type": "text", "text": "Hello"},
                {"type": "text", "text": ", world"},
            ],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 12, "output_tokens": 3},
        }))
        .unwrap();
        assert_eq!(completion.content, "Hello, world");
        assert_eq!(completion.finish_reason, FinishReason::Stop);
        assert_eq!(
            completion.usage,
            Some(Usage {
                input_tokens: 12,
                output_tokens: 3
            })
        );
    }

    #[test]
    fn a_tool_use_block_is_read_with_its_input_as_json() {
        let completion = decode_completion(&json!({
            "content": [{
                "type": "tool_use",
                "id": "toolu_1",
                "name": "read_card",
                "input": {"card": 2},
            }],
            "stop_reason": "tool_use",
        }))
        .unwrap();
        assert_eq!(completion.finish_reason, FinishReason::ToolUse);
        assert_eq!(completion.tool_calls[0].id, "toolu_1");
        assert_eq!(completion.tool_calls[0].arguments, json!({"card": 2}));
    }

    #[test]
    fn every_reason_for_stopping_is_understood() {
        assert_eq!(finish_reason(Some("max_tokens"), &[]), FinishReason::Length);
        assert_eq!(finish_reason(Some("refusal"), &[]), FinishReason::Filtered);
        assert_eq!(
            finish_reason(Some("stop_sequence"), &[]),
            FinishReason::Stop
        );
        assert_eq!(finish_reason(Some("pause_turn"), &[]), FinishReason::Stop);
        assert_eq!(
            finish_reason(Some("something new"), &[]),
            FinishReason::Stop
        );
    }

    #[test]
    fn a_reply_with_no_content_is_a_protocol_error() {
        assert!(matches!(
            decode_completion(&json!({"type": "message"})),
            Err(AiError::Protocol(_))
        ));
    }

    #[test]
    fn the_apis_own_message_is_preferred() {
        assert_eq!(
            describe_error(&json!({
                "type": "error",
                "error": {"type": "invalid_request_error", "message": "max_tokens: required"},
            }))
            .as_deref(),
            Some("max_tokens: required")
        );
    }
}
