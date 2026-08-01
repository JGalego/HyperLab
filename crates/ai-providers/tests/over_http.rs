//! What actually goes over the wire.
//!
//! Each provider's own tests check the JSON it builds and the JSON it reads.
//! These check the round trip: the path, the headers, and the reply coming
//! back through [`reqwest`] into a [`Completion`]. The server is a plain
//! [`TcpListener`](std::net::TcpListener) on loopback — see [`support`] — so
//! nothing here touches a network or a vendor.

mod support;

use hyperlab_ai::{
    AiError, AiProvider, ChatMessage, CompletionRequest, FinishReason, ProviderConfig,
    ProviderKind, ToolDefinition,
};
use hyperlab_ai_providers::{AnthropicProvider, OpenAiProvider, build};
use serde_json::json;

use support::MockServer;

/// The key these tests send, so the assertions can look for it by name.
const KEY: &str = "test-key";

/// A configuration pointing at a test server.
///
/// The key is passed in separately rather than through the environment:
/// setting a variable is `unsafe` in a threaded process, and this crate
/// forbids `unsafe`. Reading one is covered by each provider's own tests.
fn config(kind: ProviderKind, base_url: &str) -> ProviderConfig {
    let mut config = ProviderConfig::new(kind, "a-model");
    config.base_url = Some(base_url.to_string());
    config
}

#[tokio::test]
async fn an_openai_request_arrives_where_it_should() {
    let server = MockServer::replying(
        &json!({
            "choices": [{
                "message": {"role": "assistant", "content": "Hello from the stack."},
                "finish_reason": "stop",
            }],
            "usage": {"prompt_tokens": 11, "completion_tokens": 4},
        })
        .to_string(),
    );

    let provider = OpenAiProvider::with_api_key(
        "test",
        &config(ProviderKind::OpenAiCompatible, server.base_url()),
        Some(KEY.into()),
    )
    .expect("the key can be sent");
    let completion = provider
        .complete(CompletionRequest::new(
            "a-model",
            vec![ChatMessage::user("hello")],
        ))
        .await
        .expect("the server said yes");

    assert_eq!(completion.content, "Hello from the stack.");
    assert_eq!(completion.finish_reason, FinishReason::Stop);
    assert_eq!(completion.usage.map(|usage| usage.input_tokens), Some(11));

    let request = server.only_request();
    assert_eq!(request.path, "/chat/completions");
    assert_eq!(
        request.header("authorization").unwrap(),
        format!("Bearer {KEY}")
    );
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(request.body["messages"][0]["content"], "hello");
}

#[tokio::test]
async fn an_openai_embedding_request_arrives_where_it_should() {
    let server = MockServer::replying(
        &json!({"data": [{"index": 0, "embedding": [0.5, -0.25]}]}).to_string(),
    );

    let provider = OpenAiProvider::with_api_key(
        "test",
        &config(ProviderKind::OpenAiCompatible, server.base_url()),
        Some(KEY.into()),
    )
    .expect("the key can be sent")
    .with_embedding_model("an-embedding-model");
    let embeddings = provider
        .embed(vec!["a card".into()])
        .await
        .expect("the server said yes");

    assert_eq!(embeddings[0].values, vec![0.5, -0.25]);

    let request = server.only_request();
    assert_eq!(request.path, "/embeddings");
    assert_eq!(request.body["model"], "an-embedding-model");
    assert_eq!(request.body["input"], json!(["a card"]));
}

#[tokio::test]
async fn an_anthropic_request_carries_the_headers_the_api_wants() {
    let server = MockServer::replying(
        &json!({
            "type": "message",
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": "toolu_1",
                "name": "read_card",
                "input": {"card": 2},
            }],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 30, "output_tokens": 8},
        })
        .to_string(),
    );

    let provider = AnthropicProvider::with_api_key(
        "test",
        &config(ProviderKind::Anthropic, server.base_url()),
        Some(KEY.into()),
    )
    .expect("the key can be sent");
    let completion = provider
        .complete(
            CompletionRequest::new(
                "a-model",
                vec![
                    ChatMessage::system("be brief"),
                    ChatMessage::user("what is on card 2?"),
                ],
            )
            .with_tools(vec![ToolDefinition::new(
                "read_card",
                "Reads a card.",
                json!({"type": "object"}),
            )]),
        )
        .await
        .expect("the server said yes");

    assert_eq!(completion.finish_reason, FinishReason::ToolUse);
    assert_eq!(completion.tool_calls[0].name, "read_card");
    assert_eq!(completion.tool_calls[0].arguments, json!({"card": 2}));

    let request = server.only_request();
    assert_eq!(request.path, "/messages");
    assert_eq!(request.header("x-api-key"), Some(KEY));
    assert_eq!(request.header("anthropic-version"), Some("2023-06-01"));
    assert_eq!(request.body["system"], "be brief");
    assert!(
        request.body["max_tokens"].is_number(),
        "the API refuses a request without one"
    );
    assert_eq!(request.body["tools"][0]["input_schema"]["type"], "object");
}

#[tokio::test]
async fn anthropic_still_cannot_embed() {
    let server = MockServer::answering(Vec::new());
    let provider = AnthropicProvider::with_api_key(
        "test",
        &config(ProviderKind::Anthropic, server.base_url()),
        Some(KEY.into()),
    )
    .expect("the key can be sent");

    let error = provider
        .embed(vec!["a card".into()])
        .await
        .expect_err("there is no embeddings endpoint");
    assert!(matches!(error, AiError::Unsupported(_)), "{error}");
    assert!(server.received().is_empty(), "nothing should be sent");
}

#[tokio::test]
async fn a_refused_request_reports_the_providers_own_words() {
    let server = MockServer::answering(vec![(
        401,
        json!({
            "type": "error",
            "error": {"type": "authentication_error", "message": "invalid x-api-key"},
        })
        .to_string(),
    )]);

    let provider = AnthropicProvider::with_api_key(
        "test",
        &config(ProviderKind::Anthropic, server.base_url()),
        Some(KEY.into()),
    )
    .expect("the key can be sent");
    let error = provider
        .complete(CompletionRequest::new(
            "a-model",
            vec![ChatMessage::user("hello")],
        ))
        .await
        .expect_err("the server said no");

    assert_eq!(
        error,
        AiError::NotConfigured("invalid x-api-key".into()),
        "a bad key is something the user can fix, not a network fault"
    );
}

#[tokio::test]
async fn an_error_page_that_is_not_json_still_makes_a_readable_message() {
    let server = MockServer::answering(vec![(502, "<html><body>Bad gateway</body></html>".into())]);

    let provider = OpenAiProvider::with_api_key(
        "test",
        &config(ProviderKind::OpenAiCompatible, server.base_url()),
        Some(KEY.into()),
    )
    .expect("the key can be sent");
    let error = provider
        .complete(CompletionRequest::new(
            "a-model",
            vec![ChatMessage::user("hello")],
        ))
        .await
        .expect_err("the proxy said no");

    let AiError::Transport(message) = &error else {
        panic!("a gateway failure is worth retrying: {error}");
    };
    assert!(message.contains("Bad gateway"), "{message}");
}

#[tokio::test]
async fn a_provider_built_from_a_configuration_reaches_the_same_server() {
    let server = MockServer::replying(
        &json!({"choices": [{"message": {"content": "hi"}, "finish_reason": "stop"}]}).to_string(),
    );

    let provider = build("mine", &config(ProviderKind::Ollama, server.base_url()))
        .expect("a base URL and a key are all it needs");
    let completion = provider
        .complete(CompletionRequest::new("", vec![ChatMessage::user("hello")]))
        .await
        .expect("the server said yes");

    assert_eq!(completion.content, "hi");
    assert_eq!(
        server.only_request().body["model"],
        "a-model",
        "a request that names no model uses the configured one"
    );
}
