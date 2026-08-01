//! The one place the runtime waits for a person.
//!
//! `answer` and `ask` are modal: in HyperTalk the script stops until the
//! dialog is dismissed, and the answer to `ask` lands in `it` on the very
//! next line. That means the runtime has to *wait*, and waiting is the one
//! thing a message loop must never do.
//!
//! The resolution is that the runtime does not run on the message loop.
//! Commands do their work on a blocking thread (see [`commands`]), so this
//! [`Host`] can block that thread while the window, still responsive, shows
//! the dialog and sends the reply back through [`Dialogs::reply`].
//!
//! ```text
//! script → Host::ask → emit "hyperlab://dialog" ─────────► window
//!             (blocked)                                      │
//!          ◄──────────────── dialog_reply ◄──────────────────┘
//! ```
//!
//! [`commands`]: crate::commands

use std::{
    sync::{
        Arc, Mutex, PoisonError,
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    time::Duration,
};

use hyperlab_runtime::Host;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// The event the window listens for.
pub const DIALOG_EVENT: &str = "hyperlab://dialog";

/// How long a script will wait for an answer before giving up.
///
/// A person may reasonably take a while, so this is generous. It exists only
/// so that a dialog nobody can possibly answer — because the window went away
/// between the event and the reply — cannot freeze the application for ever.
const PATIENCE: Duration = Duration::from_secs(10 * 60);

/// A dialog the window is being asked to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DialogRequest {
    /// `answer "…"`: a message with an OK button.
    Answer {
        /// What to show.
        message: String,
    },
    /// `ask "…" with "…"`: a question with a text field.
    Ask {
        /// The question.
        prompt: String,
        /// What the field starts out holding.
        default: String,
    },
}

/// The dialog that is open, if one is.
///
/// Only one can be: the runtime is serialized behind a single lock, so only
/// one script runs at a time and only one script can be waiting.
#[derive(Debug, Default)]
pub struct Dialogs {
    waiting: Mutex<Option<SyncSender<Option<String>>>>,
}

impl Dialogs {
    /// Nothing open yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens a dialog, returning the channel the script waits on.
    ///
    /// If something was already open, it is cancelled: the script that asked
    /// it is no longer there to hear the answer.
    fn open(&self) -> Receiver<Option<String>> {
        let (sender, receiver) = sync_channel(1);
        if let Some(stale) = self.lock().replace(sender) {
            let _ = stale.send(None);
        }
        receiver
    }

    /// Delivers a reply. `None` means the person cancelled.
    ///
    /// Returns `false` if nothing was waiting, which the window uses to
    /// notice a dialog it dismissed twice.
    pub fn reply(&self, text: Option<String>) -> bool {
        match self.lock().take() {
            Some(sender) => sender.send(text).is_ok(),
            None => false,
        }
    }

    /// Cancels whatever is open, as when the window closes.
    pub fn cancel(&self) {
        self.reply(None);
    }

    /// A poisoned lock means a thread panicked while holding it. There is
    /// nothing to corrupt here — one optional channel — so carrying on beats
    /// taking the application down with it.
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<SyncSender<Option<String>>>> {
        self.waiting.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A [`Host`] that puts the runtime's questions on the screen.
pub struct DesktopHost {
    app: AppHandle,
    dialogs: Arc<Dialogs>,
}

impl DesktopHost {
    /// Builds a host that talks to this window.
    #[must_use]
    pub fn new(app: AppHandle, dialogs: Arc<Dialogs>) -> Self {
        Self { app, dialogs }
    }

    /// Shows a dialog and waits for the reply.
    fn show(&self, request: &DialogRequest) -> Option<String> {
        let receiver = self.dialogs.open();
        if self.app.emit(DIALOG_EVENT, request).is_err() {
            // There is no window to ask. Behave like a host that cannot ask
            // anything rather than waiting for an answer that cannot come.
            self.dialogs.cancel();
            return None;
        }
        match receiver.recv_timeout(PATIENCE) {
            Ok(reply) => reply,
            Err(_) => {
                self.dialogs.cancel();
                None
            }
        }
    }
}

impl Host for DesktopHost {
    fn answer(&mut self, message: &str) {
        self.show(&DialogRequest::Answer {
            message: message.to_string(),
        });
    }

    fn ask(&mut self, prompt: &str, default: &str) -> Option<String> {
        self.show(&DialogRequest::Ask {
            prompt: prompt.to_string(),
            default: default.to_string(),
        })
    }

    // `beep` needs no dialog: the recorded effect already tells the window.
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{thread, time::Duration};

    #[test]
    fn a_reply_reaches_the_script_that_is_waiting() {
        let dialogs = Arc::new(Dialogs::new());
        let receiver = dialogs.open();

        let answering = Arc::clone(&dialogs);
        let replier = thread::spawn(move || {
            // Give the waiter a moment to actually be waiting.
            thread::sleep(Duration::from_millis(20));
            answering.reply(Some("Grace".into()))
        });

        assert_eq!(receiver.recv().unwrap(), Some("Grace".into()));
        assert!(replier.join().unwrap(), "someone was waiting");
    }

    #[test]
    fn cancelling_unblocks_the_script() {
        let dialogs = Dialogs::new();
        let receiver = dialogs.open();
        dialogs.cancel();
        assert_eq!(receiver.recv().unwrap(), None);
    }

    #[test]
    fn replying_when_nothing_is_open_is_harmless() {
        let dialogs = Dialogs::new();
        assert!(!dialogs.reply(Some("nobody asked".into())));
    }

    #[test]
    fn a_second_dialog_cancels_the_first() {
        let dialogs = Dialogs::new();
        let first = dialogs.open();
        let second = dialogs.open();

        assert_eq!(
            first.recv().unwrap(),
            None,
            "the stale question is cancelled"
        );
        dialogs.reply(Some("here".into()));
        assert_eq!(second.recv().unwrap(), Some("here".into()));
    }
}
