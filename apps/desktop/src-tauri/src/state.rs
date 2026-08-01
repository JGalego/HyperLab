//! What the application holds while it is running.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex, PoisonError},
};

use hyperlab_runtime::{Host, Runtime};
use hyperlab_stack::Stack;

use crate::dialogs::Dialogs;

/// The application's whole state: one runtime, and where it came from.
///
/// The window is a view onto this and nothing else. There is no second copy
/// of the stack in the frontend, so there is nothing to keep in step.
///
/// The session sits behind an [`Arc`] so that commands can take it onto a
/// blocking thread. That is not an implementation detail to work around: it
/// is what lets a script wait for a dialog without freezing the window.
pub struct AppState {
    session: Arc<Mutex<Session>>,
    dialogs: Arc<Dialogs>,
}

/// One open document.
pub struct Session {
    /// The runtime, which owns the stack.
    pub runtime: Runtime,
    /// Where it is saved, once it has been.
    pub path: Option<PathBuf>,
    /// Whether anything has changed since the last save.
    pub dirty: bool,
}

impl Session {
    /// Notes that the document has changed.
    pub fn touch(&mut self) {
        self.dirty = true;
    }

    /// The path as a string, for the frontend.
    #[must_use]
    pub fn path_string(&self) -> Option<String> {
        self.path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
    }
}

impl AppState {
    /// Starts with an untitled stack, so the window always has something in
    /// it. HyperCard opened on a stack; so does HyperLab.
    #[must_use]
    pub fn new() -> Self {
        Self {
            session: Arc::new(Mutex::new(Session {
                runtime: Runtime::new(Stack::new("Untitled")),
                path: None,
                dirty: false,
            })),
            dialogs: Arc::new(Dialogs::new()),
        }
    }

    /// A handle to the session, for a command about to lock it on a blocking
    /// thread.
    #[must_use]
    pub fn session(&self) -> Arc<Mutex<Session>> {
        Arc::clone(&self.session)
    }

    /// The open dialog, if there is one.
    #[must_use]
    pub fn dialogs(&self) -> Arc<Dialogs> {
        Arc::clone(&self.dialogs)
    }

    /// Gives the runtime a host that can show dialogs.
    ///
    /// This happens during setup rather than in [`AppState::new`], because a
    /// host needs a window to talk to and there is not one yet.
    pub fn install_host(&self, host: Box<dyn Host>) {
        lock(&self.session).runtime.set_host(host);
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Locks the session.
///
/// A poisoned lock means a command panicked while holding it. Rather than
/// panicking again, the state is taken as it stands: the alternative is an
/// application that cannot be closed without losing work.
pub fn lock(session: &Mutex<Session>) -> std::sync::MutexGuard<'_, Session> {
    session.lock().unwrap_or_else(PoisonError::into_inner)
}
