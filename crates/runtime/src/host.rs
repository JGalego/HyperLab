//! The boundary between the runtime and the outside world.
//!
//! Scripts can beep, show dialogs and ask questions. The runtime must not
//! know how any of that is done — that is the shell's business — so it
//! records an [`Effect`] for everything it wants the world to do, and asks a
//! [`Host`] for the answers it needs back.
//!
//! The two serve different purposes, and both are needed:
//!
//! * The **host** is called *while the script runs*. It may block: `answer`
//!   and `ask` are modal, and the answer to `ask` has to reach the next line.
//! * The **effects** are a record, read once the handler has finished. They
//!   are how a caller with no window — a test, an MCP tool — finds out what a
//!   script did.
//!
//! A host that cannot ask anything ([`SilentHost`]) cancels every question,
//! which is the behaviour every script must already cope with.

use hyperlab_stack::Id;
use serde::{Deserialize, Serialize};

/// Something a script asked the world to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Effect {
    /// `answer "…"`: show a message.
    Answer {
        /// What to show.
        message: String,
    },
    /// `ask "…" with "…"`: ask for a line of text.
    Ask {
        /// The question.
        prompt: String,
        /// The suggested answer.
        default: String,
    },
    /// `beep`.
    Beep,
    /// `wait <n> ticks`, honoured by the shell if it wants to.
    Wait {
        /// How long, in sixtieths of a second.
        ticks: f64,
    },
    /// The current card changed.
    Navigated {
        /// The card now showing.
        card: Id,
    },
    /// Something was written to the message box.
    MessageBox {
        /// The new contents.
        text: String,
    },
    /// A script asked a language model something.
    ///
    /// Recorded whether or not an answer came back, because the question
    /// having been asked is the part a person needs to be able to see.
    Assistant {
        /// What the script asked, word for word.
        prompt: String,
        /// What it was allowed to do about it.
        intent: AiIntent,
    },
}

/// What a script is asking a language model to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AiIntent {
    /// `ai("…")`: answer in words, and change nothing.
    Answer,
    /// `ask assistant "…"`: the assistant may change the stack while it
    /// answers — through commands, so the change is undoable like any other.
    Edit,
}

/// A question a script put to a language model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiRequest {
    /// Exactly the words the script used, with nothing added.
    ///
    /// Wrapping these in a prompt — deciding what the model is told about
    /// the stack, and in what order — is the shell's business. The runtime
    /// does not know what a prompt looks like and must not learn.
    pub prompt: String,
    /// Whether answering may change the stack.
    pub intent: AiIntent,
}

impl AiRequest {
    /// A question that must be answered without touching the stack.
    #[must_use]
    pub fn answer(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            intent: AiIntent::Answer,
        }
    }

    /// A request the assistant may act on.
    #[must_use]
    pub fn edit(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            intent: AiIntent::Edit,
        }
    }
}

/// Answers the questions a script asks.
///
/// Every method has a default, so a host only implements what it can do.
///
/// A host must be [`Send`], because the [`Runtime`](crate::Runtime) that owns
/// it is: the desktop shell keeps one in shared state, and anything that
/// serves stacks over a socket will too.
pub trait Host: Send {
    /// Shows a message. The default does nothing; the effect was recorded
    /// either way.
    fn answer(&mut self, message: &str) {
        let _ = message;
    }

    /// Asks for a line of text. Returning `None` means "the user cancelled",
    /// which is what a host that cannot ask anything should report.
    fn ask(&mut self, prompt: &str, default: &str) -> Option<String> {
        let _ = (prompt, default);
        None
    }

    /// Makes a noise.
    fn beep(&mut self) {}

    /// Asks a language model something on a script's behalf.
    ///
    /// Blocking, for the same reason [`ask`](Host::ask) is: `ai("…")` is an
    /// expression, and its value has to reach the rest of the line.
    ///
    /// The error is a sentence to show someone, not a type to match on. The
    /// runtime cannot know what went wrong — it does not depend on any AI
    /// crate, and that arrow must keep pointing the way it does — so it
    /// passes the words through and attaches the script line.
    ///
    /// The default refuses. HyperLab works with no provider configured, and
    /// always will, so refusing is the ordinary case rather than a failure.
    fn ai(&mut self, request: &AiRequest) -> Result<String, String> {
        let _ = request;
        Err("no assistant is set up".to_string())
    }
}

/// A host that does nothing and answers nothing.
///
/// The right host for tests and for headless automation: every question is
/// cancelled and no assistant answers, which is behaviour every script has to
/// cope with anyway.
#[derive(Debug, Default, Clone, Copy)]
pub struct SilentHost;

impl Host for SilentHost {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_serialize_with_a_tag_the_ui_can_switch_on() {
        let json = serde_json::to_string(&Effect::Answer {
            message: "hi".into(),
        })
        .unwrap();
        assert_eq!(json, r#"{"kind":"answer","message":"hi"}"#);
    }

    #[test]
    fn the_silent_host_cancels_every_question() {
        assert_eq!(SilentHost.ask("Name?", "Bob"), None);
    }

    #[test]
    fn a_host_with_no_assistant_says_so_rather_than_pretending() {
        let refusal = SilentHost.ai(&AiRequest::answer("Summarize this card"));
        assert_eq!(refusal, Err("no assistant is set up".to_string()));
    }

    #[test]
    fn an_assistant_effect_records_what_was_asked_and_on_what_terms() {
        let json = serde_json::to_string(&Effect::Assistant {
            prompt: "Generate five cards".into(),
            intent: AiIntent::Edit,
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"kind":"assistant","prompt":"Generate five cards","intent":"edit"}"#
        );
    }
}
