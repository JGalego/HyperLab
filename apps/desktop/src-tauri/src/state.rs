//! What the application holds while it is running.

use std::{
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};

use hyperlab_runtime::Runtime;
use hyperlab_stack::Stack;

/// The application's whole state: one runtime, and where it came from.
///
/// The window is a view onto this and nothing else. There is no second copy
/// of the stack in the frontend, so there is nothing to keep in step.
pub struct AppState {
    runtime: Mutex<Session>,
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
            runtime: Mutex::new(Session {
                runtime: Runtime::new(Stack::new("Untitled")),
                path: None,
                dirty: false,
            }),
        }
    }

    /// Borrows the session.
    ///
    /// A poisoned lock means a command panicked while holding it. Rather than
    /// panicking again, the state is taken as it stands: the alternative is
    /// an application that cannot be closed without losing work.
    pub fn session(&self) -> MutexGuard<'_, Session> {
        self.runtime
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
