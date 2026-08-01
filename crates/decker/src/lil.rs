//! HyperTalk statements, as Lil.
//!
//! Lil is Decker's language and is not a HyperTalk: assignment is `x:1`, a
//! call is `f[a b]` with no commas, `if` takes no `then`, and blocks end with
//! a bare `end`. So this is a translation rather than the change of address
//! that _hyperscript was.
//!
//! Lil also evaluates right to left, with no precedence between operators, so
//! everything written here is parenthesised whether it looks like it needs to
//! be or not. `count "a " take s = "a "` is the sort of thing that reads
//! plainly, runs, and is wrong.
//!
//! What has no equivalent becomes a `#` comment where it belonged and a note
//! the caller can count, because a deck that silently does nothing is worse
//! than one that says which line it lost.

use hyperlab_parser::ast::{
    ArithmeticCommand, BinaryOp, Block, Branch, Chunk, ChunkKind, ContainerBase, CountTarget,
    Destination, ExitTarget, Expr, Handler, HandlerKind, ObjectRef, Ordinal, PartKind,
    RepeatControl, Specifier, Statement, StatementKind, UnaryOp,
};

/// HyperTalk's `it`, which is an ordinary variable here.
const IT: &str = "hlIt";

/// Which widgets a card has, so a script can be pointed at them.
pub trait Widgets {
    /// The widget name for a part, if this card has one.
    fn named(&self, kind: &str, name: &str) -> Option<String>;
    /// The widget name for a part on another card, and that card's name.
    ///
    /// A deck reaches across itself with `deck.cards.x.widgets.y`, so a script
    /// that writes into a card it is not on needs both halves. Cluedo's
    /// pickers are the whole reason this exists.
    fn elsewhere(&self, card: &str, kind: &str, name: &str) -> Option<(String, String)>;
    /// Whether a card of this name exists, for `go`.
    fn card(&self, name: &str) -> bool;
}

/// Translates one script into the Lil that goes in a `{script:…}` chunk.
///
/// Returns `None` for a script with nothing a deck can run, along with the
/// reasons why.
pub fn handlers(source: &str, widgets: &dyn Widgets) -> (Option<Handled>, Vec<String>) {
    let Ok(parsed) = hyperlab_parser::parse(source) else {
        return (
            None,
            vec!["a script that does not parse was left out".to_string()],
        );
    };
    let mut writer = Writer {
        widgets,
        notes: Vec::new(),
        depth: 0,
        temps: 0,
    };
    let mut written = Vec::new();
    let mut already = Vec::new();
    for handler in &parsed.handlers {
        let Some((event, body)) = writer.handler(handler) else {
            continue;
        };
        // One chunk holds every handler, and a deck sends each event once.
        if already.contains(&event) {
            writer
                .notes
                .push(format!("a second \"{event}\" handler was left out"));
            continue;
        }
        already.push(event.clone());
        written.push(format!("on {event} do\n{body}\nend"));
    }
    let handled = (!written.is_empty()).then(|| Handled {
        source: written.join("\n\n"),
    });
    (handled, writer.notes)
}

/// One translated script, ready to become a `{script:…}` chunk.
pub struct Handled {
    /// The Lil source, including each `on … do` and its `end`.
    pub source: String,
}

struct Writer<'a> {
    widgets: &'a dyn Widgets,
    notes: Vec<String>,
    depth: usize,
    /// How many working variables have been named, so no two collide.
    temps: usize,
}

impl Writer<'_> {
    fn note(&mut self, what: &str) -> String {
        self.notes.push(what.to_string());
        format!("# {what}")
    }

    fn indent(&self) -> String {
        " ".repeat(self.depth + 1)
    }

    /// The event a handler answers, and its body.
    fn handler(&mut self, handler: &Handler) -> Option<(String, String)> {
        // Decker has no user functions on a card, so only messages travel.
        if handler.kind == HandlerKind::Function {
            self.notes.push(format!(
                "the function \"{}\" has nowhere to live",
                handler.name
            ));
            return None;
        }
        let event = match handler.name.to_ascii_lowercase().as_str() {
            "mouseup" => "click",
            "opencard" => "view",
            // A deck's `view` is every card's arrival rather than the deck's
            // own, and it already opens on the first card. Making it `view`
            // would run the handler again on every move.
            "openstack" => {
                self.notes
                    .push("a deck has no moment of opening to run a script at".to_string());
                return None;
            }
            other => {
                self.notes
                    .push(format!("nothing in Decker sends \"{other}\""));
                return None;
            }
        };
        self.depth = 1;
        // A handler that gives up part way through is the same shape as a loop
        // that skips a turn, and gets the same rewrite.
        let body = self.block(&turn_inside_out(&handler.body, Leaves::Handler));
        Some((event.to_string(), body))
    }

    fn block(&mut self, block: &Block) -> String {
        block
            .iter()
            .map(|statement| {
                let line = self.statement(&statement.kind);
                if line.is_empty() {
                    line
                } else {
                    format!("{}{line}", self.indent())
                }
            })
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn statement(&mut self, statement: &StatementKind) -> String {
        match statement {
            StatementKind::Put {
                value,
                target,
                preposition,
            } => {
                let value = self.expr(value);
                let Some(target) = target else {
                    return format!("alert[({value})]");
                };
                let Some(place) = self.place(&target.base) else {
                    return self.note("that container has no equivalent in a deck");
                };
                use hyperlab_parser::ast::Preposition;
                if let [chunk] = target.chunks.as_slice() {
                    if *preposition != Preposition::Into {
                        return self.note("only replacing part of a container is translated");
                    }
                    return self.replace(&place, chunk, &value);
                }
                if !target.chunks.is_empty() {
                    return self.note("writing over part of part of a container is not translated");
                }
                match preposition {
                    Preposition::Into => format!("{place}:{value}"),
                    Preposition::Before => format!("{place}:(\"\" fuse (({value}),({place})))"),
                    Preposition::After => format!("{place}:(\"\" fuse (({place}),({value})))"),
                }
            }
            StatementKind::Get(value) => {
                let value = self.expr(value);
                format!("{IT}:{value}")
            }
            StatementKind::Arithmetic {
                operator,
                value,
                target,
            } => {
                let value = self.expr(value);
                let Some(place) = self.place(&target.base) else {
                    return self.note("that container cannot be counted in a deck");
                };
                let sign = match operator {
                    ArithmeticCommand::Add => "+",
                    ArithmeticCommand::Subtract => "-",
                    ArithmeticCommand::Multiply => "*",
                    ArithmeticCommand::Divide => "/",
                };
                format!("{place}:(({place}) {sign} ({value}))")
            }
            StatementKind::If {
                branches,
                otherwise,
            } => {
                let mut out = String::new();
                for (at, branch) in branches.iter().enumerate() {
                    let condition = self.expr(&branch.condition);
                    self.depth += 1;
                    let body = self.block(&branch.body);
                    self.depth -= 1;
                    let word = if at == 0 { "if" } else { "elseif" };
                    out.push_str(&format!("{word} {condition}\n{body}\n{}", self.indent()));
                }
                if let Some(otherwise) = otherwise {
                    self.depth += 1;
                    let body = self.block(otherwise);
                    self.depth -= 1;
                    out.push_str(&format!("else\n{body}\n{}", self.indent()));
                }
                out.push_str("end");
                out
            }
            StatementKind::Repeat { control, body } => self.repeat(control, body),
            StatementKind::Go(destination) => self.go(destination),
            StatementKind::Return(_) | StatementKind::Exit(ExitTarget::Repeat) => {
                // Lil has no `break` and no early return from a handler.
                self.note("leaving early is not translated")
            }
            StatementKind::Exit(_) => self.note("leaving early is not translated"),
            StatementKind::NextRepeat => self.note("\"next repeat\" is not translated"),
            StatementKind::Global(_) => String::new(),
            StatementKind::Pass(_) => self.note("there is no message path to pass along"),
            StatementKind::Send { .. } => self.note("sending a message is not translated"),
            StatementKind::Set { property, .. } => {
                self.note(&format!("setting \"{property}\" is not translated"))
            }
            StatementKind::Command { name, arguments } => self.command(name, arguments),
        }
    }

    fn repeat(&mut self, control: &RepeatControl, body: &Block) -> String {
        // Every loop becomes a `while`, and Lil has nothing that skips to the
        // next turn of one. What is left after the rewrite would skip the step
        // that ends the loop — an infinite loop rather than a wrong answer —
        // so it is refused instead of written.
        let body = &turn_inside_out(body, Leaves::Turn);
        if still_leaves(body, Leaves::Turn) {
            return self.note("a loop with \"next repeat\" in it is not translated");
        }
        match control {
            RepeatControl::While(condition) => {
                let condition = self.expr(condition);
                self.depth += 1;
                let inside = self.block(body);
                self.depth -= 1;
                format!("while {condition}\n{inside}\n{}end", self.indent())
            }
            RepeatControl::Until(condition) => {
                let condition = self.expr(condition);
                self.depth += 1;
                let inside = self.block(body);
                self.depth -= 1;
                format!("while ~({condition})\n{inside}\n{}end", self.indent())
            }
            RepeatControl::Times(count) => {
                let counter = format!("hlN{}", self.depth);
                let count = self.expr(count);
                self.depth += 1;
                let inside = self.block(body);
                let step = self.indent();
                self.depth -= 1;
                format!(
                    "{counter}:0\n{}while (({counter}) < ({count}))\n{inside}\n{step}{counter}:(({counter}) + 1)\n{}end",
                    self.indent(),
                    self.indent()
                )
            }
            RepeatControl::With {
                variable,
                from,
                to,
                down,
            } => {
                let from = self.expr(from);
                let to = self.expr(to);
                // Lil has `<` and `>` and nothing that takes the ends in too,
                // so the test is the other one negated.
                let (past, step) = if *down { ("<", "-") } else { (">", "+") };
                self.depth += 1;
                let inside = self.block(body);
                let tail = self.indent();
                self.depth -= 1;
                format!(
                    "{variable}:{from}\n{}while !(({variable}) {past} ({to}))\n{inside}\n{tail}{variable}:(({variable}) {step} 1)\n{}end",
                    self.indent(),
                    self.indent()
                )
            }
            RepeatControl::Forever => self.note("a loop with no end would hang a deck"),
        }
    }

    fn go(&mut self, destination: &Destination) -> String {
        match destination {
            Destination::Back => "go[\"Back\"]".to_string(),
            Destination::Card(specifier) => match specifier {
                Specifier::Ordinal(Ordinal::First) => "go[\"First\"]".to_string(),
                Specifier::Ordinal(Ordinal::Last) => "go[\"Last\"]".to_string(),
                Specifier::Ordinal(Ordinal::Next) => "go[\"Next\"]".to_string(),
                Specifier::Ordinal(Ordinal::Previous) => "go[\"Prev\"]".to_string(),
                Specifier::Value(Expr::Text(name)) if self.widgets.card(name) => {
                    format!("go[\"{}\"]", crate::deck::identifier(name))
                }
                Specifier::Value(Expr::Text(name)) => {
                    self.note(&format!("there is no card called \"{name}\""))
                }
                Specifier::Value(Expr::Number(position)) => {
                    // Decker counts cards from zero and HyperTalk from one.
                    format!("go[{}]", (*position as i64 - 1).max(0))
                }
                _ => self.note("that card is chosen at run time"),
            },
        }
    }

    fn command(&mut self, name: &str, arguments: &[Expr]) -> String {
        let first = arguments
            .first()
            .map_or_else(|| "\"\"".to_string(), |value| self.expr(value));
        match name.to_ascii_lowercase().as_str() {
            "answer" => format!("alert[({first})]"),
            "ask" => {
                let fallback = arguments
                    .get(1)
                    .map_or_else(|| "\"\"".to_string(), |value| self.expr(value));
                // The modal suspends the script and hands back what was typed.
                format!("{IT}:alert[({first}) \"string\" ({fallback})]")
            }
            "beep" => self.note("there is no beep in a deck"),
            other => self.note(&format!("\"{other}\" is not translated")),
        }
    }

    /// Writing over one chunk of a container: `put x into line 2 of y`.
    ///
    /// Lil has no chunks, so the container is split, the pieces either side of
    /// the range are kept, and the lot is joined again. The split lands in a
    /// variable because it is read twice and the container may be a widget.
    fn replace(&mut self, place: &str, chunk: &Chunk, value: &str) -> String {
        let separator = between(chunk.kind);
        let start = self.expr(&chunk.start);
        let last = match &chunk.end {
            Some(end) => self.expr(end),
            None => start.clone(),
        };
        self.temps += 1;
        let pieces = format!("hlBit{}", self.temps);
        format!(
            "{pieces}:({separator} split ({place}))\n{}{place}:({separator} fuse \
             ((((({start})-1) take {pieces}),({value})),(({last}) drop {pieces})))",
            self.indent()
        )
    }

    /// Where a container lives in Lil.
    fn place(&mut self, base: &ContainerBase) -> Option<String> {
        match base {
            ContainerBase::Variable(name) => Some(name.clone()),
            ContainerBase::It => Some(IT.to_string()),
            ContainerBase::Object(object) => self.widget(object).map(|name| format!("{name}.text")),
            ContainerBase::MessageBox => None,
        }
    }

    /// What a script says to reach a part, if the deck has one for it.
    fn widget(&mut self, object: &ObjectRef) -> Option<String> {
        let ObjectRef::Part {
            kind,
            specifier,
            owner,
            ..
        } = object
        else {
            return None;
        };
        let Specifier::Value(Expr::Text(name)) = specifier.as_ref() else {
            return None;
        };
        let kind = match kind {
            PartKind::Button => "button",
            PartKind::Field => "field",
            PartKind::Image => "image",
        };
        match owner.as_deref() {
            None => self.widgets.named(kind, name),
            Some(ObjectRef::Card(which)) => match which.as_ref() {
                Specifier::Current => self.widgets.named(kind, name),
                Specifier::Value(Expr::Text(card)) => self
                    .widgets
                    .elsewhere(card, kind, name)
                    .map(|(card, widget)| format!("deck.cards.{card}.widgets.{widget}")),
                _ => None,
            },
            Some(_) => None,
        }
    }

    fn expr(&mut self, expression: &Expr) -> String {
        match expression {
            Expr::Number(number) => {
                if number.fract() == 0.0 && number.abs() < 1e15 {
                    format!("{}", *number as i64)
                } else {
                    format!("{number}")
                }
            }
            Expr::Text(text) => quoted(text),
            Expr::Constant(name) => match name.to_ascii_lowercase().as_str() {
                "true" => "1".to_string(),
                "false" => "0".to_string(),
                "empty" => "\"\"".to_string(),
                "space" => "\" \"".to_string(),
                "return" | "newline" => "\"\\n\"".to_string(),
                "comma" => "\",\"".to_string(),
                "zero" => "0".to_string(),
                other => quoted(other),
            },
            Expr::Variable(name) => name.clone(),
            Expr::It => IT.to_string(),
            Expr::Object(object) => match self.widget(object) {
                Some(name) => format!("{name}.text"),
                None => {
                    self.notes
                        .push("a part the deck does not have was read".to_string());
                    "\"\"".to_string()
                }
            },
            Expr::Unary { operator, operand } => {
                let operand = self.expr(operand);
                match operator {
                    UnaryOp::Negate => format!("(0-({operand}))"),
                    // `!`, not `~`: `~` is Lil's match, and takes two operands.
                    UnaryOp::Not => format!("(!({operand}))"),
                }
            }
            Expr::Binary {
                operator,
                left,
                right,
            } => {
                let left = self.expr(left);
                let right = self.expr(right);
                let infix = |sign: &str| format!("(({left}) {sign} ({right}))");
                match operator {
                    BinaryOp::Add => infix("+"),
                    BinaryOp::Subtract => infix("-"),
                    BinaryOp::Multiply => infix("*"),
                    BinaryOp::Divide => infix("/"),
                    // Lil's `%` divides its right operand by its left, which
                    // is the way round HyperTalk does not read.
                    BinaryOp::Modulo => format!("(({right}) % ({left}))"),
                    BinaryOp::Power => infix("^"),
                    BinaryOp::IntegerDivide => format!("(floor (({left}) / ({right})))"),
                    BinaryOp::Equal => infix("="),
                    BinaryOp::NotEqual => format!("(!(({left}) = ({right})))"),
                    BinaryOp::Less => infix("<"),
                    BinaryOp::Greater => infix(">"),
                    BinaryOp::LessOrEqual => format!("(!(({left}) > ({right})))"),
                    BinaryOp::GreaterOrEqual => format!("(!(({left}) < ({right})))"),
                    // Min and max, which for a nothing-or-one truth is and
                    // and or. Every comparison above yields one of the two.
                    BinaryOp::And => infix("&"),
                    BinaryOp::Or => infix("|"),
                    BinaryOp::Contains => format!("(({right}) in ({left}))"),
                    BinaryOp::IsIn => format!("(({left}) in ({right}))"),
                    BinaryOp::StartsWith => {
                        format!("((((count ({right})) take ({left}))) = ({right}))")
                    }
                    // A negative count takes from the end of a string.
                    BinaryOp::EndsWith => {
                        format!("((((0-(count ({right}))) take ({left}))) = ({right}))")
                    }
                    // `fuse` joins with a separator, and an empty one joins
                    // plainly, which is what `&` does in HyperTalk.
                    BinaryOp::Concat => format!("(\"\" fuse (({left}),({right})))"),
                    BinaryOp::ConcatSpace => format!("(\" \" fuse (({left}),({right})))"),
                }
            }
            Expr::Count(target) => self.count(target),
            Expr::Chunk { chunks, source } => self.chunk(chunks, source),
            Expr::Call { name, arguments } => self.call(name, arguments),
            Expr::Of { name, operand } => {
                let operand = self.expr(operand);
                if name == "length" {
                    format!("(count ({operand}))")
                } else {
                    self.notes
                        .push(format!("\"the {name} of …\" is not translated"));
                    "\"\"".to_string()
                }
            }
            Expr::The(name) => {
                self.notes.push(format!("\"the {name}\" is not translated"));
                "\"\"".to_string()
            }
            Expr::Exists { .. } => {
                self.notes
                    .push("\"there is a …\" is not translated".to_string());
                "0".to_string()
            }
        }
    }

    fn count(&mut self, target: &CountTarget) -> String {
        match target {
            CountTarget::Cards => "(count deck.cards)".to_string(),
            CountTarget::Chunks { kind, source } => {
                let source = self.expr(source);
                let separator = between(*kind);
                format!("(count ({separator} split ({source})))")
            }
            CountTarget::Backgrounds | CountTarget::Parts { .. } => {
                self.notes
                    .push("counting the stack's own objects is not translated".to_string());
                "0".to_string()
            }
        }
    }

    /// `line 2 of x`, and the rest of the family.
    ///
    /// Lil has no chunks, so each one is a split, a slice and a join. Chunks
    /// nest outermost first, so they are applied in the reverse of the order
    /// they are written in.
    fn chunk(&mut self, chunks: &[Chunk], source: &Expr) -> String {
        let mut out = self.expr(source);
        for chunk in chunks.iter().rev() {
            let separator = between(chunk.kind);
            let start = self.expr(&chunk.start);
            let howmany = match &chunk.end {
                Some(end) => {
                    let end = self.expr(end);
                    format!("(1+(({end})-({start})))")
                }
                None => "1".to_string(),
            };
            out = format!(
                "({separator} fuse ({howmany} take ((({start})-1) drop ({separator} split ({out})))))"
            );
        }
        out
    }

    fn call(&mut self, name: &str, arguments: &[Expr]) -> String {
        let written: Vec<String> = arguments.iter().map(|value| self.expr(value)).collect();
        let first = written
            .first()
            .cloned()
            .unwrap_or_else(|| "\"\"".to_string());
        match name.to_ascii_lowercase().as_str() {
            "length" => format!("(count ({first}))"),
            // Nothing casts in Lil; arithmetic reads a number out of text.
            "number" | "value" => format!("(0+({first}))"),
            "abs" => format!("(({first}) | (0-({first})))"),
            "round" => format!("(floor (0.5+({first})))"),
            "trunc" => format!("(floor ({first}))"),
            "sqrt" => format!("(sqrt ({first}))"),
            "random" => format!("(1+(random[({first})]))"),
            "min" => format!("({})", written.join(" & ")),
            "max" => format!("({})", written.join(" | ")),
            other => {
                self.notes.push(format!("\"{other}\" is not translated"));
                "\"\"".to_string()
            }
        }
    }
}

/// What separates one chunk from the next.
const fn between(kind: ChunkKind) -> &'static str {
    match kind {
        ChunkKind::Char => "\"\"",
        ChunkKind::Word => "\" \"",
        ChunkKind::Item => "\",\"",
        ChunkKind::Line => "\"\\n\"",
    }
}

/// What a statement can abandon.
#[derive(Clone, Copy, PartialEq)]
enum Leaves {
    /// `next repeat`: the rest of this turn of the nearest loop.
    Turn,
    /// `exit <handler>` and its relatives: the rest of the handler.
    Handler,
}

impl Leaves {
    fn matches(self, statement: &StatementKind) -> bool {
        match self {
            Self::Turn => matches!(statement, StatementKind::NextRepeat),
            Self::Handler => matches!(
                statement,
                StatementKind::Exit(ExitTarget::Handler(_) | ExitTarget::Everything)
                    | StatementKind::Return(None)
            ),
        }
    }
}

/// Turns a statement that abandons the rest of a block inside out.
///
/// Lil has nothing that jumps, but abandoning the rest is the same as running
/// the rest only when the guard was false, and that it can say. So
/// `if x then next repeat` becomes `if !x … end`, and a guard that does
/// something before it leaves gets the rest as its `else`. Anything after an
/// unconditional one is dropped, because it could never have run.
///
/// `stop` is `next repeat` inside a loop and `exit` inside a handler; the
/// shape of the rewrite is the same either way.
fn turn_inside_out(body: &[Statement], stop: Leaves) -> Block {
    let mut out = Vec::with_capacity(body.len());
    for (at, statement) in body.iter().enumerate() {
        if stop.matches(&statement.kind) {
            return out;
        }
        let rest = || turn_inside_out(&body[at + 1..], stop);
        let StatementKind::If {
            branches,
            otherwise: None,
        } = &statement.kind
        else {
            out.push(statement.clone());
            continue;
        };
        let [only] = branches.as_slice() else {
            out.push(statement.clone());
            continue;
        };
        let Some((last, before)) = only.body.split_last() else {
            out.push(statement.clone());
            continue;
        };
        if !stop.matches(&last.kind) {
            out.push(statement.clone());
            continue;
        }
        // A guard that does nothing but leave reads better negated than as an
        // `if` with an empty arm.
        out.push(Statement::new(
            if before.is_empty() {
                StatementKind::If {
                    branches: vec![Branch {
                        condition: Expr::Unary {
                            operator: UnaryOp::Not,
                            operand: Box::new(only.condition.clone()),
                        },
                        body: rest(),
                    }],
                    otherwise: None,
                }
            } else {
                StatementKind::If {
                    branches: vec![Branch {
                        condition: only.condition.clone(),
                        body: turn_inside_out(before, stop),
                    }],
                    otherwise: Some(rest()),
                }
            },
            statement.line,
        ));
        return out;
    }
    out
}

/// Whether a block still leaves early, at any depth.
fn still_leaves(block: &[Statement], stop: Leaves) -> bool {
    block.iter().any(|statement| {
        stop.matches(&statement.kind)
            || match &statement.kind {
                StatementKind::If {
                    branches,
                    otherwise,
                } => {
                    branches
                        .iter()
                        .any(|branch| still_leaves(&branch.body, stop))
                        || otherwise
                            .as_ref()
                            .is_some_and(|body| still_leaves(body, stop))
                }
                // A `next repeat` belongs to the nearest loop, so one further
                // in is that loop's business rather than this one's.
                StatementKind::Repeat { body, .. } => {
                    stop != Leaves::Turn && still_leaves(body, stop)
                }
                _ => false,
            }
    })
}

/// A Lil string literal.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Everything;
    impl Widgets for Everything {
        fn named(&self, _kind: &str, name: &str) -> Option<String> {
            Some(crate::deck::identifier(name))
        }
        fn elsewhere(&self, card: &str, _kind: &str, name: &str) -> Option<(String, String)> {
            Some((crate::deck::identifier(card), crate::deck::identifier(name)))
        }
        fn card(&self, _name: &str) -> bool {
            true
        }
    }

    fn out(source: &str) -> String {
        handlers(source, &Everything)
            .0
            .map_or_else(String::new, |one| one.source)
    }

    fn notes(source: &str) -> Vec<String> {
        handlers(source, &Everything).1
    }

    #[test]
    fn a_mouse_handler_becomes_a_click_handler() {
        let written = out("on mouseUp\n  answer \"hi\"\nend mouseUp");
        assert!(written.starts_with("on click do\n"), "{written}");
        assert!(written.ends_with("\nend"), "{written}");
        assert!(written.contains("alert[(\"hi\")]"), "{written}");
    }

    #[test]
    fn assignment_is_a_colon_and_a_call_has_no_commas() {
        let written = out("on mouseUp\n  put \"x\" into field \"Notes\"\nend mouseUp");
        assert!(written.contains("notes.text:\"x\""), "{written}");
    }

    #[test]
    fn navigation_uses_deckers_own_words() {
        let written = out("on mouseUp\n  go to next card\nend mouseUp");
        assert!(written.contains("go[\"Next\"]"), "{written}");
        // Decker counts cards from zero, HyperTalk from one.
        let numbered = out("on mouseUp\n  go to card 3\nend mouseUp");
        assert!(numbered.contains("go[2]"), "{numbered}");
    }

    #[test]
    fn a_loop_that_would_never_end_is_refused_rather_than_written() {
        // One with an `else` cannot be turned inside out, and left as it is it
        // would skip the step that ends the `while`. A deck that hangs is
        // worse than a deck that says which line it left out.
        let source = concat!(
            "on mouseUp\n",
            "  repeat with i = 1 to 3\n",
            "    if i = 2 then\n",
            "      next repeat\n",
            "    else\n",
            "      add 1 to total\n",
            "    end if\n",
            "  end repeat\n",
            "end mouseUp"
        );
        assert!(out(source).contains("# a loop with"), "{}", out(source));
        assert_eq!(notes(source).len(), 1);
    }

    #[test]
    fn a_guard_that_skips_a_turn_is_turned_inside_out() {
        // Skipping the rest of a turn is running the rest only when the guard
        // is false, which is something Lil can say.
        let written = out(concat!(
            "on mouseUp\n",
            "  repeat with i = 1 to 3\n",
            "    if i = 2 then next repeat\n",
            "    add 1 to total\n",
            "  end repeat\n",
            "end mouseUp"
        ));
        assert!(written.contains("if (!(((i) = (2))))"), "{written}");
        assert!(written.contains("total:((total) + (1))"), "{written}");
        assert!(!written.contains('#'), "{written}");
    }

    #[test]
    fn a_field_on_another_card_is_reached_through_the_deck() {
        let written = out(
            "on mouseUp\n  put \"x\" into field \"Suspect\" of card \"The Mansion\"\nend mouseUp",
        );
        assert!(
            written.contains("deck.cards.themansion.widgets.suspect.text:\"x\""),
            "{written}"
        );
    }

    #[test]
    fn a_chunk_is_a_split_a_slice_and_a_join() {
        let written = out("on mouseUp\n  put line 2 of x into y\nend mouseUp");
        assert!(written.contains("\"\\n\" split"), "{written}");
        assert!(written.contains("\"\\n\" fuse"), "{written}");
        assert!(written.contains("drop"), "{written}");
    }

    #[test]
    fn counting_cards_asks_the_deck() {
        let written = out("on mouseUp\n  answer the number of cards\nend mouseUp");
        assert!(written.contains("(count deck.cards)"), "{written}");
    }

    #[test]
    fn an_ordinary_counting_loop_is_written_out() {
        let written = out(
            "on mouseUp\n  repeat with i = 1 to 3\n    add 1 to total\n  end repeat\nend mouseUp",
        );
        assert!(written.contains("i:1"), "{written}");
        assert!(written.contains("while !((i) > (3))"), "{written}");
        assert!(written.contains("i:((i) + 1)"), "{written}");
    }

    #[test]
    fn joining_text_uses_fuse() {
        let written = out("on mouseUp\n  put \"a\" & \"b\" into x\nend mouseUp");
        assert!(written.contains("fuse"), "{written}");
    }

    #[test]
    fn a_message_decker_never_sends_is_left_out_with_a_reason() {
        assert_eq!(notes("on mouseEnter\n  beep\nend mouseEnter").len(), 1);
        assert!(out("on mouseEnter\n  beep\nend mouseEnter").is_empty());
    }
}
