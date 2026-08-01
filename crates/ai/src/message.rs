//! What is said to a model, and what it says back.
//!
//! These types are the lowest common denominator of every provider worth
//! supporting. Anything one provider offers and another does not belongs in
//! that provider's own crate, not here: the moment this file grows a field
//! only one vendor understands, HyperLab has picked a vendor.

use serde::{Deserialize, Serialize};

/// Who is speaking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Instructions that frame the conversation.
    System,
    /// The person.
    User,
    /// The model.
    Assistant,
    /// The result of a tool the model asked for.
    Tool,
}

/// One turn of a conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Who is speaking.
    pub role: Role,
    /// What was said.
    pub content: String,
    /// Tools the assistant asked to use, when `role` is
    /// [`Role::Assistant`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// Which tool call this message answers, when `role` is [`Role::Tool`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    /// A plain message with no tool involvement.
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    /// A system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(Role::System, content)
    }

    /// A message from the person.
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(Role::User, content)
    }

    /// A message from the model.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(Role::Assistant, content)
    }

    /// The result of running a tool.
    pub fn tool_result(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: Some(id.into()),
        }
    }
}

/// A model asking for a tool to be run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Identifies this request, so the answer can be matched to it.
    pub id: String,
    /// Which tool.
    pub name: String,
    /// Its arguments, as JSON.
    pub arguments: serde_json::Value,
}

/// What a model is being asked to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionRequest {
    /// Which model, in the provider's own naming.
    pub model: String,
    /// The conversation so far.
    pub messages: Vec<ChatMessage>,
    /// Tools the model may ask for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<crate::tool::ToolDefinition>,
    /// How adventurous to be, from 0 to 1. `None` means the provider's own
    /// default, which is usually the right thing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// A ceiling on the reply length.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

impl CompletionRequest {
    /// A request with nothing but a model and a conversation.
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            tools: Vec::new(),
            temperature: None,
            max_tokens: None,
        }
    }

    /// Offers the model a set of tools.
    #[must_use]
    pub fn with_tools(mut self, tools: Vec<crate::tool::ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }
}

/// Why a model stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FinishReason {
    /// It finished what it had to say.
    Stop,
    /// It hit the token ceiling.
    Length,
    /// It wants a tool run before continuing.
    ToolUse,
    /// The provider stopped it.
    Filtered,
}

/// What a model said.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    /// The text of the reply.
    pub content: String,
    /// Any tools it wants run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// Why it stopped.
    pub finish_reason: FinishReason,
    /// What it cost, when the provider says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

impl Completion {
    /// A plain text reply.
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            tool_calls: Vec::new(),
            finish_reason: FinishReason::Stop,
            usage: None,
        }
    }
}

/// How many tokens a request used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    /// Tokens sent.
    pub input_tokens: u32,
    /// Tokens received.
    pub output_tokens: u32,
}

/// A vector representation of a piece of text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Embedding {
    /// The vector.
    pub values: Vec<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_serialize_without_empty_fields() {
        let json = serde_json::to_string(&ChatMessage::user("hi")).unwrap();
        assert_eq!(json, r#"{"role":"user","content":"hi"}"#);
    }

    #[test]
    fn a_tool_result_remembers_which_call_it_answers() {
        let message = ChatMessage::tool_result("call_1", "42");
        assert_eq!(message.role, Role::Tool);
        assert_eq!(message.tool_call_id.as_deref(), Some("call_1"));
    }
}
