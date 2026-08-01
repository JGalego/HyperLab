//! The runtime: the one thing that owns mutable state.

use std::collections::BTreeMap;

use hyperlab_stack::{Id, Object, ObjectId, ObjectKind, PropertyBag, Stack, StackError, Value};

use crate::{
    command::{Applied, Command},
    error::{RuntimeError, RuntimeResult},
    event::{Message, messages},
    history::History,
    host::{Effect, Host, SilentHost},
    interpreter::Interpreter,
};

/// Everything HyperLab knows at one moment: the stack, where we are in it,
/// what can be undone, and what scripts have left lying around.
///
/// Nothing else in the system owns mutable stack state. The UI asks the
/// runtime to run a [`Command`]; scripts do the same; so does the MCP layer.
/// One path in, one place to look when something changed.
pub struct Runtime {
    stack: Stack,
    current_card: Id,
    history: History,
    back_stack: Vec<Id>,
    globals: BTreeMap<String, Value>,
    message_box: String,
    result: Value,
    effects: Vec<Effect>,
    host: Box<dyn Host>,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime")
            .field("stack", &self.stack.name())
            .field("current_card", &self.current_card)
            .field("pending_effects", &self.effects.len())
            .finish_non_exhaustive()
    }
}

impl Runtime {
    /// Opens a stack, starting on its first card.
    #[must_use]
    pub fn new(stack: Stack) -> Self {
        Self::with_host(stack, Box::new(SilentHost))
    }

    /// Opens a stack with a host that can answer `ask`.
    #[must_use]
    pub fn with_host(stack: Stack, host: Box<dyn Host>) -> Self {
        let current_card = stack.cards()[0].id();
        Self {
            stack,
            current_card,
            history: History::new(),
            back_stack: Vec::new(),
            globals: BTreeMap::new(),
            message_box: String::new(),
            result: Value::Empty,
            effects: Vec::new(),
            host,
        }
    }

    // ---------------------------------------------------------------- state

    /// The stack, for reading. There is deliberately no mutable counterpart:
    /// changes go through [`Runtime::execute`].
    #[must_use]
    pub fn stack(&self) -> &Stack {
        &self.stack
    }

    /// Gives up ownership of the stack, for saving or for closing.
    #[must_use]
    pub fn into_stack(self) -> Stack {
        self.stack
    }

    /// Replaces the stack, as when a different document is opened. The
    /// history is cleared, because it describes a stack that is no longer
    /// here.
    pub fn open(&mut self, stack: Stack) {
        self.current_card = stack.cards()[0].id();
        self.stack = stack;
        self.history.clear();
        self.back_stack.clear();
        self.result = Value::Empty;
    }

    /// The card showing now.
    #[must_use]
    pub const fn current_card(&self) -> Id {
        self.current_card
    }

    /// The zero-based position of the current card.
    #[must_use]
    pub fn current_card_index(&self) -> usize {
        self.stack.card_index(self.current_card).unwrap_or(0)
    }

    /// The contents of the message box.
    #[must_use]
    pub fn message_box(&self) -> &str {
        &self.message_box
    }

    /// Sets the message box, recording the effect so the shell can show it.
    pub fn set_message_box(&mut self, text: impl Into<String>) {
        self.message_box = text.into();
        self.effects.push(Effect::MessageBox {
            text: self.message_box.clone(),
        });
    }

    /// What the last handler returned, which scripts read as `the result`.
    #[must_use]
    pub fn result(&self) -> &Value {
        &self.result
    }

    /// Sets `the result`.
    pub fn set_result(&mut self, value: Value) {
        self.result = value;
    }

    /// A global variable.
    #[must_use]
    pub fn global(&self, name: &str) -> Option<&Value> {
        self.globals.get(&PropertyBag::normalize(name))
    }

    /// Sets a global variable.
    pub fn set_global(&mut self, name: &str, value: Value) {
        self.globals.insert(PropertyBag::normalize(name), value);
    }

    /// Every global variable, for the inspector.
    pub fn globals(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.globals
            .iter()
            .map(|(name, value)| (name.as_str(), value))
    }

    /// Takes everything scripts asked the world to do since the last call.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    /// Records an effect. Used by the interpreter and by the shell.
    pub(crate) fn push_effect(&mut self, effect: Effect) {
        self.effects.push(effect);
    }

    /// Replaces the host.
    ///
    /// A shell usually cannot build its host until it has a window to talk
    /// to, which is later than it needs a runtime.
    pub fn set_host(&mut self, host: Box<dyn Host>) {
        self.host = host;
    }

    /// The host, for the interpreter's `answer` and `ask`.
    pub(crate) fn host_mut(&mut self) -> &mut dyn Host {
        self.host.as_mut()
    }

    // ------------------------------------------------------------- commands

    /// Runs a command and records how to undo it.
    ///
    /// # Errors
    ///
    /// Returns whatever the command failed with; the stack is unchanged.
    pub fn execute(&mut self, command: Command) -> RuntimeResult<Option<ObjectId>> {
        let undoable = command.is_undoable();
        let Applied { inverse, created } = command.apply(&mut self.stack)?;
        if undoable {
            self.history.record(inverse);
        }
        self.ensure_current_card_exists();
        Ok(created)
    }

    /// Runs a command without recording it, for changes that are not the
    /// user's to undo — loading, migration, and the inverse halves of undo
    /// and redo.
    fn execute_silently(&mut self, command: Command) -> RuntimeResult<Command> {
        let Applied { inverse, .. } = command.apply(&mut self.stack)?;
        self.ensure_current_card_exists();
        Ok(inverse)
    }

    /// Undoes the last change.
    ///
    /// # Errors
    ///
    /// Returns an error if the change cannot be reversed, which would mean
    /// the history and the stack had drifted apart.
    pub fn undo(&mut self) -> RuntimeResult<bool> {
        let Some(command) = self.history.take_undo() else {
            return Ok(false);
        };
        let inverse = self.execute_silently(command)?;
        self.history.push_redo(inverse);
        Ok(true)
    }

    /// Redoes the last undone change.
    ///
    /// # Errors
    ///
    /// As for [`Runtime::undo`].
    pub fn redo(&mut self) -> RuntimeResult<bool> {
        let Some(command) = self.history.take_redo() else {
            return Ok(false);
        };
        let inverse = self.execute_silently(command)?;
        self.history.push_undo(inverse);
        Ok(true)
    }

    /// The undo and redo stacks.
    #[must_use]
    pub const fn history(&self) -> &History {
        &self.history
    }

    /// If the current card has just been deleted, fall back to a card that
    /// still exists rather than leaving the runtime pointing at nothing.
    fn ensure_current_card_exists(&mut self) {
        if self.stack.card(self.current_card).is_none() {
            self.current_card = self.stack.cards()[0].id();
        }
    }

    // ----------------------------------------------------------- navigation

    /// Goes to a card, sending `closeCard` and `openCard`.
    ///
    /// # Errors
    ///
    /// Returns an error if there is no such card, or if a card script fails.
    pub fn go_to_card(&mut self, card: Id) -> RuntimeResult<()> {
        if self.stack.card(card).is_none() {
            return Err(StackError::NoSuchObject {
                kind: ObjectKind::Card,
                id: card,
            }
            .into());
        }
        Interpreter::new(self).navigate_to(card, true)
    }

    /// Goes to the card at a zero-based position, wrapping around the ends
    /// the way a stack of cards does.
    ///
    /// # Errors
    ///
    /// As for [`Runtime::go_to_card`].
    pub fn go_to_index(&mut self, index: isize) -> RuntimeResult<()> {
        let count = self.stack.card_count() as isize;
        let wrapped = index.rem_euclid(count);
        let card = self.stack.cards()[wrapped as usize].id();
        self.go_to_card(card)
    }

    /// Goes back to the previously visited card, if there is one.
    ///
    /// # Errors
    ///
    /// As for [`Runtime::go_to_card`].
    pub fn go_back(&mut self) -> RuntimeResult<bool> {
        Interpreter::new(self).navigate_back()
    }

    /// Records that navigation has happened. Only the interpreter calls this,
    /// after it has sent `closeCard`.
    pub(crate) fn commit_navigation(&mut self, card: Id, remember: bool) {
        if remember {
            self.back_stack.push(self.current_card);
        }
        self.current_card = card;
        self.push_effect(Effect::Navigated { card });
    }

    /// Takes the card `go back` should return to.
    pub(crate) fn pop_back_stack(&mut self) -> Option<Id> {
        self.back_stack.pop()
    }

    // -------------------------------------------------------------- scripts

    /// Sends a message to an object and lets it travel the message path.
    ///
    /// Returns what the handler returned, or [`Value::Empty`] if nothing
    /// handled it — an unhandled message is normal, not an error.
    ///
    /// # Errors
    ///
    /// Returns an error if a script fails to parse or fails while running.
    pub fn send_message(&mut self, message: &Message, to: ObjectId) -> RuntimeResult<Value> {
        Interpreter::new(self).dispatch(message, to)
    }

    /// Sends `openStack`, which a stack script may use to set itself up.
    ///
    /// # Errors
    ///
    /// As for [`Runtime::send_message`].
    pub fn open_stack(&mut self) -> RuntimeResult<Value> {
        let stack = ObjectId::new(ObjectKind::Stack, self.stack.id());
        self.send_message(&Message::new(messages::OPEN_STACK), stack)?;
        let card = ObjectId::new(ObjectKind::Card, self.current_card);
        self.send_message(&Message::new(messages::OPEN_CARD), card)
    }

    /// Runs a fragment of HyperTalk as if it were the body of a handler on
    /// `me`. This is how the message box, the MCP `run_script` tool and
    /// future AI assistants execute code.
    ///
    /// # Errors
    ///
    /// Returns an error if the fragment does not parse or fails while
    /// running.
    pub fn run_script(&mut self, source: &str, me: ObjectId) -> RuntimeResult<Value> {
        Interpreter::new(self).run_fragment(source, me)
    }

    /// Checks that a script parses, without running it.
    ///
    /// # Errors
    ///
    /// Returns the first parse error.
    pub fn check_script(source: &str) -> RuntimeResult<()> {
        hyperlab_parser::parse(source)?;
        Ok(())
    }

    // --------------------------------------------------------------- access

    /// An object by kind and id.
    ///
    /// # Errors
    ///
    /// Returns an error naming the object if it is not there.
    pub fn object(&self, object: ObjectId) -> RuntimeResult<&dyn Object> {
        self.stack
            .object(object.kind, object.id)
            .ok_or_else(|| RuntimeError::new(format!("there is no {object}")))
    }

    /// The stack, mutably, for persistence and migration only.
    ///
    /// Editing through this bypasses undo, which is why nothing in the
    /// editing path may use it.
    pub fn stack_mut_unchecked(&mut self) -> &mut Stack {
        &mut self.stack
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Command, PartOwner};
    use hyperlab_stack::{PartKind, Rect};

    fn runtime() -> Runtime {
        Runtime::new(Stack::new("Test"))
    }

    #[test]
    fn a_new_runtime_starts_on_the_first_card() {
        let runtime = runtime();
        assert_eq!(runtime.current_card(), runtime.stack().cards()[0].id());
        assert_eq!(runtime.current_card_index(), 0);
    }

    #[test]
    fn commands_can_be_undone_and_redone() {
        let mut runtime = runtime();
        runtime
            .execute(Command::CreateCard {
                after: 0,
                background: None,
            })
            .unwrap();
        assert_eq!(runtime.stack().card_count(), 2);
        assert!(runtime.history().can_undo());

        assert!(runtime.undo().unwrap());
        assert_eq!(runtime.stack().card_count(), 1);

        assert!(runtime.redo().unwrap());
        assert_eq!(runtime.stack().card_count(), 2);
    }

    #[test]
    fn undoing_with_nothing_to_undo_is_not_an_error() {
        let mut runtime = runtime();
        assert!(!runtime.undo().unwrap());
        assert!(!runtime.redo().unwrap());
    }

    #[test]
    fn deleting_the_current_card_moves_somewhere_real() {
        let mut runtime = runtime();
        runtime
            .execute(Command::CreateCard {
                after: 0,
                background: None,
            })
            .unwrap();
        let second = runtime.stack().cards()[1].id();
        runtime.go_to_card(second).unwrap();

        runtime.execute(Command::DeleteCard { id: second }).unwrap();
        assert_eq!(runtime.current_card(), runtime.stack().cards()[0].id());
    }

    #[test]
    fn navigation_records_where_it_has_been() {
        let mut runtime = runtime();
        runtime
            .execute(Command::CreateCard {
                after: 0,
                background: None,
            })
            .unwrap();
        let first = runtime.stack().cards()[0].id();
        let second = runtime.stack().cards()[1].id();

        runtime.go_to_card(second).unwrap();
        assert_eq!(runtime.current_card(), second);
        assert!(runtime.go_back().unwrap());
        assert_eq!(runtime.current_card(), first);
        assert!(!runtime.go_back().unwrap(), "nowhere left to go back to");
    }

    #[test]
    fn navigation_wraps_around_the_ends() {
        let mut runtime = runtime();
        runtime
            .execute(Command::CreateCard {
                after: 0,
                background: None,
            })
            .unwrap();
        let first = runtime.stack().cards()[0].id();
        let second = runtime.stack().cards()[1].id();

        runtime.go_to_index(-1).unwrap();
        assert_eq!(runtime.current_card(), second);
        runtime.go_to_index(2).unwrap();
        assert_eq!(runtime.current_card(), first);
    }

    #[test]
    fn navigation_is_reported_as_an_effect() {
        let mut runtime = runtime();
        runtime
            .execute(Command::CreateCard {
                after: 0,
                background: None,
            })
            .unwrap();
        let second = runtime.stack().cards()[1].id();
        runtime.take_effects();

        runtime.go_to_card(second).unwrap();
        assert_eq!(
            runtime.take_effects(),
            vec![Effect::Navigated { card: second }]
        );
    }

    #[test]
    fn creating_a_part_reports_what_it_created() {
        let mut runtime = runtime();
        let card = runtime.current_card();
        let created = runtime
            .execute(Command::CreatePart {
                owner: PartOwner::Card { id: card },
                kind: PartKind::Button,
                name: "Go".into(),
                geometry: Rect::default(),
            })
            .unwrap()
            .expect("a part was created");
        assert_eq!(created.kind, ObjectKind::Button);
        assert_eq!(runtime.object(created).unwrap().name(), "Go");
    }
}
