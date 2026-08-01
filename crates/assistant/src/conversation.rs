//! The conversation, and what the user sees of it.
//!
//! Two records are kept, deliberately not one. [`ChatMessage`]s are what goes
//! on the wire; [`Entry`]s are what a person reads. They diverge — a tool
//! result is a wall of JSON on the wire and one line in the panel — and
//! pretending otherwise would mean showing someone either too much or a
//! polite fiction.

use hyperlab_ai::{ChatMessage, Completion, CompletionRequest, ToolDefinition};
use serde::Serialize;

use crate::{briefing::Briefing, briefing::SYSTEM_PROMPT, tools::ToolOutcome};

/// The most rounds of tool calls one question may set off.
///
/// An assistant that keeps calling tools without answering is stuck, and
/// every round costs the user money and time. Ten is far more than any real
/// request needs.
pub const MAX_ROUNDS: usize = 10;

/// One thing that happened, as the user sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Entry {
    /// The user asked something, and this is what went with it.
    Question {
        /// What they asked.
        text: String,
        /// Exactly what was sent alongside.
        briefing: Briefing,
    },
    /// The assistant answered.
    Answer {
        /// What it said.
        text: String,
    },
    /// The assistant used a tool.
    Used {
        /// Which one.
        tool: String,
        /// What it asked the tool to do, in one line.
        arguments: String,
        /// Whether it was allowed to.
        allowed: bool,
        /// What came back, or why it did not.
        outcome: String,
    },
    /// Something went wrong.
    Failed {
        /// What.
        reason: String,
    },
}

/// One exchange with a model, from the question to the answer.
#[derive(Debug, Clone, Default)]
pub struct Conversation {
    messages: Vec<ChatMessage>,
    entries: Vec<Entry>,
    rounds: usize,
}

impl Conversation {
    /// A conversation that has not started.
    #[must_use]
    pub fn new() -> Self {
        Self {
            messages: vec![ChatMessage::system(SYSTEM_PROMPT)],
            entries: Vec::new(),
            rounds: 0,
        }
    }

    /// Records a question, and the description of the stack that goes with it.
    ///
    /// The briefing is a separate message from the question so that a reader
    /// of the transcript can see where the user's words stop and HyperLab's
    /// description begins.
    pub fn ask(&mut self, question: &str, briefing: Briefing) {
        self.rounds = 0;
        self.messages.push(ChatMessage::user(briefing.as_message()));
        self.messages.push(ChatMessage::user(question.to_string()));
        self.entries.push(Entry::Question {
            text: question.to_string(),
            briefing,
        });
    }

    /// The request to send next.
    #[must_use]
    pub fn request(&self, model: &str, tools: Vec<ToolDefinition>) -> CompletionRequest {
        CompletionRequest::new(model, self.messages.clone()).with_tools(tools)
    }

    /// Records what the model said.
    ///
    /// The turn goes on the wire even when it is empty — a tool result has to
    /// answer something — but only words the user could read become an entry.
    pub fn record_reply(&mut self, completion: &Completion) {
        let mut turn = ChatMessage::assistant(completion.content.clone());
        turn.tool_calls = completion.tool_calls.clone();
        self.messages.push(turn);

        if !completion.content.trim().is_empty() {
            self.entries.push(Entry::Answer {
                text: completion.content.clone(),
            });
        }
    }

    /// Records what a tool did, for the model and for the user.
    pub fn record_tool(&mut self, outcome: &ToolOutcome) {
        self.messages.push(ChatMessage::tool_result(
            outcome.id.clone(),
            outcome.text.clone(),
        ));
        self.entries.push(Entry::Used {
            tool: outcome.tool.clone(),
            arguments: outcome.arguments.clone(),
            allowed: outcome.allowed,
            outcome: outcome.text.clone(),
        });
    }

    /// Records that the turn could not be finished.
    pub fn record_failure(&mut self, reason: impl Into<String>) {
        self.entries.push(Entry::Failed {
            reason: reason.into(),
        });
    }

    /// Counts a round of tool calls, and says whether to keep going.
    ///
    /// Returns `false` once [`MAX_ROUNDS`] have been spent on one question.
    pub fn begin_round(&mut self) -> bool {
        self.rounds += 1;
        self.rounds <= MAX_ROUNDS
    }

    /// Everything that has happened, oldest first.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The messages as the model sees them.
    #[must_use]
    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    /// Forgets everything, including the questions.
    pub fn clear(&mut self) {
        *self = Self::new();
    }
}

#[cfg(test)]
mod tests {
    use hyperlab_ai::{FinishReason, Role, ToolCall};
    use serde_json::json;

    use super::*;

    fn briefing() -> Briefing {
        Briefing {
            context: "# Stack \"Notes\"".into(),
            included_field_text: false,
            included_scripts: true,
        }
    }

    #[test]
    fn a_conversation_opens_with_its_instructions() {
        let conversation = Conversation::new();
        assert_eq!(conversation.messages().len(), 1);
        assert_eq!(conversation.messages()[0].role, Role::System);
    }

    #[test]
    fn the_users_words_are_kept_apart_from_hyperlabs_description() {
        let mut conversation = Conversation::new();
        conversation.ask("What is on this card?", briefing());

        let sent: Vec<&str> = conversation
            .messages()
            .iter()
            .map(|message| message.content.as_str())
            .collect();

        // Two user messages, and the last is exactly what was typed.
        assert_eq!(sent.last(), Some(&"What is on this card?"));
        assert!(sent[sent.len() - 2].contains("# Stack \"Notes\""));
    }

    #[test]
    fn a_reply_with_only_tool_calls_shows_nothing_to_the_user_yet() {
        let mut conversation = Conversation::new();
        conversation.ask("Add a field", briefing());

        conversation.record_reply(&Completion {
            content: String::new(),
            tool_calls: vec![ToolCall {
                id: "1".into(),
                name: "create_field".into(),
                arguments: json!({ "name": "Notes" }),
            }],
            finish_reason: FinishReason::ToolUse,
            usage: None,
        });

        // The empty turn still goes on the wire — the tool result has to
        // answer something — but there is nothing to read yet.
        assert_eq!(
            conversation.messages().last().unwrap().role,
            Role::Assistant
        );
        assert_eq!(conversation.entries().len(), 1, "only the question so far");
    }

    #[test]
    fn what_a_tool_did_is_shown_as_one_line_and_sent_as_its_answer() {
        let mut conversation = Conversation::new();
        conversation.record_tool(&ToolOutcome {
            id: "7".into(),
            tool: "write_field".into(),
            arguments: r#"{"name":"Title"}"#.into(),
            allowed: true,
            text: r#"{"written":true}"#.into(),
        });

        let last = conversation.messages().last().unwrap();
        assert_eq!(last.role, Role::Tool);
        assert_eq!(last.tool_call_id.as_deref(), Some("7"));

        assert!(matches!(
            conversation.entries().last(),
            Some(Entry::Used { tool, allowed: true, .. }) if tool == "write_field"
        ));
    }

    #[test]
    fn a_question_that_never_settles_is_stopped() {
        let mut conversation = Conversation::new();
        for _ in 0..MAX_ROUNDS {
            assert!(conversation.begin_round());
        }
        assert!(
            !conversation.begin_round(),
            "the {MAX_ROUNDS}th round is the last"
        );
    }

    #[test]
    fn asking_again_gives_the_new_question_a_full_budget() {
        let mut conversation = Conversation::new();
        for _ in 0..MAX_ROUNDS {
            conversation.begin_round();
        }
        conversation.ask("something else", briefing());
        assert!(conversation.begin_round(), "a new question starts over");
    }

    #[test]
    fn the_transcript_records_exactly_what_was_sent_with_each_question() {
        let mut conversation = Conversation::new();
        conversation.ask("hello", briefing());

        let Some(Entry::Question { briefing: kept, .. }) = conversation.entries().first() else {
            panic!("expected a question");
        };
        assert_eq!(kept, &briefing());
    }
}
