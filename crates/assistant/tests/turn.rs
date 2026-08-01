//! A whole turn, driven the way the desktop drives it.
//!
//! The point of these is the *loop*: ask, get tool calls back, run them, ask
//! again with the results, get words. Every unit test covers one step; this
//! covers the seam between them, which is where a turn actually goes wrong.

use hyperlab_ai::{
    AiProvider, ChatMessage, Completion, ContextOptions, FinishReason, MockProvider, Role, ToolCall,
};
use hyperlab_assistant::{Briefing, Conversation, Entry, tools};
use hyperlab_mcp::{AllowAll, DenyAll, Policy, ToolRegistry};
use hyperlab_runtime::Runtime;
use hyperlab_stack::{PartContainer, Stack};
use serde_json::json;

/// Runs a future to completion. The mock is always ready, so this never spins.
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::task::{Context, Poll, Waker};
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = Box::pin(future);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}

fn asking_for(tool: &str, arguments: serde_json::Value) -> Completion {
    Completion {
        content: String::new(),
        tool_calls: vec![ToolCall {
            id: format!("call-{tool}"),
            name: tool.to_string(),
            arguments,
        }],
        finish_reason: FinishReason::ToolUse,
        usage: None,
    }
}

/// Drives a turn to its end, exactly as the shell does.
fn take_turn(
    runtime: &mut Runtime,
    conversation: &mut Conversation,
    provider: &MockProvider,
    policy: &mut Policy,
    approver: &mut dyn hyperlab_mcp::Approver,
) {
    let registry = ToolRegistry::new();
    while conversation.begin_round() {
        let request = conversation.request("mock", registry.definitions());
        // In the shell this is where the session lock is not held.
        let completion = block_on(provider.complete(request)).expect("the mock answers");
        conversation.record_reply(&completion);

        if completion.tool_calls.is_empty() {
            return;
        }
        for outcome in tools::run(runtime, &registry, policy, approver, &completion.tool_calls) {
            conversation.record_tool(&outcome);
        }
    }
    conversation.record_failure("the assistant kept going without answering");
}

#[test]
fn a_question_that_needs_a_tool_runs_it_and_then_answers() {
    let mut runtime = Runtime::new(Stack::new("Notes"));
    let provider = MockProvider::new("mock").with_replies(vec![
        asking_for("create_field", json!({ "name": "Summary" })),
        Completion::text("I added a field called Summary."),
    ]);

    let mut conversation = Conversation::new();
    conversation.ask(
        "Add a summary field",
        Briefing::about(&runtime, ContextOptions::default()),
    );
    take_turn(
        &mut runtime,
        &mut conversation,
        &provider,
        &mut Policy::trusted(),
        &mut AllowAll,
    );

    // The stack really changed, through the command bus.
    let card = runtime.stack().cards()[0].clone();
    assert_eq!(card.parts().len(), 1);
    assert!(runtime.undo().unwrap(), "and it undoes like anything else");

    // And the user can read what happened, in order.
    let kinds: Vec<&str> = conversation
        .entries()
        .iter()
        .map(|entry| match entry {
            Entry::Question { .. } => "question",
            Entry::Answer { .. } => "answer",
            Entry::Used { .. } => "used",
            Entry::Failed { .. } => "failed",
        })
        .collect();
    assert_eq!(kinds, ["question", "used", "answer"]);
}

#[test]
fn what_the_tool_said_reaches_the_model() {
    let mut runtime = Runtime::new(Stack::new("Notes"));
    let provider = MockProvider::new("mock").with_replies(vec![
        asking_for("list_cards", json!({})),
        Completion::text("There is one card."),
    ]);

    let mut conversation = Conversation::new();
    conversation.ask(
        "How many cards?",
        Briefing::about(&runtime, ContextOptions::default()),
    );
    take_turn(
        &mut runtime,
        &mut conversation,
        &provider,
        &mut Policy::trusted(),
        &mut AllowAll,
    );

    // The second request carries the tool's answer, attached to the call it
    // answers. Without the id the provider rejects the whole request.
    let second = &provider.requests()[1];
    let result = second
        .messages
        .iter()
        .find(|message| message.role == Role::Tool)
        .expect("the tool result was sent back");
    assert_eq!(result.tool_call_id.as_deref(), Some("call-list_cards"));
    assert!(result.content.contains("cards"));
}

#[test]
fn a_refused_tool_leaves_the_stack_alone_and_the_model_is_told_why() {
    let mut runtime = Runtime::new(Stack::new("Notes"));
    let provider = MockProvider::new("mock").with_replies(vec![
        asking_for("create_card", json!({})),
        Completion::text("I am not allowed to do that."),
    ]);

    let mut conversation = Conversation::new();
    conversation.ask(
        "Add a card",
        Briefing::about(&runtime, ContextOptions::default()),
    );
    take_turn(
        &mut runtime,
        &mut conversation,
        &provider,
        // Read-only, and nobody to ask.
        &mut Policy::new(),
        &mut DenyAll,
    );

    assert_eq!(runtime.stack().card_count(), 1, "nothing was created");
    assert!(matches!(
        conversation.entries().get(1),
        Some(Entry::Used { allowed: false, .. })
    ));

    let requests = provider.requests();
    let told: &ChatMessage = requests[1]
        .messages
        .iter()
        .find(|message| message.role == Role::Tool)
        .expect("the refusal was sent back");
    assert!(told.content.contains("Not allowed"), "got {}", told.content);
}

#[test]
fn an_assistant_that_never_answers_is_stopped_and_says_so() {
    let mut runtime = Runtime::new(Stack::new("Notes"));
    // Always another tool call, never an answer.
    let provider = MockProvider::new("mock").with_replies(
        (0..hyperlab_assistant::MAX_ROUNDS + 5)
            .map(|_| asking_for("list_cards", json!({})))
            .collect(),
    );

    let mut conversation = Conversation::new();
    conversation.ask(
        "Go forever",
        Briefing::about(&runtime, ContextOptions::default()),
    );
    take_turn(
        &mut runtime,
        &mut conversation,
        &provider,
        &mut Policy::trusted(),
        &mut AllowAll,
    );

    assert_eq!(provider.requests().len(), hyperlab_assistant::MAX_ROUNDS);
    assert!(matches!(
        conversation.entries().last(),
        Some(Entry::Failed { .. })
    ));
}

#[test]
fn field_contents_are_not_sent_unless_the_user_asked_for_them() {
    let mut runtime = Runtime::new(Stack::new("Notes"));
    let registry = ToolRegistry::new();
    registry
        .call(&mut runtime, "create_field", &json!({ "name": "Diary" }))
        .unwrap();
    registry
        .call(
            &mut runtime,
            "write_field",
            &json!({ "name": "Diary", "text": "something private" }),
        )
        .unwrap();

    let provider = MockProvider::new("mock").with_replies(vec![Completion::text("Noted.")]);
    let mut conversation = Conversation::new();
    conversation.ask(
        "What is this card for?",
        Briefing::about(&runtime, ContextOptions::default()),
    );
    take_turn(
        &mut runtime,
        &mut conversation,
        &provider,
        &mut Policy::trusted(),
        &mut AllowAll,
    );

    let everything_sent = provider.requests()[0]
        .messages
        .iter()
        .map(|message| message.content.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !everything_sent.contains("something private"),
        "field contents left the machine without being asked for"
    );
    assert!(
        everything_sent.contains("Diary"),
        "the field's name is fine to send"
    );
}
