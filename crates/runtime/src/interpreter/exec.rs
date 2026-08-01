//! Executing statements.

use hyperlab_parser::ast::{
    ArithmeticCommand, Block, Destination, ExitTarget, Expr, Preposition, RepeatControl, Statement,
    StatementKind,
};
use hyperlab_stack::{Id, Value};

use super::{Flow, Interpreter, MAX_ITERATIONS};
use crate::{
    command::Command,
    error::{RuntimeError, RuntimeResult},
    event::Message,
};

impl Interpreter<'_> {
    /// Runs a sequence of statements.
    pub(crate) fn execute_block(&mut self, block: &Block) -> RuntimeResult<Flow> {
        for statement in block {
            let flow = self.execute_statement(statement)?;
            if flow != Flow::Normal {
                return Ok(flow);
            }
        }
        Ok(Flow::Normal)
    }

    /// Runs one statement, tagging any error with the line it came from.
    fn execute_statement(&mut self, statement: &Statement) -> RuntimeResult<Flow> {
        self.execute_statement_kind(&statement.kind)
            .map_err(|error| error.at_line(statement.line))
    }

    fn execute_statement_kind(&mut self, kind: &StatementKind) -> RuntimeResult<Flow> {
        match kind {
            StatementKind::Put {
                value,
                target,
                preposition,
            } => {
                let value = self.evaluate(value)?;
                match target {
                    Some(container) => self.write_container(container, &value, *preposition)?,
                    None => {
                        let text = self.combine_with_message_box(&value, *preposition);
                        self.runtime.set_message_box(text);
                    }
                }
                Ok(Flow::Normal)
            }

            StatementKind::Set {
                property,
                object,
                value,
            } => {
                let value = self.evaluate(value)?;
                let object = match object {
                    Some(reference) => self.resolve_object(reference)?,
                    None => self.me()?,
                };
                self.set_property(object, property, value)?;
                Ok(Flow::Normal)
            }

            StatementKind::Get(expression) => {
                let value = self.evaluate(expression)?;
                self.set_variable("it", value)?;
                Ok(Flow::Normal)
            }

            StatementKind::Arithmetic {
                operator,
                value,
                target,
            } => {
                let operand = self.number(value)?;
                let current = self.read_container(target)?;
                let current = current.as_number().unwrap_or(0.0);
                let result = match operator {
                    ArithmeticCommand::Add => current + operand,
                    ArithmeticCommand::Subtract => current - operand,
                    ArithmeticCommand::Multiply => current * operand,
                    ArithmeticCommand::Divide => {
                        if operand == 0.0 {
                            return Err(RuntimeError::new("I cannot divide by zero"));
                        }
                        current / operand
                    }
                };
                self.write_container(target, &Value::Number(result), Preposition::Into)?;
                Ok(Flow::Normal)
            }

            StatementKind::If {
                branches,
                otherwise,
            } => {
                for branch in branches {
                    if self.truth(&branch.condition)? {
                        return self.execute_block(&branch.body);
                    }
                }
                match otherwise {
                    Some(body) => self.execute_block(body),
                    None => Ok(Flow::Normal),
                }
            }

            StatementKind::Repeat { control, body } => self.execute_repeat(control, body),

            StatementKind::Exit(target) => match target {
                ExitTarget::Repeat => Ok(Flow::ExitRepeat),
                ExitTarget::Everything => Ok(Flow::ExitAll),
                ExitTarget::Handler(name) => {
                    // `exit foo` must name the handler it is leaving, which
                    // catches a stale name left behind by a rename.
                    let running = self.current_handler()?;
                    if running.eq_ignore_ascii_case(name) {
                        Ok(Flow::ExitHandler)
                    } else {
                        Err(RuntimeError::new(format!(
                            "\"exit {name}\" is inside \"{running}\", not \"{name}\""
                        )))
                    }
                }
            },

            StatementKind::NextRepeat => Ok(Flow::NextRepeat),

            StatementKind::Pass(name) => Ok(Flow::Pass(name.clone())),

            StatementKind::Return(expression) => {
                let value = match expression {
                    Some(expression) => self.evaluate(expression)?,
                    None => Value::Empty,
                };
                Ok(Flow::Return(value))
            }

            StatementKind::Global(names) => {
                self.declare_globals(names)?;
                Ok(Flow::Normal)
            }

            StatementKind::Go(destination) => {
                self.go(destination)?;
                Ok(Flow::Normal)
            }

            StatementKind::Send { message, target } => {
                let name = self.evaluate(message)?.as_text();
                let target = self.resolve_object(target)?;
                let message = Message::new(name.trim());
                let value = self.dispatch(&message, target)?;
                self.runtime.set_result(value);
                Ok(Flow::Normal)
            }

            StatementKind::Command { name, arguments } => self.run_command(name, arguments),
        }
    }

    fn execute_repeat(&mut self, control: &RepeatControl, body: &Block) -> RuntimeResult<Flow> {
        let mut iterations: u64 = 0;
        let mut counter: Option<(String, f64, f64, f64)> = None;

        // Bounds that are fixed for the whole loop are evaluated once, which
        // is both faster and what scripts expect.
        let mut remaining = match control {
            RepeatControl::Times(count) => Some(self.number(count)?.max(0.0) as u64),
            RepeatControl::With {
                variable,
                from,
                to,
                down,
            } => {
                let from = self.number(from)?;
                let to = self.number(to)?;
                let step = if *down { -1.0 } else { 1.0 };
                counter = Some((variable.clone(), from, to, step));
                None
            }
            _ => None,
        };

        loop {
            iterations += 1;
            if iterations > MAX_ITERATIONS {
                return Err(RuntimeError::new(
                    "this repeat loop ran more than a million times; \
                     it is probably never going to stop",
                ));
            }

            match control {
                RepeatControl::Forever => {}
                RepeatControl::Times(_) => match remaining {
                    Some(0) | None => return Ok(Flow::Normal),
                    Some(left) => remaining = Some(left - 1),
                },
                RepeatControl::While(condition) => {
                    if !self.truth(condition)? {
                        return Ok(Flow::Normal);
                    }
                }
                RepeatControl::Until(condition) => {
                    if self.truth(condition)? {
                        return Ok(Flow::Normal);
                    }
                }
                RepeatControl::With { .. } => {
                    let Some((variable, current, limit, step)) = counter.as_mut() else {
                        return Ok(Flow::Normal);
                    };
                    let done = if *step > 0.0 {
                        *current > *limit
                    } else {
                        *current < *limit
                    };
                    if done {
                        return Ok(Flow::Normal);
                    }
                    let (name, value) = (variable.clone(), *current);
                    *current += *step;
                    self.set_variable(&name, Value::Number(value))?;
                }
            }

            match self.execute_block(body)? {
                Flow::Normal | Flow::NextRepeat => {}
                Flow::ExitRepeat => return Ok(Flow::Normal),
                other => return Ok(other),
            }
        }
    }

    /// Carries out `go`.
    fn go(&mut self, destination: &Destination) -> RuntimeResult<()> {
        match destination {
            Destination::Back => {
                self.navigate_back()?;
                Ok(())
            }
            Destination::Card(specifier) => {
                let card = self.resolve_card(specifier)?;
                self.navigate_to(card, true)
            }
        }
    }

    /// Moves to a card, sending `closeCard` and `openCard` on the way.
    ///
    /// Navigation lives here rather than on the runtime so that a script that
    /// navigates from an `openCard` handler shares this interpreter's frame
    /// budget, and so cannot loop for ever.
    pub(crate) fn navigate_to(&mut self, card: Id, remember: bool) -> RuntimeResult<()> {
        if self.runtime.stack().card(card).is_none() {
            return Err(RuntimeError::new(format!(
                "there is no card with id {card}"
            )));
        }
        if card == self.runtime.current_card() {
            return Ok(());
        }
        let leaving = self.runtime.current_card();
        self.dispatch(
            &Message::new(crate::event::messages::CLOSE_CARD),
            hyperlab_stack::ObjectId::new(hyperlab_stack::ObjectKind::Card, leaving),
        )?;
        self.runtime.commit_navigation(card, remember);
        self.dispatch(
            &Message::new(crate::event::messages::OPEN_CARD),
            hyperlab_stack::ObjectId::new(hyperlab_stack::ObjectKind::Card, card),
        )?;
        Ok(())
    }

    /// Returns to the previously visited card.
    pub(crate) fn navigate_back(&mut self) -> RuntimeResult<bool> {
        let Some(card) = self.runtime.pop_back_stack() else {
            return Ok(false);
        };
        if self.runtime.stack().card(card).is_none() {
            return Ok(false);
        }
        self.navigate_to(card, false)?;
        Ok(true)
    }

    /// Applies a `put … before/after` to the message box.
    fn combine_with_message_box(&self, value: &Value, preposition: Preposition) -> String {
        let existing = self.runtime.message_box();
        match preposition {
            Preposition::Into => value.as_text(),
            Preposition::Before => format!("{}{existing}", value.as_text()),
            Preposition::After => format!("{existing}{}", value.as_text()),
        }
    }

    /// Sets a property through the command bus, so scripted edits are
    /// undoable exactly like the user's.
    pub(crate) fn set_property(
        &mut self,
        object: hyperlab_stack::ObjectId,
        property: &str,
        value: Value,
    ) -> RuntimeResult<()> {
        self.runtime.execute(Command::SetProperty {
            object,
            property: property.to_string(),
            value: Some(value),
        })?;
        Ok(())
    }

    /// Evaluates an expression that must be a number.
    pub(crate) fn number(&mut self, expression: &Expr) -> RuntimeResult<f64> {
        let value = self.evaluate(expression)?;
        value
            .as_number()
            .ok_or_else(|| RuntimeError::new(format!("I expected a number but found \"{value}\"")))
    }

    /// Evaluates an expression that must be true or false.
    fn truth(&mut self, expression: &Expr) -> RuntimeResult<bool> {
        let value = self.evaluate(expression)?;
        value.as_bool().ok_or_else(|| {
            RuntimeError::new(format!("I expected true or false but found \"{value}\""))
        })
    }
}
