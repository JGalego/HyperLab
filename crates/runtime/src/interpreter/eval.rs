//! Evaluating expressions.

use hyperlab_parser::ast::{BinaryOp, CountTarget, Expr, Layer, UnaryOp};
use hyperlab_stack::{Object, ObjectKind, PartContainer, Value};

use super::{Interpreter, builtins};
use crate::{
    chunk,
    error::{RuntimeError, RuntimeResult},
    host::AiRequest,
};

impl Interpreter<'_> {
    /// Works out what an expression is worth.
    pub(crate) fn evaluate(&mut self, expression: &Expr) -> RuntimeResult<Value> {
        match expression {
            Expr::Number(number) => Ok(Value::Number(*number)),
            Expr::Text(text) => Ok(Value::text(text)),

            Expr::Constant(name) => builtins::constant(name)
                .ok_or_else(|| RuntimeError::new(format!("I do not know the constant {name}"))),

            // A name that has never been used as a variable stands for
            // itself, which is how classic HyperTalk lets scripts say
            // `put cancel into it` without quotes.
            Expr::Variable(name) => Ok(self.variable(name).unwrap_or_else(|| Value::text(name))),

            Expr::It => Ok(self.variable("it").unwrap_or(Value::Empty)),

            Expr::Unary { operator, operand } => {
                let value = self.evaluate(operand)?;
                match operator {
                    UnaryOp::Negate => {
                        let number = value.as_number().ok_or_else(|| {
                            RuntimeError::new(format!("I cannot negate \"{value}\""))
                        })?;
                        Ok(Value::Number(-number))
                    }
                    UnaryOp::Not => {
                        let boolean = value.as_bool().ok_or_else(|| {
                            RuntimeError::new(format!("\"{value}\" is not true or false"))
                        })?;
                        Ok(Value::Bool(!boolean))
                    }
                }
            }

            Expr::Binary {
                operator,
                left,
                right,
            } => self.evaluate_binary(*operator, left, right),

            Expr::Call { name, arguments } => {
                let values = self.evaluate_all(arguments)?;
                self.call_function(name, &values)
            }

            Expr::The(name) => self.evaluate_the(name),

            Expr::Of { name, operand } => self.evaluate_of(name, operand),

            Expr::Object(reference) => {
                let object = self.resolve_object(reference)?;
                self.contents_of(object)
            }

            Expr::Count(target) => self.evaluate_count(target),

            Expr::Chunk { chunks, source } => {
                let text = self.evaluate(source)?.as_text();
                let slices = self.slices(chunks)?;
                Ok(Value::text(chunk::extract_nested(&text, &slices)))
            }

            Expr::Exists { object, negated } => {
                let exists = self.object_exists(object);
                Ok(Value::Bool(exists != *negated))
            }
        }
    }

    /// Evaluates a list of expressions, left to right.
    pub(crate) fn evaluate_all(&mut self, expressions: &[Expr]) -> RuntimeResult<Vec<Value>> {
        expressions
            .iter()
            .map(|expression| self.evaluate(expression))
            .collect()
    }

    fn evaluate_binary(
        &mut self,
        operator: BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> RuntimeResult<Value> {
        // `and` and `or` stop as soon as the answer is known, so that
        // `if there is a field "x" and the text of field "x" is empty` is
        // safe to write.
        match operator {
            BinaryOp::And => {
                return Ok(Value::Bool(self.boolean(left)? && self.boolean(right)?));
            }
            BinaryOp::Or => {
                return Ok(Value::Bool(self.boolean(left)? || self.boolean(right)?));
            }
            _ => {}
        }

        let left = self.evaluate(left)?;
        let right = self.evaluate(right)?;
        apply_binary(operator, &left, &right)
    }

    /// Evaluates an expression that must be true or false.
    fn boolean(&mut self, expression: &Expr) -> RuntimeResult<bool> {
        let value = self.evaluate(expression)?;
        value
            .as_bool()
            .ok_or_else(|| RuntimeError::new(format!("\"{value}\" is not true or false")))
    }

    /// `the <name>`: a function of no arguments, or a property of `me`.
    fn evaluate_the(&mut self, name: &str) -> RuntimeResult<Value> {
        if name == "result" {
            return Ok(self.runtime.result().clone());
        }
        if let Some(value) = builtins::nullary(name) {
            return Ok(value);
        }
        let me = self.me()?;
        self.property(me, name)
            .map_err(|_| RuntimeError::new(format!("I do not know what \"the {name}\" means here")))
    }

    /// `the <name> of <operand>`: a property when the operand is an object,
    /// and a one-argument function otherwise.
    ///
    /// Deciding this here, rather than in the parser, is what lets new
    /// properties and new functions appear without grammar changes.
    fn evaluate_of(&mut self, name: &str, operand: &Expr) -> RuntimeResult<Value> {
        if let Expr::Object(reference) = operand {
            let object = self.resolve_object(reference)?;
            return self.property(object, name);
        }
        let value = self.evaluate(operand)?;
        self.call_function(name, std::slice::from_ref(&value))
    }

    /// Calls a function: a handler somewhere in the message path if there is
    /// one, otherwise a built-in.
    fn call_function(&mut self, name: &str, arguments: &[Value]) -> RuntimeResult<Value> {
        if let Some(value) = self.call_user_function(name, arguments)? {
            return Ok(value);
        }
        if name.eq_ignore_ascii_case("result") {
            return Ok(self.runtime.result().clone());
        }
        // `ai` is not in the built-in table because everything there is a
        // pure function of its arguments. This one has to reach the host.
        if name.eq_ignore_ascii_case("ai") {
            return self.evaluate_ai(arguments);
        }
        builtins::call(name, arguments)?
            .ok_or_else(|| RuntimeError::new(format!("I do not know a function called \"{name}\"")))
    }

    /// `ai("…")`: asks a language model, and evaluates to what it says.
    ///
    /// Unlike `ask assistant`, a refusal is an error rather than a value.
    /// This sits in the middle of an expression, and there is no honest
    /// answer to `ai("…") + 1` when nothing answered.
    fn evaluate_ai(&mut self, arguments: &[Value]) -> RuntimeResult<Value> {
        let prompt = arguments.first().map(Value::as_text).unwrap_or_default();
        if prompt.trim().is_empty() {
            return Err(RuntimeError::new("\"ai\" needs something to ask"));
        }
        self.ask_assistant(&AiRequest::answer(prompt))
            .map(Value::text)
            .map_err(RuntimeError::new)
    }

    fn evaluate_count(&mut self, target: &CountTarget) -> RuntimeResult<Value> {
        let count = match target {
            CountTarget::Cards => self.runtime.stack().card_count(),
            CountTarget::Backgrounds => self.runtime.stack().backgrounds().len(),
            CountTarget::Chunks { kind, source } => {
                let text = self.evaluate(source)?.as_text();
                chunk::count(&text, *kind)
            }
            CountTarget::Parts { kind, layer, owner } => {
                let kind = super::objects::part_kind(*kind);
                let (card, background) = match owner {
                    Some(reference) => {
                        let owner = self.resolve_object(reference)?;
                        match owner.kind {
                            ObjectKind::Card => (
                                Some(owner.id),
                                self.runtime.stack().background_of(owner.id).map(Object::id),
                            ),
                            ObjectKind::Background => (None, Some(owner.id)),
                            _ => {
                                return Err(RuntimeError::new(format!(
                                    "{owner} does not have buttons or fields"
                                )));
                            }
                        }
                    }
                    None => {
                        let card = self.runtime.current_card();
                        (
                            Some(card),
                            self.runtime.stack().background_of(card).map(Object::id),
                        )
                    }
                };

                let mut total = 0;
                if matches!(layer, Layer::Card | Layer::Unspecified) {
                    if let Some(card) = card.and_then(|id| self.runtime.stack().card(id)) {
                        total += card.parts_of_kind(kind).len();
                    }
                }
                if matches!(layer, Layer::Background | Layer::Unspecified) {
                    if let Some(background) =
                        background.and_then(|id| self.runtime.stack().background(id))
                    {
                        total += background.parts_of_kind(kind).len();
                    }
                }
                total
            }
        };
        Ok(Value::from(count))
    }
}

/// Applies an infix operator to two values.
fn apply_binary(operator: BinaryOp, left: &Value, right: &Value) -> RuntimeResult<Value> {
    use BinaryOp::{
        Add, And, Concat, ConcatSpace, Contains, Divide, EndsWith, Equal, Greater, GreaterOrEqual,
        IntegerDivide, IsIn, Less, LessOrEqual, Modulo, Multiply, NotEqual, Or, Power, StartsWith,
        Subtract,
    };

    let arithmetic = |operator: BinaryOp| -> RuntimeResult<f64> {
        let a = number(left)?;
        let b = number(right)?;
        Ok(match operator {
            Add => a + b,
            Subtract => a - b,
            Multiply => a * b,
            Power => a.powf(b),
            _ => unreachable!("only arithmetic operators reach here"),
        })
    };

    Ok(match operator {
        Add | Subtract | Multiply | Power => Value::Number(arithmetic(operator)?),
        Divide | IntegerDivide | Modulo => {
            let a = number(left)?;
            let b = number(right)?;
            if b == 0.0 {
                return Err(RuntimeError::new("I cannot divide by zero"));
            }
            Value::Number(match operator {
                Divide => a / b,
                IntegerDivide => (a / b).trunc(),
                _ => a % b,
            })
        }
        Concat => Value::text(format!("{}{}", left.as_text(), right.as_text())),
        ConcatSpace => Value::text(format!("{} {}", left.as_text(), right.as_text())),
        Equal => Value::Bool(left.loosely_equals(right)),
        NotEqual => Value::Bool(!left.loosely_equals(right)),
        Less | Greater | LessOrEqual | GreaterOrEqual => {
            let ordering = compare(left, right);
            Value::Bool(match operator {
                Less => ordering.is_lt(),
                Greater => ordering.is_gt(),
                LessOrEqual => ordering.is_le(),
                _ => ordering.is_ge(),
            })
        }
        Contains => Value::Bool(contains(&left.as_text(), &right.as_text())),
        IsIn => Value::Bool(contains(&right.as_text(), &left.as_text())),
        StartsWith => Value::Bool(
            left.as_text()
                .to_lowercase()
                .starts_with(&right.as_text().to_lowercase()),
        ),
        EndsWith => Value::Bool(
            left.as_text()
                .to_lowercase()
                .ends_with(&right.as_text().to_lowercase()),
        ),
        And | Or => unreachable!("short-circuit operators are handled before this"),
    })
}

/// Numbers compare as numbers; anything else compares as text, ignoring case.
fn compare(left: &Value, right: &Value) -> std::cmp::Ordering {
    match (left.as_number(), right.as_number()) {
        (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
        _ => left
            .as_text()
            .to_lowercase()
            .cmp(&right.as_text().to_lowercase()),
    }
}

fn contains(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn number(value: &Value) -> RuntimeResult<f64> {
    value
        .as_number()
        .ok_or_else(|| RuntimeError::new(format!("I expected a number but found \"{value}\"")))
}
