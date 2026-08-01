//! HyperTalk statements and expressions, as _hyperscript.
//!
//! The two languages agree about more than they disagree, so most of what
//! follows is a rename. The interesting parts are the three places they part
//! company, each documented where it is handled and listed in the crate
//! header.

use crate::Translation;
use hyperlab_parser::ast::{
    ArithmeticCommand, BinaryOp, Block, Chunk, Container, ContainerBase, CountTarget, Destination,
    ExitTarget, Expr, Handler, HandlerKind, ObjectRef, Ordinal, PartKind, Preposition,
    RepeatControl, Specifier, StatementKind, UnaryOp,
};

/// HyperTalk's `it`, which cannot keep its name.
///
/// `it` in _hyperscript is the previous command's result. Assigning to it is
/// accepted and yields `null`, which is the worst kind of difference: it
/// parses, runs, and is wrong.
pub const IT: &str = "hlIt";

/// How a script's object references become element ids.
///
/// The page knows which card it is translating and can resolve `field "Notes"`
/// to the one element that means, exactly as the runtime resolves it against
/// the card and then the background. Anything translating a script on its own
/// has no card to ask, and says so.
pub trait Elements {
    /// The id of the named part, without the `#`.
    ///
    /// `kind` is `"button"`, `"field"` or `"image"`. A word rather than either
    /// of the two `PartKind` enums, because the grammar has one and the object
    /// model has another and this sits between them.
    fn id(&self, kind: &str, name: &str) -> Option<String>;

    /// The id of the card at a one-based position, for `go to card 3`.
    fn card_at(&self, position: usize) -> Option<String>;

    /// The id of the named card.
    fn card_named(&self, name: &str) -> Option<String>;

    /// The id of a part on a named card, for `field "X" of card "Y"`.
    ///
    /// A card's own parts are not reachable from another card by name alone,
    /// so a stack that hands work between cards says which one it means. The
    /// picker cards in Cluedo are the whole reason this exists.
    fn id_on_card(&self, card: &str, kind: &str, name: &str) -> Option<String>;
}

/// An [`Elements`] that knows nothing, for translating a script by itself.
pub struct Unknown;

impl Elements for Unknown {
    fn id(&self, _kind: &str, _name: &str) -> Option<String> {
        None
    }
    fn card_at(&self, _position: usize) -> Option<String> {
        None
    }
    fn card_named(&self, _name: &str) -> Option<String> {
        None
    }
    fn id_on_card(&self, _card: &str, _kind: &str, _name: &str) -> Option<String> {
        None
    }
}

/// Translates the handlers in a HyperTalk script.
///
/// # Errors
///
/// Returns the parser's complaint if the script does not parse.
pub fn script(source: &str) -> Result<Translation, String> {
    let parsed = hyperlab_parser::parse(source).map_err(|error| error.to_string())?;
    let mut writer = Writer::new(&Unknown);
    let mut pieces = Vec::new();
    for handler in &parsed.handlers {
        pieces.push(writer.handler(handler));
    }
    Ok(Translation {
        source: pieces.join("\n\n"),
        notes: writer.notes,
    })
}

/// Translates one script against a page that knows where things are.
pub fn handlers(source: &str, elements: &dyn Elements) -> Translation {
    let Ok(parsed) = hyperlab_parser::parse(source) else {
        return Translation {
            source: String::new(),
            notes: vec!["a script that does not parse was left out".to_string()],
        };
    };
    let mut writer = Writer::new(elements);
    let mut pieces = Vec::new();
    for handler in &parsed.handlers {
        pieces.push(writer.handler(handler));
    }
    Translation {
        source: pieces.join(" "),
        notes: writer.notes,
    }
}

/// Builds _hyperscript, one statement at a time.
struct Writer<'a> {
    elements: &'a dyn Elements,
    notes: Vec<String>,
    /// Nesting depth of `repeat … times index`, so nested loops do not share
    /// a counter name.
    depth: usize,
}

impl<'a> Writer<'a> {
    fn new(elements: &'a dyn Elements) -> Self {
        Self {
            elements,
            notes: Vec::new(),
            depth: 0,
        }
    }

    fn note(&mut self, what: &str) -> String {
        self.notes.push(what.to_string());
        format!("-- {what}")
    }

    // ------------------------------------------------------------- handlers

    fn handler(&mut self, handler: &Handler) -> String {
        if handler.kind == HandlerKind::Function {
            let body = self.block(&handler.body);
            return format!(
                "def {}({}) {body} end",
                handler.name,
                handler.parameters.join(", ")
            );
        }
        let body = self.block(&handler.body);
        format!("on {} {body}", event(&handler.name))
    }

    fn block(&mut self, block: &Block) -> String {
        block
            .iter()
            .map(|statement| self.statement(&statement.kind))
            .collect::<Vec<_>>()
            .join(" ")
    }

    // ----------------------------------------------------------- statements

    fn statement(&mut self, statement: &StatementKind) -> String {
        match statement {
            StatementKind::Put {
                value,
                target,
                preposition,
            } => self.put(value, target.as_ref(), *preposition),
            StatementKind::Get(value) => {
                let value = self.expr(value);
                format!("set {IT} to {value}")
            }
            StatementKind::Set {
                property,
                object,
                value,
            } => self.set_property(property, object.as_ref(), value),
            StatementKind::Arithmetic {
                operator,
                value,
                target,
            } => self.arithmetic(*operator, value, target),
            StatementKind::If {
                branches,
                otherwise,
            } => {
                let mut out = String::new();
                for (at, branch) in branches.iter().enumerate() {
                    let condition = self.expr(&branch.condition);
                    let body = self.block(&branch.body);
                    if at == 0 {
                        out.push_str(&format!("if {condition} then {body}"));
                    } else {
                        out.push_str(&format!(" else if {condition} then {body}"));
                    }
                }
                if let Some(otherwise) = otherwise {
                    let body = self.block(otherwise);
                    out.push_str(&format!(" else {body}"));
                }
                out.push_str(" end");
                out
            }
            StatementKind::Repeat { control, body } => self.repeat(control, body),
            StatementKind::Exit(ExitTarget::Repeat) => "break".to_string(),
            StatementKind::Exit(_) => "return".to_string(),
            StatementKind::NextRepeat => "continue".to_string(),
            StatementKind::Return(value) => match value {
                Some(value) => {
                    let value = self.expr(value);
                    format!("return {value}")
                }
                None => "return".to_string(),
            },
            StatementKind::Go(destination) => self.go(destination),
            StatementKind::Send { message, target } => {
                let Expr::Text(name) = message else {
                    return self.note("a message worked out at run time cannot be sent");
                };
                match self.element(target) {
                    Some(id) => format!("send {} to #{id}", event(name)),
                    None => self.note(&format!("nowhere to send \"{name}\"")),
                }
            }
            StatementKind::Global(_) => {
                // Every variable in a handler is already page-wide here, so a
                // declaration has nothing left to say.
                String::new()
            }
            StatementKind::Pass(message) => self.note(&format!(
                "\"pass {message}\" has no message path to pass to"
            )),
            StatementKind::Command { name, arguments } => self.command(name, arguments),
        }
    }

    fn put(
        &mut self,
        value: &Expr,
        target: Option<&Container>,
        preposition: Preposition,
    ) -> String {
        let value = self.expr(value);
        let Some(target) = target else {
            // The message box is HyperLab's scratch line. A page has none, and
            // the console is the honest stand-in.
            return format!("log {value}");
        };
        let (read, write) = match self.place(&target.base) {
            Some(pair) => pair,
            None => return self.note("that container has no equivalent on a page"),
        };

        if let [chunk] = target.chunks.as_slice() {
            // Writing over part of a container: the page has a helper for it,
            // because splitting, replacing a range and rejoining is four lines
            // of _hyperscript and one call.
            if preposition != Preposition::Into {
                return self.note("only replacing part of a container is translated");
            }
            let kind = chunk.kind.as_str();
            let from = self.expr(&chunk.start);
            let to = match &chunk.end {
                Some(end) => self.expr(end),
                None => from.clone(),
            };
            return format!(
                "set {write} to hlSplice({read} as String, '{kind}',                  {from} as Int, {to} as Int, {value} as String)"
            );
        }
        if !target.chunks.is_empty() {
            return self.note("putting into part of part of a container is not translated");
        }

        match preposition {
            Preposition::Into => format!("set {write} to {value}"),
            Preposition::Before => format!("set {write} to ({value} + {read})"),
            Preposition::After => format!("set {write} to ({read} + {value})"),
        }
    }

    /// How to read a container, and how to write it.
    ///
    /// A variable is the same word twice. A field is an element, and the two
    /// differ: `the value of #x` reads, `#x's value` is assigned to.
    fn place(&mut self, base: &ContainerBase) -> Option<(String, String)> {
        match base {
            ContainerBase::Variable(name) => Some((name.clone(), name.clone())),
            ContainerBase::It => Some((IT.to_string(), IT.to_string())),
            ContainerBase::Object(object) => {
                let id = self.element(object)?;
                Some((format!("(the value of #{id})"), format!("#{id}'s value")))
            }
            ContainerBase::MessageBox => None,
        }
    }

    fn set_property(&mut self, property: &str, object: Option<&ObjectRef>, value: &Expr) -> String {
        let Some(object) = object else {
            return self.note(&format!("\"the {property}\" is not a thing a page has"));
        };
        let Some(id) = self.element(object) else {
            return self.note(&format!("nothing on the page to set the {property} of"));
        };
        let value = self.expr(value);
        match property {
            "text" => format!("set #{id}'s value to {value}"),
            "visible" => format!("if {value} then show #{id} else hide #{id} end"),
            "enabled" => format!("set #{id}'s disabled to not ({value})"),
            "name" => self.note("renaming a part on the page is not translated"),
            other => self.note(&format!("\"{other}\" has no equivalent on a page")),
        }
    }

    fn arithmetic(
        &mut self,
        operator: ArithmeticCommand,
        value: &Expr,
        target: &Container,
    ) -> String {
        let value = self.expr(value);
        let Some((read, write)) = self.place(&target.base) else {
            return self.note("that container cannot be counted on a page");
        };
        let sign = match operator {
            ArithmeticCommand::Add => "+",
            ArithmeticCommand::Subtract => "-",
            ArithmeticCommand::Multiply => "*",
            ArithmeticCommand::Divide => "/",
        };
        format!("set {write} to (({read} as Float) {sign} ({value} as Float))")
    }

    fn repeat(&mut self, control: &RepeatControl, body: &Block) -> String {
        match control {
            RepeatControl::Forever => {
                let body = self.block(body);
                format!("repeat forever {body} end")
            }
            RepeatControl::Times(count) => {
                // The count has to be a name: `repeat (a + b) times` is a
                // parse error, and `repeat n times` is not.
                let howmany = format!("hlTimes{}", self.depth);
                let count = self.expr(count);
                self.depth += 1;
                let body = self.block(body);
                self.depth -= 1;
                format!("set {howmany} to ({count} as Int) repeat {howmany} times {body} end")
            }
            RepeatControl::While(condition) => {
                let condition = self.expr(condition);
                let body = self.block(body);
                format!("repeat while {condition} {body} end")
            }
            RepeatControl::Until(condition) => {
                let condition = self.expr(condition);
                let body = self.block(body);
                format!("repeat until {condition} {body} end")
            }
            RepeatControl::With {
                variable,
                from,
                to,
                down,
            } => {
                // `repeat with i = 1 to n` is a parse error in _hyperscript, so
                // the count is done by the language and the variable derived
                // from its index. A hand-rolled counter would look tidier and
                // be wrong: `next repeat` becomes `continue`, which would skip
                // the increment and loop for ever.
                let depth = self.depth;
                let (counter, first, howmany) = (
                    format!("hlStep{depth}"),
                    format!("hlFrom{depth}"),
                    format!("hlTimes{depth}"),
                );
                let from = self.expr(from);
                let to = self.expr(to);
                let step = if *down { "-" } else { "+" };
                self.depth += 1;
                let body = self.block(body);
                self.depth -= 1;
                format!(
                    "set {first} to ({from} as Int) \
                     set {howmany} to Math.abs(({to} as Int) - {first}) + 1 \
                     repeat {howmany} times index {counter} \
                     set {variable} to {first} {step} {counter} {body} end"
                )
            }
        }
    }

    fn go(&mut self, destination: &Destination) -> String {
        match destination {
            Destination::Back => "call hlBack()".to_string(),
            Destination::Card(specifier) => match specifier {
                Specifier::Current => "call hlGo(hlCard)".to_string(),
                Specifier::Ordinal(Ordinal::First) => "call hlGo(0)".to_string(),
                Specifier::Ordinal(Ordinal::Last) => "call hlGo(-1)".to_string(),
                Specifier::Ordinal(Ordinal::Next) => "call hlGo(hlCard + 1)".to_string(),
                Specifier::Ordinal(Ordinal::Previous) => "call hlGo(hlCard - 1)".to_string(),
                Specifier::Ordinal(_) => self.note("that card is chosen at run time"),
                Specifier::Value(Expr::Text(name)) => match self.elements.card_named(name) {
                    Some(id) => format!("call hlGoTo('{id}')"),
                    None => self.note(&format!("there is no card called \"{name}\"")),
                },
                Specifier::Value(Expr::Number(position)) => {
                    let index = *position as usize;
                    match self.elements.card_at(index) {
                        Some(id) => format!("call hlGoTo('{id}')"),
                        None => self.note(&format!("there is no card {index}")),
                    }
                }
                Specifier::Value(_) | Specifier::Id(_) => {
                    self.note("that card is chosen at run time")
                }
            },
        }
    }

    fn command(&mut self, name: &str, arguments: &[Expr]) -> String {
        let lowered = name.to_ascii_lowercase();
        let argument = |writer: &mut Self, at: usize| {
            arguments
                .get(at)
                .map_or_else(|| "''".to_string(), |value| writer.expr(value))
        };
        match lowered.as_str() {
            "answer" => {
                let message = argument(self, 0);
                format!("call alert({message})")
            }
            "ask" => {
                let prompt = argument(self, 0);
                let fallback = if arguments.len() > 1 {
                    argument(self, 1)
                } else {
                    "''".to_string()
                };
                // `prompt` gives back null when it is cancelled, and HyperTalk
                // promises empty, so the two are reconciled here rather than in
                // every script that asks.
                format!("set {IT} to (prompt({prompt}, {fallback}) or '')")
            }
            "beep" => self.note("there is no beep on a page"),
            "ask assistant" => self.note("a page has no assistant to ask"),
            "wait" => {
                let howlong = argument(self, 0);
                format!("wait ({howlong} as Int) * 60 ms")
            }
            "show" | "hide" => {
                let Some(Expr::Object(object)) = arguments.first() else {
                    return self.note(&format!("nothing named to {lowered}"));
                };
                match self.element(object) {
                    Some(id) => format!("{lowered} #{id}"),
                    None => self.note(&format!("nothing on the page to {lowered}")),
                }
            }
            "put" | "add" | "choose" | "domenu" | "play" | "visual" => {
                self.note(&format!("\"{lowered}\" is not translated"))
            }
            other => {
                // A call to a handler defined elsewhere in the script. `def`
                // makes those into functions, so this is an ordinary call.
                let written = arguments
                    .iter()
                    .map(|value| self.expr(value))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("call {other}({written})")
            }
        }
    }

    // ---------------------------------------------------------- expressions

    fn expr(&mut self, expression: &Expr) -> String {
        match expression {
            Expr::Number(number) => {
                if number.fract() == 0.0 && number.abs() < 1e15 {
                    format!("{}", *number as i64)
                } else {
                    format!("{number}")
                }
            }
            Expr::Text(text) => crate::html::quoted(text),
            Expr::Constant(name) => constant(name),
            Expr::Variable(name) => name.clone(),
            Expr::It => IT.to_string(),
            Expr::Unary { operator, operand } => {
                let operand = self.expr(operand);
                match operator {
                    UnaryOp::Negate => format!("(0 - {operand})"),
                    UnaryOp::Not => format!("not ({operand})"),
                }
            }
            Expr::Binary {
                operator,
                left,
                right,
            } => self.binary(*operator, left, right),
            Expr::Object(object) => match self.element(object) {
                Some(id) => format!("(the value of #{id})"),
                None => {
                    self.notes
                        .push("a part the page does not have was read".to_string());
                    "''".to_string()
                }
            },
            Expr::Count(target) => self.count(target),
            Expr::Chunk { chunks, source } => self.chunk(chunks, source),
            Expr::Call { name, arguments } => self.call(name, arguments),
            Expr::The(name) => self.the(name),
            Expr::Of { name, operand } => {
                let operand = self.expr(operand);
                match name.as_str() {
                    "length" => format!("({operand}).length"),
                    other => {
                        self.notes
                            .push(format!("\"the {other} of …\" is not translated"));
                        "''".to_string()
                    }
                }
            }
            Expr::Exists { .. } => {
                self.notes
                    .push("\"there is a …\" is not translated".to_string());
                "false".to_string()
            }
        }
    }

    fn binary(&mut self, operator: BinaryOp, left: &Expr, right: &Expr) -> String {
        let left = self.expr(left);
        let right = self.expr(right);
        let arithmetic = |sign: &str| format!("(({left} as Float) {sign} ({right} as Float))");
        match operator {
            BinaryOp::Add => arithmetic("+"),
            BinaryOp::Subtract => arithmetic("-"),
            BinaryOp::Multiply => arithmetic("*"),
            BinaryOp::Divide => arithmetic("/"),
            BinaryOp::Modulo => arithmetic("%"),
            BinaryOp::IntegerDivide => {
                format!("Math.floor(({left} as Float) / ({right} as Float))")
            }
            BinaryOp::Power => format!("(({left} as Float) ** ({right} as Float))"),
            // `&` joins text, and `+` in _hyperscript would add two numbers
            // that only look like text, so both sides are made into strings.
            BinaryOp::Concat => format!("(({left} as String) + ({right} as String))"),
            BinaryOp::ConcatSpace => {
                format!("(({left} as String) + ' ' + ({right} as String))")
            }
            BinaryOp::Equal => format!("({left} is {right})"),
            BinaryOp::NotEqual => format!("({left} is not {right})"),
            BinaryOp::Less => format!("({left} < {right})"),
            BinaryOp::Greater => format!("({left} > {right})"),
            BinaryOp::LessOrEqual => format!("({left} <= {right})"),
            BinaryOp::GreaterOrEqual => format!("({left} >= {right})"),
            BinaryOp::Contains => format!("({left} contains {right})"),
            BinaryOp::IsIn => format!("({right} contains {left})"),
            BinaryOp::StartsWith => format!("({left} starts with {right})"),
            BinaryOp::EndsWith => format!("({left} ends with {right})"),
            BinaryOp::And => format!("({left} and {right})"),
            BinaryOp::Or => format!("({left} or {right})"),
        }
    }

    fn count(&mut self, target: &CountTarget) -> String {
        match target {
            CountTarget::Chunks { kind, source } => {
                let source = self.expr(source);
                let kind = kind.as_str();
                format!("hlCount({source} as String, '{kind}')")
            }
            CountTarget::Cards => "hlCards.length".to_string(),
            other => {
                let _ = other;
                self.notes
                    .push("counting the stack's own objects is not translated".to_string());
                "0".to_string()
            }
        }
    }

    fn chunk(&mut self, chunks: &[Chunk], source: &Expr) -> String {
        // Chunks nest outermost first, so `word 2 of line 3` slices the line
        // and then the word — the reverse of the order they are written in.
        let mut out = format!("({})", self.expr(source));
        for chunk in chunks.iter().rev() {
            let start = self.expr(&chunk.start);
            let end = match &chunk.end {
                Some(end) => self.expr(end),
                None => start.clone(),
            };
            let kind = chunk.kind.as_str();
            out = format!("hlPart({out} as String, '{kind}', {start} as Int, {end} as Int)");
        }
        out
    }

    fn call(&mut self, name: &str, arguments: &[Expr]) -> String {
        let written: Vec<String> = arguments.iter().map(|value| self.expr(value)).collect();
        let first = written.first().cloned().unwrap_or_else(|| "''".to_string());
        match name.to_ascii_lowercase().as_str() {
            "length" => format!("({first} as String).length"),
            "number" | "value" => format!("({first} as Float)"),
            "abs" => format!("Math.abs({first} as Float)"),
            "round" => format!("Math.round({first} as Float)"),
            "trunc" => format!("Math.trunc({first} as Float)"),
            "sqrt" => format!("Math.sqrt({first} as Float)"),
            "random" => format!("(Math.floor(Math.random() * ({first} as Int)) + 1)"),
            "min" => format!("Math.min({})", written.join(", ")),
            "max" => format!("Math.max({})", written.join(", ")),
            "toupper" | "touppercase" => format!("({first} as String).toUpperCase()"),
            "tolower" | "tolowercase" => format!("({first} as String).toLowerCase()"),
            "ai" => {
                self.notes
                    .push("ai(…) needs a model, and a page has none".to_string());
                "''".to_string()
            }
            other => {
                // A function the script defines itself becomes a `def`, and
                // this is how it is called.
                format!("{other}({})", written.join(", "))
            }
        }
    }

    fn the(&mut self, name: &str) -> String {
        match name {
            "result" => "result".to_string(),
            "ticks" => "(Date.now() / 16.667)".to_string(),
            "seconds" => "(Date.now() / 1000)".to_string(),
            other => {
                self.notes
                    .push(format!("\"the {other}\" is not translated"));
                "''".to_string()
            }
        }
    }

    /// The element id for an object reference, if the page has one.
    fn element(&mut self, object: &ObjectRef) -> Option<String> {
        match object {
            ObjectRef::Me | ObjectRef::Target => None,
            ObjectRef::Part {
                kind,
                specifier,
                owner,
                ..
            } => {
                let Specifier::Value(Expr::Text(name)) = specifier.as_ref() else {
                    return None;
                };
                match owner.as_deref() {
                    None => self.elements.id(word(*kind), name),
                    Some(ObjectRef::Card(which)) => match which.as_ref() {
                        Specifier::Value(Expr::Text(card)) => {
                            self.elements.id_on_card(card, word(*kind), name)
                        }
                        Specifier::Current => self.elements.id(word(*kind), name),
                        _ => None,
                    },
                    Some(_) => None,
                }
            }
            _ => None,
        }
    }
}

/// What the grammar's idea of a part is called.
const fn word(kind: PartKind) -> &'static str {
    match kind {
        PartKind::Button => "button",
        PartKind::Field => "field",
        PartKind::Image => "image",
    }
}

/// The DOM event a HyperTalk message becomes.
///
/// The mouse messages have real equivalents. The rest are HyperLab's own and
/// travel as custom events the page sends, which is what `hyperlab:` marks.
fn event(message: &str) -> String {
    match message.to_ascii_lowercase().as_str() {
        "mouseup" => "click".to_string(),
        "mousedown" => "mousedown".to_string(),
        "mouseenter" => "mouseenter".to_string(),
        "mouseleave" => "mouseleave".to_string(),
        "openstack" => "load from window".to_string(),
        other => format!("hyperlab:{other}"),
    }
}

/// A HyperTalk named constant.
fn constant(name: &str) -> String {
    match name.to_ascii_lowercase().as_str() {
        "true" => "true".to_string(),
        "false" => "false".to_string(),
        "empty" => "''".to_string(),
        "space" => "' '".to_string(),
        "tab" => "'\\t'".to_string(),
        "return" | "newline" => "'\\n'".to_string(),
        "comma" => "','".to_string(),
        "quote" => "'\"'".to_string(),
        "zero" => "0".to_string(),
        other => crate::html::quoted(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(source: &str) -> String {
        script(source).expect("it parses").source
    }

    #[test]
    fn a_mouse_handler_becomes_a_click_handler() {
        assert_eq!(out("on mouseUp\n  beep\nend mouseUp").lines().count(), 1);
        assert!(out("on mouseUp\nend mouseUp").starts_with("on click"));
    }

    #[test]
    fn it_is_renamed_because_hyperscript_will_not_be_assigned_to() {
        // Verified in a browser: `set it to x` parses, runs, and leaves null.
        let written = out("on mouseUp\n  get 2\n  put it into total\nend mouseUp");
        assert!(written.contains(&format!("set {IT} to 2")), "{written}");
        assert!(written.contains(&format!("set total to {IT}")), "{written}");
        assert!(!written.contains("set it to"));
    }

    #[test]
    fn counting_loops_let_hyperscript_keep_the_count() {
        // Not a hand-rolled counter: `next repeat` becomes `continue`, which
        // would skip an increment and never finish.
        let written =
            out("on mouseUp\n  repeat with i = 1 to 3\n    next repeat\n  end repeat\nend mouseUp");
        assert!(written.contains("times index"), "{written}");
        assert!(written.contains("continue"), "{written}");
        assert!(!written.contains("set i to i +"), "{written}");
    }

    #[test]
    fn nested_counting_loops_do_not_share_a_counter() {
        let written = out(
            "on mouseUp\n  repeat with i = 1 to 2\n    repeat with j = 1 to 2\n    end repeat\n  end repeat\nend mouseUp",
        );
        assert!(written.contains("hlStep0"), "{written}");
        assert!(written.contains("hlStep1"), "{written}");
    }

    #[test]
    fn the_words_the_two_languages_share_are_left_alone() {
        let written = out(
            "on mouseUp\n  if x starts with \"a\" and y contains \"b\" then\n    put 1 into z\n  end if\nend mouseUp",
        );
        assert!(written.contains("starts with"), "{written}");
        assert!(written.contains("contains"), "{written}");
        assert!(written.contains("if "), "{written}");
        assert!(written.ends_with("end"), "{written}");
    }

    #[test]
    fn concatenation_makes_both_sides_text_first() {
        // `+` would add "1" and "2" into 3 rather than joining them.
        let written = out("on mouseUp\n  put 1 & 2 into x\nend mouseUp");
        assert!(written.contains("as String"), "{written}");
    }

    #[test]
    fn a_function_becomes_a_def() {
        let written = out("function twice n\n  return n * 2\nend twice");
        assert!(written.starts_with("def twice(n)"), "{written}");
        assert!(written.contains("return"), "{written}");
    }

    #[test]
    fn what_does_not_translate_is_a_comment_and_a_note() {
        let translated = script("on mouseUp\n  beep\nend mouseUp").expect("it parses");
        assert_eq!(translated.notes.len(), 1);
        assert!(translated.source.contains("-- there is no beep"));
        assert!(!translated.is_complete());
    }

    #[test]
    fn asking_turns_a_cancelled_prompt_into_empty() {
        // HyperTalk promises empty; the browser gives null.
        let written = out("on mouseUp\n  ask \"name?\" with \"x\"\nend mouseUp");
        assert!(written.contains("or ''"), "{written}");
    }

    #[test]
    fn a_script_that_does_not_parse_is_reported_rather_than_half_translated() {
        assert!(script("on mouseUp\n  if\nend mouseUp").is_err());
    }
}
