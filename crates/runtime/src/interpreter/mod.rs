//! The HyperTalk interpreter.
//!
//! The interpreter walks the AST the parser produced. It is a tree walker,
//! not a bytecode machine, because scripts in a HyperCard-like system are
//! short, are edited constantly, and are read by people far more often than
//! they are run.
//!
//! # How a script runs
//!
//! 1. Something sends a [`Message`] to an object.
//! 2. The message walks the [message path](crate::event::message_path) until
//!    an object's script has a matching handler.
//! 3. The handler's statements run in a [`Frame`] that knows `me`, `the
//!    target` and the local variables.
//! 4. `pass` sends the message on to the next object in the path.
//!
//! Every change a script makes goes through [`Command`](crate::Command), the
//! same way the user's changes do — so scripted edits are undoable, and an AI
//! that drives the runtime cannot invent a private back door.

mod builtins;
mod eval;
mod exec;
mod objects;

use std::collections::{BTreeMap, BTreeSet};

use hyperlab_parser::{
    ast::{Handler, HandlerKind},
    parse,
};
use hyperlab_stack::{Object, ObjectId, ObjectKind, PropertyBag, Value};

use crate::{
    error::{RuntimeError, RuntimeResult},
    event::{Message, message_path},
    runtime::Runtime,
};

/// How deeply handlers may call one another before the runtime gives up.
///
/// Without a limit, a handler that sends itself a message would take the
/// whole application down with it.
const MAX_DEPTH: usize = 64;

/// The most times a `repeat` loop may go round.
///
/// A live programming system must not be able to freeze itself; a script that
/// wants to run forever should use `idle`.
const MAX_ITERATIONS: u64 = 1_000_000;

/// One running handler.
#[derive(Debug, Clone)]
pub(crate) struct Frame {
    /// The object whose script this is.
    me: ObjectId,
    /// The object the message was originally sent to.
    target: ObjectId,
    /// The handler's name, for `exit <name>` and `pass <name>`.
    handler: String,
    /// Local variables, by normalized name.
    locals: BTreeMap<String, Value>,
    /// Names this handler declared `global`.
    globals: BTreeSet<String>,
    /// The message path this handler was found on.
    path: Vec<ObjectId>,
    /// Where in that path `me` sits, so `pass` knows where to carry on.
    path_index: usize,
}

/// Where control went after a statement.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Flow {
    /// On to the next statement.
    Normal,
    /// Leave the innermost loop.
    ExitRepeat,
    /// Start the loop's next turn.
    NextRepeat,
    /// Leave this handler, and any loops inside it.
    ExitHandler,
    /// Abandon everything: `exit to HyperLab`.
    ExitAll,
    /// Leave this handler with a value.
    Return(Value),
    /// Hand the message to the next object in the path.
    Pass(String),
}

/// What running a handler produced.
enum Outcome {
    /// It finished, with this value.
    Value(Value),
    /// It passed the message on under this name.
    Passed(String),
    /// It stopped everything.
    Aborted,
}

/// Executes scripts against a [`Runtime`].
pub(crate) struct Interpreter<'a> {
    runtime: &'a mut Runtime,
    frames: Vec<Frame>,
}

impl<'a> Interpreter<'a> {
    /// Prepares to run scripts against `runtime`.
    pub(crate) fn new(runtime: &'a mut Runtime) -> Self {
        Self {
            runtime,
            frames: Vec::new(),
        }
    }

    /// Sends a message to an object and lets it travel the message path.
    pub(crate) fn dispatch(&mut self, message: &Message, to: ObjectId) -> RuntimeResult<Value> {
        let path = message_path(self.runtime.stack(), to, self.runtime.current_card());
        Ok(self
            .dispatch_along(&path, 0, &message.name, &message.arguments, to)?
            .unwrap_or(Value::Empty))
    }

    /// Runs a fragment of HyperTalk as though it were a handler body on `me`.
    pub(crate) fn run_fragment(&mut self, source: &str, me: ObjectId) -> RuntimeResult<Value> {
        // A fragment may be either a bare sequence of statements or a full
        // script; wrapping it makes both work with one code path.
        let wrapped = format!("on __fragment\n{source}\nend __fragment");
        let script = match parse(&wrapped) {
            Ok(script) => script,
            // If wrapping failed the source probably contains its own
            // handlers; try it as written.
            Err(_) => parse(source)?,
        };
        // A script full of `function` definitions has nothing to run; only
        // a message handler is something you can ask for by itself.
        let Some(handler) = script
            .handlers
            .iter()
            .find(|handler| handler.kind == HandlerKind::Message)
        else {
            return Ok(Value::Empty);
        };
        let path = message_path(self.runtime.stack(), me, self.runtime.current_card());
        match self.run_handler(handler, &[], me, me, &path, 0)? {
            Outcome::Value(value) => Ok(value),
            Outcome::Passed(_) | Outcome::Aborted => Ok(Value::Empty),
        }
    }

    /// Walks the message path from `start`, running the first matching
    /// handler and following `pass`.
    ///
    /// Returns `None` when nothing handled the message, which is the normal
    /// case for most messages and is not an error.
    pub(crate) fn dispatch_along(
        &mut self,
        path: &[ObjectId],
        start: usize,
        name: &str,
        arguments: &[Value],
        target: ObjectId,
    ) -> RuntimeResult<Option<Value>> {
        let mut name = name.to_string();
        let mut index = start;

        while index < path.len() {
            let me = path[index];
            let Some(source) = self.script_of(me) else {
                index += 1;
                continue;
            };
            let script = parse(&source).map_err(|error| self.describe(me, error.into()))?;
            if let Some(handler) = script.handler(HandlerKind::Message, &name) {
                return match self.run_handler(handler, arguments, me, target, path, index)? {
                    Outcome::Value(value) => Ok(Some(value)),
                    Outcome::Aborted => Ok(Some(Value::Empty)),
                    Outcome::Passed(passed) => {
                        name = passed;
                        index += 1;
                        continue;
                    }
                };
            }
            index += 1;
        }
        Ok(None)
    }

    /// Calls a `function` handler, searching outwards from `me`.
    ///
    /// Returns `None` when no handler by that name exists, so the caller can
    /// fall back to the built-in functions.
    pub(crate) fn call_user_function(
        &mut self,
        name: &str,
        arguments: &[Value],
    ) -> RuntimeResult<Option<Value>> {
        let (target, path, start) = match self.frames.last() {
            Some(frame) => (frame.target, frame.path.clone(), frame.path_index),
            None => {
                let stack = ObjectId::new(ObjectKind::Stack, self.runtime.stack().id());
                (stack, vec![stack], 0)
            }
        };

        for index in start..path.len() {
            let owner = path[index];
            let Some(source) = self.script_of(owner) else {
                continue;
            };
            let script = parse(&source).map_err(|error| self.describe(owner, error.into()))?;
            if let Some(handler) = script.handler(HandlerKind::Function, name) {
                return match self.run_handler(handler, arguments, owner, target, &path, index)? {
                    Outcome::Value(value) => Ok(Some(value)),
                    Outcome::Passed(_) | Outcome::Aborted => Ok(Some(Value::Empty)),
                };
            }
        }
        Ok(None)
    }

    /// Runs one handler in a fresh frame.
    fn run_handler(
        &mut self,
        handler: &Handler,
        arguments: &[Value],
        me: ObjectId,
        target: ObjectId,
        path: &[ObjectId],
        path_index: usize,
    ) -> RuntimeResult<Outcome> {
        if self.frames.len() >= MAX_DEPTH {
            return Err(RuntimeError::new(format!(
                "\"{}\" is nested more than {MAX_DEPTH} handlers deep; is it calling itself?",
                handler.name
            )));
        }

        let mut locals = BTreeMap::new();
        for (index, parameter) in handler.parameters.iter().enumerate() {
            locals.insert(
                PropertyBag::normalize(parameter),
                arguments.get(index).cloned().unwrap_or(Value::Empty),
            );
        }
        // `it` starts empty in every handler, as in HyperCard.
        locals.insert("it".to_string(), Value::Empty);

        self.frames.push(Frame {
            me,
            target,
            handler: handler.name.clone(),
            locals,
            globals: BTreeSet::new(),
            path: path.to_vec(),
            path_index,
        });

        let flow = self.execute_block(&handler.body);
        self.frames.pop();
        let flow = flow.map_err(|error| self.describe(me, error))?;

        Ok(match flow {
            Flow::Return(value) => Outcome::Value(value),
            Flow::Pass(name) => Outcome::Passed(name),
            Flow::ExitAll => Outcome::Aborted,
            _ => Outcome::Value(Value::Empty),
        })
    }

    // -------------------------------------------------------------- helpers

    /// The script of an object, or `None` when it has none worth parsing.
    fn script_of(&self, object: ObjectId) -> Option<String> {
        let source = self
            .runtime
            .stack()
            .object(object.kind, object.id)?
            .script()
            .trim()
            .to_string();
        (!source.is_empty()).then_some(source)
    }

    /// Adds "which object's script" to an error, which is the first thing
    /// anyone debugging a stack wants to know.
    fn describe(&self, object: ObjectId, error: RuntimeError) -> RuntimeError {
        let name = self
            .runtime
            .stack()
            .object(object.kind, object.id)
            .map(|object| object.name().to_string())
            .unwrap_or_default();
        let where_ = if name.is_empty() {
            format!("{object}")
        } else {
            format!("{} \"{name}\"", object.kind)
        };
        RuntimeError {
            message: format!("{} (in the script of {where_})", error.message),
            line: error.line,
        }
    }

    /// The frame currently running.
    fn frame(&self) -> RuntimeResult<&Frame> {
        self.frames
            .last()
            .ok_or_else(|| RuntimeError::new("there is no handler running"))
    }

    fn frame_mut(&mut self) -> RuntimeResult<&mut Frame> {
        self.frames
            .last_mut()
            .ok_or_else(|| RuntimeError::new("there is no handler running"))
    }

    /// `me`: the object whose script is running.
    fn me(&self) -> RuntimeResult<ObjectId> {
        Ok(self.frame()?.me)
    }

    /// The name of the handler currently running.
    fn current_handler(&self) -> RuntimeResult<String> {
        Ok(self.frame()?.handler.clone())
    }

    // ------------------------------------------------------------ variables

    /// Reads a variable, or `None` if it has never been set.
    pub(crate) fn variable(&self, name: &str) -> Option<Value> {
        let key = PropertyBag::normalize(name);
        let frame = self.frames.last()?;
        if frame.globals.contains(&key) {
            return self.runtime.global(&key).cloned();
        }
        frame.locals.get(&key).cloned()
    }

    /// Writes a variable, respecting any `global` declaration.
    pub(crate) fn set_variable(&mut self, name: &str, value: Value) -> RuntimeResult<()> {
        let key = PropertyBag::normalize(name);
        let is_global = self.frame()?.globals.contains(&key);
        if is_global {
            self.runtime.set_global(&key, value);
        } else {
            self.frame_mut()?.locals.insert(key, value);
        }
        Ok(())
    }

    /// Declares names as global for the current handler.
    fn declare_globals(&mut self, names: &[String]) -> RuntimeResult<()> {
        for name in names {
            let key = PropertyBag::normalize(name);
            if self.runtime.global(&key).is_none() {
                self.runtime.set_global(&key, Value::Empty);
            }
            self.frame_mut()?.globals.insert(key);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{Runtime, command::Command};
    use hyperlab_stack::{Object, ObjectId, ObjectKind, Stack};

    #[test]
    fn a_fragment_runs_without_a_handler_around_it() {
        let mut runtime = Runtime::new(Stack::new("Test"));
        let stack = ObjectId::new(ObjectKind::Stack, runtime.stack().id());
        runtime.run_script("put 2 + 2", stack).unwrap();
        assert_eq!(runtime.message_box(), "4");
    }

    #[test]
    fn a_fragment_may_also_be_a_whole_script() {
        let mut runtime = Runtime::new(Stack::new("Test"));
        let stack = ObjectId::new(ObjectKind::Stack, runtime.stack().id());
        let value = runtime
            .run_script("function double n\n  return n * 2\nend double", stack)
            .unwrap();
        assert_eq!(value, hyperlab_stack::Value::Empty);
    }

    #[test]
    fn runaway_recursion_stops_with_an_explanation() {
        let mut runtime = Runtime::new(Stack::new("Test"));
        let stack_id = ObjectId::new(ObjectKind::Stack, runtime.stack().id());
        runtime
            .execute(Command::SetScript {
                object: stack_id,
                script: "on loopForever\n  loopForever\nend loopForever".into(),
            })
            .unwrap();
        let error = runtime
            .send_message(&crate::event::Message::new("loopForever"), stack_id)
            .unwrap_err();
        assert!(error.message.contains("deep"), "{error}");
    }
}
