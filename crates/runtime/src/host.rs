//! The boundary between the runtime and the outside world.
//!
//! Scripts can beep, show dialogs and ask questions. The runtime must not
//! know how any of that is done — that is the shell's business — so it
//! records an [`Effect`] for everything it wants the world to do, and asks a
//! [`Host`] for the answers it needs back.
//!
//! The desktop app replays the effects after a handler finishes. Tests read
//! them directly. Nothing about the runtime changes between the two.

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
}

/// A host that does nothing and answers nothing.
///
/// This is the right host for tests, for headless automation and for the
/// desktop app, which replays the recorded effects on the UI thread instead
/// of blocking the runtime.
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
}
