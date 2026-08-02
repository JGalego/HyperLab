//! Built-in constants, functions and commands.
//!
//! Everything here is deliberately small and replaceable. A built-in is only
//! a name the runtime knows about *after* it has looked for a handler with
//! that name, so any stack can override any of them by writing its own.

use hyperlab_parser::ast::Expr;
use hyperlab_stack::Value;

use super::{Flow, Interpreter};
use crate::{
    error::{RuntimeError, RuntimeResult},
    host::{AiRequest, Effect},
};

/// The value of a named constant, or `None` if the name is not one.
#[must_use]
pub(crate) fn constant(name: &str) -> Option<Value> {
    Some(match name {
        "empty" => Value::Empty,
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "quote" => Value::text("\""),
        "return" | "linefeed" | "newline" => Value::text("\n"),
        "space" => Value::text(" "),
        "tab" => Value::text("\t"),
        "comma" => Value::text(","),
        "colon" => Value::text(":"),
        "pi" => Value::Number(std::f64::consts::PI),
        _ => return None,
    })
}

/// Functions of no arguments: `the date`, `the ticks`, and friends.
#[must_use]
pub(crate) fn nullary(name: &str) -> Option<Value> {
    let seconds = unix_seconds();
    Some(match name {
        "date" | "short date" => Value::text(format_date(seconds, DateStyle::Short)),
        "long date" => Value::text(format_date(seconds, DateStyle::Long)),
        "abbrev date" | "abbreviated date" => {
            Value::text(format_date(seconds, DateStyle::Abbreviated))
        }
        "time" | "short time" => Value::text(format_time(seconds, false)),
        "long time" => Value::text(format_time(seconds, true)),
        "seconds" | "secs" => Value::Number(seconds as f64),
        "ticks" => Value::Number((seconds * 60) as f64),
        _ => return None,
    })
}

/// Calls a built-in function.
///
/// Returns `Ok(None)` when the name is not a built-in, so the caller can say
/// so in its own words.
pub(crate) fn call(name: &str, arguments: &[Value]) -> RuntimeResult<Option<Value>> {
    let lower = name.to_ascii_lowercase();
    let first = || arguments.first().cloned().unwrap_or(Value::Empty);
    let number_at = |index: usize| -> RuntimeResult<f64> {
        let value = arguments.get(index).cloned().unwrap_or(Value::Empty);
        value.as_number().ok_or_else(|| {
            RuntimeError::new(format!("\"{name}\" needs a number, but got \"{value}\""))
        })
    };

    let value = match lower.as_str() {
        "length" => Value::from(first().as_text().chars().count()),
        "abs" => Value::Number(number_at(0)?.abs()),
        "sqrt" => Value::Number(number_at(0)?.sqrt()),
        "trunc" => Value::Number(number_at(0)?.trunc()),
        "round" => Value::Number(number_at(0)?.round()),
        "exp" => Value::Number(number_at(0)?.exp()),
        "ln" => Value::Number(number_at(0)?.ln()),
        "sin" => Value::Number(number_at(0)?.sin()),
        "cos" => Value::Number(number_at(0)?.cos()),
        "tan" => Value::Number(number_at(0)?.tan()),
        "min" | "max" | "sum" | "average" | "avg" => {
            let numbers = spread(arguments, name)?;
            if numbers.is_empty() {
                return Ok(Some(Value::Empty));
            }
            Value::Number(match lower.as_str() {
                "min" => numbers.iter().copied().fold(f64::INFINITY, f64::min),
                "max" => numbers.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                "sum" => numbers.iter().sum(),
                _ => numbers.iter().sum::<f64>() / numbers.len() as f64,
            })
        }
        "random" => {
            let limit = number_at(0)?.max(1.0) as u64;
            Value::Number((next_random() % limit + 1) as f64)
        }
        "chartonum" => {
            let text = first().as_text();
            match text.chars().next() {
                Some(c) => Value::from(i64::from(u32::from(c))),
                None => Value::Empty,
            }
        }
        "numtochar" => {
            let code = u32::try_from(number_at(0)? as i64).unwrap_or(0);
            Value::text(char::from_u32(code).map(String::from).unwrap_or_default())
        }
        "offset" => {
            let needle = first().as_text().to_lowercase();
            let haystack = arguments
                .get(1)
                .cloned()
                .unwrap_or(Value::Empty)
                .as_text()
                .to_lowercase();
            let position = if needle.is_empty() {
                None
            } else {
                haystack
                    .find(&needle)
                    .map(|byte| haystack[..byte].chars().count() + 1)
            };
            Value::from(position.unwrap_or(0))
        }
        "value" => {
            // `the value of "2 + 2"` re-parses text as an expression. It is
            // constant folding, not a doorway into the runtime: no object
            // references, no side effects.
            let source = first().as_text();
            let expression = hyperlab_parser::parse_expression(&source)?;
            constant_fold(&expression)?
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}

/// Flattens arguments into numbers, accepting both `sum(1, 2)` and
/// `sum("1,2")`, as HyperTalk's list functions do.
fn spread(arguments: &[Value], name: &str) -> RuntimeResult<Vec<f64>> {
    let mut numbers = Vec::new();
    for argument in arguments {
        let text = argument.as_text();
        for piece in text.split(',') {
            let piece = piece.trim();
            if piece.is_empty() {
                continue;
            }
            numbers.push(piece.parse::<f64>().map_err(|_| {
                RuntimeError::new(format!("\"{name}\" needs numbers, but got \"{piece}\""))
            })?);
        }
    }
    Ok(numbers)
}

/// Evaluates the small subset of expressions `value()` allows.
fn constant_fold(expression: &Expr) -> RuntimeResult<Value> {
    use hyperlab_parser::ast::{BinaryOp, UnaryOp};
    Ok(match expression {
        Expr::Number(number) => Value::Number(*number),
        Expr::Text(text) => Value::text(text),
        Expr::Constant(name) => constant(name).unwrap_or(Value::Empty),
        Expr::Unary { operator, operand } => {
            let value = constant_fold(operand)?;
            match operator {
                UnaryOp::Negate => Value::Number(-value.as_number().unwrap_or(0.0)),
                UnaryOp::Not => Value::Bool(!value.as_bool().unwrap_or(false)),
            }
        }
        Expr::Binary {
            operator,
            left,
            right,
        } => {
            let a = constant_fold(left)?.as_number().unwrap_or(0.0);
            let b = constant_fold(right)?.as_number().unwrap_or(0.0);
            match operator {
                BinaryOp::Add => Value::Number(a + b),
                BinaryOp::Subtract => Value::Number(a - b),
                BinaryOp::Multiply => Value::Number(a * b),
                BinaryOp::Divide if b != 0.0 => Value::Number(a / b),
                BinaryOp::Divide => return Err(RuntimeError::new("I cannot divide by zero")),
                _ => {
                    return Err(RuntimeError::new(
                        "\"value\" understands only simple arithmetic",
                    ));
                }
            }
        }
        _ => {
            return Err(RuntimeError::new(
                "\"value\" understands only simple arithmetic",
            ));
        }
    })
}

impl Interpreter<'_> {
    /// Runs a command that is not part of the grammar.
    ///
    /// A handler with the same name always wins, so a stack can redefine
    /// `beep` or add `logToFile` and use them the same way.
    pub(crate) fn run_command(&mut self, name: &str, arguments: &[Expr]) -> RuntimeResult<Flow> {
        // Look for a handler first: user code beats built-ins.
        let values = self.evaluate_all(arguments)?;
        if let Some(value) = self.dispatch_from_me(name, &values)? {
            self.runtime.set_result(value);
            return Ok(Flow::Normal);
        }

        let text_at =
            |index: usize| -> String { values.get(index).map(Value::as_text).unwrap_or_default() };

        match name.to_ascii_lowercase().as_str() {
            "answer" => {
                let message = text_at(0);
                self.runtime.push_effect(Effect::Answer {
                    message: message.clone(),
                });
                self.runtime.host_mut().answer(&message);
            }
            "ask" => {
                let prompt = text_at(0);
                let default = text_at(1);
                self.runtime.push_effect(Effect::Ask {
                    prompt: prompt.clone(),
                    default: default.clone(),
                });
                let answer = self.runtime.host_mut().ask(&prompt, &default);
                // A cancelled question leaves `it` empty and says so in
                // `the result`, exactly as HyperCard does.
                match answer {
                    Some(text) => {
                        self.set_variable("it", Value::text(text))?;
                        self.runtime.set_result(Value::Empty);
                    }
                    None => {
                        self.set_variable("it", Value::Empty)?;
                        self.runtime.set_result(Value::text("Cancel"));
                    }
                }
            }
            "ask assistant" => {
                let prompt = text_at(0);
                if prompt.trim().is_empty() {
                    return Err(RuntimeError::new(
                        "\"ask assistant\" needs something to ask for",
                    ));
                }
                // Answered exactly like `ask`: the reply lands in `it`, and a
                // refusal says so in `the result` rather than stopping the
                // handler, so a stack still runs where no model is set up.
                match self.ask_assistant(&AiRequest::edit(prompt)) {
                    Ok(reply) => {
                        self.set_variable("it", Value::text(reply))?;
                        self.runtime.set_result(Value::Empty);
                    }
                    Err(refusal) => {
                        self.set_variable("it", Value::Empty)?;
                        self.runtime.set_result(Value::text(refusal));
                    }
                }
            }
            "beep" => {
                self.runtime.push_effect(Effect::Beep);
                self.runtime.host_mut().beep();
            }
            "wait" => {
                let amount = values.first().and_then(Value::as_number).unwrap_or(0.0);
                let unit = text_at(1).to_ascii_lowercase();
                let ticks = match unit.as_str() {
                    "second" | "seconds" | "sec" | "secs" => amount * 60.0,
                    "millisecond" | "milliseconds" => amount * 0.06,
                    _ => amount,
                };
                self.runtime.push_effect(Effect::Wait { ticks });
            }
            "hide" | "show" => {
                let visible = name.eq_ignore_ascii_case("show");
                let reference = object_argument(arguments, name)?;
                let object = self.resolve_object(reference)?;
                self.set_property(object, "visible", Value::Bool(visible))?;
            }
            "choose" | "domenu" | "play" | "visual" | "reset" | "unlock" | "lock" => {
                // Recognised, and deliberately no-ops until HyperLab has the
                // feature they refer to. Scripts written for HyperCard should
                // not fall over on a line that only affects appearance.
            }
            other => {
                return Err(RuntimeError::new(format!(
                    "I do not know how to \"{other}\""
                )));
            }
        }
        Ok(Flow::Normal)
    }

    /// Puts a question to a language model, and records that it was asked.
    ///
    /// The effect is pushed before the host is called, so a question that is
    /// refused — or that never comes back — still shows up in the record of
    /// what the script did.
    pub(crate) fn ask_assistant(&mut self, request: &AiRequest) -> Result<String, String> {
        self.runtime.push_effect(Effect::Assistant {
            prompt: request.prompt.clone(),
            intent: request.intent,
        });
        self.runtime.host_mut().ai(request)
    }

    /// Sends a message outwards from the object whose script is running.
    ///
    /// Returns `None` if nothing handled it.
    fn dispatch_from_me(
        &mut self,
        name: &str,
        arguments: &[Value],
    ) -> RuntimeResult<Option<Value>> {
        let Some(frame) = self.frames.last() else {
            return Ok(None);
        };
        let (path, start, target) = (frame.path.clone(), frame.path_index, frame.target);
        self.dispatch_along(&path, start, name, arguments, target)
    }
}

/// Pulls the object reference out of a command like `hide button "Go"`.
fn object_argument<'a>(
    arguments: &'a [Expr],
    name: &str,
) -> RuntimeResult<&'a hyperlab_parser::ast::ObjectRef> {
    match arguments.first() {
        Some(Expr::Object(reference)) => Ok(reference),
        _ => Err(RuntimeError::new(format!(
            "\"{name}\" needs an object, such as `{name} button \"Go\"`"
        ))),
    }
}

// ------------------------------------------------------------------- clocks

/// Seconds since the Unix epoch, from the same clock that stamps objects —
/// which is the one a host may have replaced on a platform with none.
fn unix_seconds() -> u64 {
    hyperlab_stack::now_millis() / 1_000
}

/// A random number, from a generator seeded by the clock.
///
/// This is not cryptography and does not pretend to be; it is the source of
/// `random(10)` and `any card`.
fn next_random() -> u64 {
    use std::cell::Cell;
    thread_local! {
        static STATE: Cell<u64> = const { Cell::new(0) };
    }
    STATE.with(|state| {
        let mut x = state.get();
        if x == 0 {
            // Seeded from the shared clock rather than the platform's, which
            // a browser does not have. The constant covers a clock stuck at
            // zero; the `| 1` keeps xorshift off its fixed point.
            let seed = hyperlab_stack::now_millis();
            x = if seed == 0 {
                0x2545_F491_4F6C_DD1D
            } else {
                seed
            } | 1;
        }
        // xorshift64: tiny, fast, and good enough to shuffle cards.
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        state.set(x);
        x
    })
}

/// A random position in a collection of `count` items.
pub(crate) fn random_index(count: usize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(usize::try_from(next_random() % count as u64).unwrap_or(0))
}

/// How `the date` should be written.
enum DateStyle {
    /// `1/8/26`
    Short,
    /// `Saturday, August 1, 2026`
    Long,
    /// `Sat, Aug 1, 2026`
    Abbreviated,
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const DAYS: [&str; 7] = [
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
];

/// Splits a Unix timestamp into a civil date.
///
/// This is Howard Hinnant's `civil_from_days`, which is short enough to carry
/// rather than take a dependency for.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = u32::try_from(day_of_year - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn format_date(seconds: u64, style: DateStyle) -> String {
    let days = (seconds / 86_400) as i64;
    let (year, month, day) = civil_from_days(days);
    let month_name = MONTHS[(month as usize - 1).min(11)];
    let weekday = DAYS[usize::try_from(days.rem_euclid(7)).unwrap_or(0)];
    match style {
        DateStyle::Short => format!("{month}/{day}/{:02}", year.rem_euclid(100)),
        DateStyle::Long => format!("{weekday}, {month_name} {day}, {year}"),
        DateStyle::Abbreviated => format!(
            "{}, {} {day}, {year}",
            &weekday[..3],
            &month_name[..3.min(month_name.len())]
        ),
    }
}

fn format_time(seconds: u64, long: bool) -> String {
    let day_seconds = seconds % 86_400;
    let (hour24, minute, second) = (
        day_seconds / 3600,
        (day_seconds % 3600) / 60,
        day_seconds % 60,
    );
    let suffix = if hour24 < 12 { "AM" } else { "PM" };
    let hour = match hour24 % 12 {
        0 => 12,
        other => other,
    };
    if long {
        format!("{hour}:{minute:02}:{second:02} {suffix}")
    } else {
        format!("{hour}:{minute:02} {suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_have_the_classic_values() {
        assert_eq!(constant("empty"), Some(Value::Empty));
        assert_eq!(constant("quote"), Some(Value::text("\"")));
        assert_eq!(constant("nonsense"), None);
    }

    #[test]
    fn functions_report_unknown_names_rather_than_guessing() {
        assert_eq!(call("nosuchfunction", &[]).unwrap(), None);
    }

    #[test]
    fn text_functions_count_characters_not_bytes() {
        assert_eq!(
            call("length", &[Value::text("héllo")]).unwrap(),
            Some(Value::Number(5.0))
        );
        assert_eq!(
            call("offset", &[Value::text("l"), Value::text("héllo")]).unwrap(),
            Some(Value::Number(3.0))
        );
        assert_eq!(
            call("offset", &[Value::text("z"), Value::text("héllo")]).unwrap(),
            Some(Value::Number(0.0))
        );
    }

    #[test]
    fn list_functions_accept_arguments_or_a_comma_separated_list() {
        assert_eq!(
            call("sum", &[Value::text("1,2,3")]).unwrap(),
            Some(Value::Number(6.0))
        );
        assert_eq!(
            call("max", &[Value::Number(1.0), Value::Number(9.0)]).unwrap(),
            Some(Value::Number(9.0))
        );
    }

    #[test]
    fn value_evaluates_arithmetic_and_nothing_else() {
        assert_eq!(
            call("value", &[Value::text("2 + 3 * 4")]).unwrap(),
            Some(Value::Number(14.0))
        );
        assert!(call("value", &[Value::text("field \"x\"")]).is_err());
    }

    #[test]
    fn random_stays_inside_its_range() {
        for _ in 0..100 {
            let value = call("random", &[Value::Number(6.0)])
                .unwrap()
                .unwrap()
                .as_number()
                .unwrap();
            assert!((1.0..=6.0).contains(&value), "{value} is out of range");
        }
    }

    #[test]
    fn dates_are_formatted_the_classic_way() {
        // 2026-07-26T12:34:56Z, a Sunday.
        let seconds = 1_785_069_296;
        assert_eq!(format_date(seconds, DateStyle::Short), "7/26/26");
        assert_eq!(
            format_date(seconds, DateStyle::Long),
            "Sunday, July 26, 2026"
        );
        assert_eq!(
            format_date(seconds, DateStyle::Abbreviated),
            "Sun, Jul 26, 2026"
        );
        assert_eq!(format_time(seconds, false), "12:34 PM");
        assert_eq!(format_time(seconds, true), "12:34:56 PM");
    }

    #[test]
    fn the_epoch_is_a_thursday() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(DAYS[0], "Thursday");
    }
}
