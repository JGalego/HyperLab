//! Running the tools a model asked for.
//!
//! This is the half of a turn that touches the stack, and it is deliberately
//! synchronous: it wants `&mut Runtime`, the completion that asked for it
//! does not, and keeping them apart is what lets the caller hold the session
//! lock for the tool calls and drop it for the network.
//!
//! Nothing here reaches the stack directly. Every call goes through the same
//! [`ToolRegistry`] a person's menu commands would, and past a [`Policy`]
//! first, so an assistant can do exactly what the user can do and no more.

use hyperlab_ai::ToolCall;
use hyperlab_mcp::{Approver, Policy, ToolRegistry, Verdict};
use hyperlab_runtime::Runtime;
use hyperlab_stack::Object;

/// The longest tool answer that goes back to the model.
///
/// A tool that returns a whole stack would otherwise fill the context window
/// and cost the user for the privilege. Anything cut says so.
pub const MAX_RESULT: usize = 8_000;

/// What happened when a model asked for a tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutcome {
    /// The call this answers.
    pub id: String,
    /// The tool that was asked for.
    pub tool: String,
    /// Its arguments, as one line for a person to read.
    pub arguments: String,
    /// Whether it was allowed to run.
    pub allowed: bool,
    /// What came back, or why nothing did.
    pub text: String,
}

/// Runs every tool a completion asked for, in order.
///
/// A refusal or a failure is an outcome rather than an error: the model has
/// to be told, in a form it can act on, so that it can try something else or
/// explain itself to the user.
pub fn run(
    runtime: &mut Runtime,
    registry: &ToolRegistry,
    policy: &mut Policy,
    approver: &mut dyn Approver,
    calls: &[ToolCall],
) -> Vec<ToolOutcome> {
    calls
        .iter()
        .map(|call| run_one(runtime, registry, policy, approver, call))
        .collect()
}

fn run_one(
    runtime: &mut Runtime,
    registry: &ToolRegistry,
    policy: &mut Policy,
    approver: &mut dyn Approver,
    call: &ToolCall,
) -> ToolOutcome {
    let arguments = one_line(&call.arguments);
    let refused = |text: String| ToolOutcome {
        id: call.id.clone(),
        tool: call.name.clone(),
        arguments: arguments.clone(),
        allowed: false,
        text,
    };

    let Some(tool) = registry.get(&call.name) else {
        return refused(format!("There is no tool called \"{}\".", call.name));
    };

    let stack = runtime.stack().name().to_string();
    let decision = policy.decide(&call.name, tool.access, &stack, approver);
    if let Verdict::Refused { reason } = decision.verdict {
        return refused(format!("Not allowed: {reason}."));
    }

    match registry.call(runtime, &call.name, &call.arguments) {
        Ok(value) => ToolOutcome {
            id: call.id.clone(),
            tool: call.name.clone(),
            arguments,
            allowed: true,
            text: truncate(&one_line(&value)),
        },
        // The tool said no. That is news for the model, not a broken turn.
        Err(error) => ToolOutcome {
            id: call.id.clone(),
            tool: call.name.clone(),
            arguments,
            allowed: true,
            text: truncate(&error.to_string()),
        },
    }
}

/// Renders JSON as a single line.
fn one_line(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

/// Cuts a long answer, saying so rather than trailing off.
fn truncate(text: &str) -> String {
    if text.chars().count() <= MAX_RESULT {
        return text.to_string();
    }
    let kept: String = text.chars().take(MAX_RESULT).collect();
    let dropped = text.chars().count() - MAX_RESULT;
    format!("{kept}\n… ({dropped} characters omitted)")
}

#[cfg(test)]
mod tests {
    use hyperlab_mcp::{AllowAll, DenyAll};
    use hyperlab_stack::Stack;
    use serde_json::json;

    use super::*;

    fn call(name: &str, arguments: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "1".into(),
            name: name.into(),
            arguments,
        }
    }

    #[test]
    fn a_tool_the_policy_allows_changes_the_stack_and_can_be_undone() {
        let mut runtime = Runtime::new(Stack::new("Notes"));
        let mut policy = Policy::trusted();
        let before = runtime.stack().card_count();

        let outcomes = run(
            &mut runtime,
            &ToolRegistry::new(),
            &mut policy,
            &mut AllowAll,
            &[call("create_card", json!({}))],
        );

        assert!(outcomes[0].allowed);
        assert_eq!(runtime.stack().card_count(), before + 1);
        assert!(
            runtime.undo().unwrap(),
            "an assistant's change undoes like any other"
        );
    }

    #[test]
    fn a_refusal_is_told_to_the_model_rather_than_thrown() {
        let mut runtime = Runtime::new(Stack::new("Notes"));
        let mut policy = Policy::new();

        let outcomes = run(
            &mut runtime,
            &ToolRegistry::new(),
            &mut policy,
            &mut DenyAll,
            &[call("create_card", json!({}))],
        );

        assert!(!outcomes[0].allowed);
        assert!(
            outcomes[0].text.starts_with("Not allowed:"),
            "got {}",
            outcomes[0].text
        );
        assert_eq!(runtime.stack().card_count(), 1, "nothing happened");
    }

    #[test]
    fn a_tool_that_does_not_exist_is_answered_not_ignored() {
        let mut runtime = Runtime::new(Stack::new("Notes"));

        let outcomes = run(
            &mut runtime,
            &ToolRegistry::new(),
            &mut Policy::trusted(),
            &mut AllowAll,
            &[call("teleport", json!({}))],
        );

        // Every call must get an answer carrying its id, or the next request
        // is malformed and the whole turn fails.
        assert_eq!(outcomes[0].id, "1");
        assert!(outcomes[0].text.contains("no tool called"));
    }

    #[test]
    fn a_tool_that_fails_reports_why_and_the_turn_carries_on() {
        let mut runtime = Runtime::new(Stack::new("Notes"));

        let outcomes = run(
            &mut runtime,
            &ToolRegistry::new(),
            &mut Policy::trusted(),
            &mut AllowAll,
            &[call("read_field", json!({ "name": "Nope" }))],
        );

        assert!(outcomes[0].allowed, "it was allowed to try");
        assert!(!outcomes[0].text.is_empty());
    }

    #[test]
    fn every_call_is_answered_in_the_order_it_was_asked() {
        let mut runtime = Runtime::new(Stack::new("Notes"));
        let calls = vec![
            ToolCall {
                id: "a".into(),
                name: "list_cards".into(),
                arguments: json!({}),
            },
            ToolCall {
                id: "b".into(),
                name: "current_card".into(),
                arguments: json!({}),
            },
        ];

        let outcomes = run(
            &mut runtime,
            &ToolRegistry::new(),
            &mut Policy::trusted(),
            &mut AllowAll,
            &calls,
        );

        assert_eq!(
            outcomes.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
    }

    #[test]
    fn a_very_long_answer_is_cut_and_says_that_it_was() {
        let long = "x".repeat(MAX_RESULT + 100);
        let cut = truncate(&long);
        assert!(
            cut.contains("100 characters omitted"),
            "got the tail: {}",
            &cut[cut.len() - 40..]
        );
    }
}
