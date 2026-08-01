//! The HyperTalk abstract syntax tree.
//!
//! The tree is a plain data structure: it has no methods that touch a stack,
//! no interpreter state and no interior mutability. Parsing produces it and
//! nothing ever mutates it, which is what lets the runtime cache compiled
//! scripts and lets tools analyse them safely.
//!
//! Two deliberate omissions keep the tree small:
//!
//! * There is no distinction between "property access" and "function call".
//!   `the length of x` and `the visible of button 1` both parse to
//!   [`Expr::Of`]; only the runtime knows which names are properties. New
//!   properties therefore never require parser changes.
//! * There is no fixed list of commands. Anything the parser does not
//!   recognise as syntax becomes a [`StatementKind::Command`], which the
//!   runtime dispatches exactly like a message. Adding `beep`, `wait` or a
//!   future `ask assistant` is a runtime change, not a grammar change.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Applied to every AST node so the tree can be serialized when the `serde`
/// feature is on, without repeating the attribute forty times.
macro_rules! ast_node {
    ($(#[$meta:meta])* $vis:vis enum $name:ident { $($body:tt)* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq)]
        #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
        $vis enum $name { $($body)* }
    };
    ($(#[$meta:meta])* $vis:vis struct $name:ident { $($body:tt)* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq)]
        #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
        $vis struct $name { $($body)* }
    };
}

ast_node! {
    /// A whole script: the handlers found in one object's source.
    pub struct Script {
        /// The handlers, in source order.
        pub handlers: Vec<Handler>,
    }
}

impl Script {
    /// An empty script, which handles nothing.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// The handler for `name`, compared case-insensitively.
    #[must_use]
    pub fn handler(&self, kind: HandlerKind, name: &str) -> Option<&Handler> {
        self.handlers
            .iter()
            .find(|handler| handler.kind == kind && handler.name.eq_ignore_ascii_case(name))
    }
}

ast_node! {
    /// Whether a handler responds to a message or computes a value.
    #[derive(Copy, Eq)]
    pub enum HandlerKind {
        /// `on mouseUp … end mouseUp`
        Message,
        /// `function total … end total`
        Function,
    }
}

ast_node! {
    /// One handler.
    pub struct Handler {
        /// Message handler or function.
        pub kind: HandlerKind,
        /// The message or function name, as written.
        pub name: String,
        /// Parameter names, as written.
        pub parameters: Vec<String>,
        /// The statements between the header and `end`.
        pub body: Block,
        /// Line of the `on`/`function` keyword, counting from one.
        pub line: u32,
    }
}

/// A sequence of statements.
pub type Block = Vec<Statement>;

ast_node! {
    /// A statement, together with where it came from.
    pub struct Statement {
        /// What the statement does.
        pub kind: StatementKind,
        /// Source line, counting from one. Errors and future debuggers use it.
        pub line: u32,
    }
}

impl Statement {
    /// Wraps a kind with its source line.
    #[must_use]
    pub const fn new(kind: StatementKind, line: u32) -> Self {
        Self { kind, line }
    }
}

ast_node! {
    /// Everything a statement can be.
    pub enum StatementKind {
        /// `put <value> [into|before|after <container>]`. With no container the
        /// value goes to the message box.
        Put {
            /// The value to store.
            value: Expr,
            /// Where it goes; `None` means the message box.
            target: Option<Container>,
            /// Whether it replaces, prepends or appends.
            preposition: Preposition,
        },
        /// `set [the] <property> [of <object>] to <value>`.
        Set {
            /// The property name, lower-cased by the parser.
            property: String,
            /// The object it belongs to; `None` means the current card's stack
            /// or a global setting.
            object: Option<ObjectRef>,
            /// The new value.
            value: Expr,
        },
        /// `get <value>`, which is shorthand for `put <value> into it`.
        Get(Expr),
        /// `add`, `subtract`, `multiply` and `divide`.
        Arithmetic {
            /// Which of the four.
            operator: ArithmeticCommand,
            /// The operand.
            value: Expr,
            /// The container that is updated in place.
            target: Container,
        },
        /// `if … then … else … end if`, including `else if` chains.
        If {
            /// Each `(condition, body)` pair, in order.
            branches: Vec<Branch>,
            /// The final `else` body, if any.
            otherwise: Option<Block>,
        },
        /// `repeat … end repeat`.
        Repeat {
            /// What controls the loop.
            control: RepeatControl,
            /// The loop body.
            body: Block,
        },
        /// `exit repeat`, `exit <handler>` or `exit to hyperlab`.
        Exit(ExitTarget),
        /// `next repeat`.
        NextRepeat,
        /// `pass <message>`: hand the current message to the next object in
        /// the message path.
        Pass(String),
        /// `return [<value>]`.
        Return(Option<Expr>),
        /// `global a, b`: declare names that refer to global variables.
        Global(Vec<String>),
        /// `go [to] <destination>`.
        Go(Destination),
        /// `send <message> to <object>`.
        Send {
            /// The message text, evaluated at run time.
            message: Expr,
            /// Who receives it.
            target: ObjectRef,
        },
        /// Anything else: `answer "hi"`, `beep`, or a call to a handler
        /// defined elsewhere in the message path.
        Command {
            /// The command name, as written.
            name: String,
            /// Positional arguments.
            arguments: Vec<Expr>,
        },
    }
}

ast_node! {
    /// One `if`/`else if` arm.
    pub struct Branch {
        /// The condition.
        pub condition: Expr,
        /// What runs when it is true.
        pub body: Block,
    }
}

ast_node! {
    /// How `put` combines the value with what is already there.
    #[derive(Copy, Eq)]
    pub enum Preposition {
        /// Replace the contents.
        Into,
        /// Insert in front of the contents.
        Before,
        /// Append to the contents.
        After,
    }
}

ast_node! {
    /// The four in-place arithmetic commands.
    #[derive(Copy, Eq)]
    pub enum ArithmeticCommand {
        /// `add <value> to <container>`
        Add,
        /// `subtract <value> from <container>`
        Subtract,
        /// `multiply <container> by <value>`
        Multiply,
        /// `divide <container> by <value>`
        Divide,
    }
}

ast_node! {
    /// What a `repeat` loop counts on.
    pub enum RepeatControl {
        /// `repeat` / `repeat forever`
        Forever,
        /// `repeat <count> [times]`
        Times(Expr),
        /// `repeat while <condition>`
        While(Expr),
        /// `repeat until <condition>`
        Until(Expr),
        /// `repeat with <variable> = <from> to <to>`
        With {
            /// The loop variable.
            variable: String,
            /// First value.
            from: Expr,
            /// Last value, inclusive.
            to: Expr,
            /// Whether the counter decreases (`down to`).
            down: bool,
        },
    }
}

ast_node! {
    /// What `exit` leaves.
    pub enum ExitTarget {
        /// `exit repeat`
        Repeat,
        /// `exit <handler name>`
        Handler(String),
        /// `exit to hyperlab`: abandon the whole execution.
        Everything,
    }
}

ast_node! {
    /// Where `go` goes.
    pub enum Destination {
        /// A card, named or numbered or ordinal.
        Card(Specifier),
        /// `go back`: the previously visited card.
        Back,
    }
}

// ---------------------------------------------------------------- containers

ast_node! {
    /// Something a value can be put into.
    pub struct Container {
        /// What is being written to.
        pub base: ContainerBase,
        /// Optional chunks, outermost first: `word 2 of line 3 of x` is
        /// `[word 2, line 3]`.
        pub chunks: Vec<Chunk>,
    }
}

ast_node! {
    /// The addressable part of a [`Container`].
    pub enum ContainerBase {
        /// A local or global variable.
        Variable(String),
        /// `it`, the implicit variable written by `get` and `ask`.
        It,
        /// A field, or any other object with contents.
        Object(ObjectRef),
        /// The message box.
        MessageBox,
    }
}

ast_node! {
    /// A piece of text selected out of a larger piece.
    pub struct Chunk {
        /// Which unit is counted.
        pub kind: ChunkKind,
        /// The first unit, counting from one.
        pub start: Box<Expr>,
        /// The last unit for a range like `char 2 to 5`.
        pub end: Option<Box<Expr>>,
    }
}

ast_node! {
    /// The units HyperTalk can slice text into.
    #[derive(Copy, Eq, Hash)]
    pub enum ChunkKind {
        /// A single character.
        Char,
        /// Text between spaces.
        Word,
        /// Text between commas.
        Item,
        /// Text between newlines.
        Line,
    }
}

impl ChunkKind {
    /// The keyword that introduces this chunk.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Char => "char",
            Self::Word => "word",
            Self::Item => "item",
            Self::Line => "line",
        }
    }
}

// ----------------------------------------------------------------- objects

ast_node! {
    /// A reference to an object, resolved at run time.
    pub enum ObjectRef {
        /// `me`: the object whose script is running.
        Me,
        /// `the target`: the object the current message was sent to.
        Target,
        /// `this stack`.
        Stack,
        /// A card.
        Card(Box<Specifier>),
        /// A background.
        Background(Box<Specifier>),
        /// A button or a field.
        Part {
            /// Button or field.
            kind: PartKind,
            /// Whether the script said `card`, `background`, or neither.
            layer: Layer,
            /// Which one.
            specifier: Box<Specifier>,
            /// `of card 3`, if written.
            owner: Option<Box<ObjectRef>>,
        },
    }
}

ast_node! {
    /// Buttons and fields, as the grammar sees them.
    ///
    /// This mirrors the object model's part kinds but does not depend on it:
    /// the parser knows the *syntax* `button`/`field`, not the object model.
    #[derive(Copy, Eq)]
    pub enum PartKind {
        /// `button`, `btn`
        Button,
        /// `field`, `fld`
        Field,
    }
}

ast_node! {
    /// Which layer a part reference means.
    #[derive(Copy, Eq)]
    pub enum Layer {
        /// The script said `card button …`.
        Card,
        /// The script said `background button …`.
        Background,
        /// The script just said `button …`; the runtime searches the card
        /// first, then the background.
        Unspecified,
    }
}

ast_node! {
    /// How one object out of many is picked.
    pub enum Specifier {
        /// `this card`, or a bare `card` with nothing after it.
        Current,
        /// `card id 12`.
        Id(Expr),
        /// `card 3` or `card "Home"`: a number selects by position, anything
        /// else selects by name. The distinction is made at run time because
        /// the expression may be a variable.
        Value(Expr),
        /// `first`, `last`, `next`, …
        Ordinal(Ordinal),
    }
}

ast_node! {
    /// Positional words.
    #[derive(Copy, Eq)]
    pub enum Ordinal {
        /// The first one.
        First,
        /// The second one.
        Second,
        /// The third one.
        Third,
        /// The fourth one.
        Fourth,
        /// The fifth one.
        Fifth,
        /// The last one.
        Last,
        /// The one in the middle.
        Middle,
        /// One chosen at random.
        Any,
        /// The one after the current one, wrapping around.
        Next,
        /// The one before the current one, wrapping around.
        Previous,
    }
}

impl Ordinal {
    /// The zero-based index this ordinal picks out of `count` items, if it is
    /// a fixed position. `Any`, `Next` and `Previous` need run-time context
    /// and return `None`.
    #[must_use]
    pub const fn index(self, count: usize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        match self {
            Self::First => Some(0),
            Self::Second => Some(1),
            Self::Third => Some(2),
            Self::Fourth => Some(3),
            Self::Fifth => Some(4),
            Self::Last => Some(count - 1),
            Self::Middle => Some(count / 2),
            Self::Any | Self::Next | Self::Previous => None,
        }
    }
}

// -------------------------------------------------------------- expressions

ast_node! {
    /// An expression.
    pub enum Expr {
        /// A number written in the source.
        Number(f64),
        /// A quoted string.
        Text(String),
        /// A named constant: `true`, `empty`, `quote`, `return`, …
        Constant(String),
        /// A variable, or a bare word used as an unquoted string.
        Variable(String),
        /// `it`.
        It,
        /// A prefix operator.
        Unary {
            /// Which operator.
            operator: UnaryOp,
            /// What it applies to.
            operand: Box<Expr>,
        },
        /// An infix operator.
        Binary {
            /// Which operator.
            operator: BinaryOp,
            /// Left-hand side.
            left: Box<Expr>,
            /// Right-hand side.
            right: Box<Expr>,
        },
        /// `name(arguments)`.
        Call {
            /// The function name, as written.
            name: String,
            /// The arguments.
            arguments: Vec<Expr>,
        },
        /// `the <name>`: a function of no arguments, or a global property.
        The(String),
        /// `the <name> of <operand>`: a property when the operand is an
        /// object, a one-argument function otherwise.
        Of {
            /// The property or function name, lower-cased.
            name: String,
            /// What it is applied to.
            operand: Box<Expr>,
        },
        /// An object used as a value: a field yields its text, a button its
        /// name.
        Object(ObjectRef),
        /// `the number of <something>`.
        Count(Box<CountTarget>),
        /// `word 2 of x`, `char 1 to 3 of x`.
        Chunk {
            /// Chunks, outermost first.
            chunks: Vec<Chunk>,
            /// The text they slice.
            source: Box<Expr>,
        },
        /// `there is a <object>` / `there is no <object>`.
        Exists {
            /// The object to look for.
            object: Box<ObjectRef>,
            /// Whether the script asked for absence instead.
            negated: bool,
        },
    }
}

ast_node! {
    /// What `the number of …` counts.
    pub enum CountTarget {
        /// `the number of cards`.
        Cards,
        /// `the number of backgrounds`.
        Backgrounds,
        /// `the number of buttons [of <object>]`.
        Parts {
            /// Buttons or fields.
            kind: PartKind,
            /// Which layer to count.
            layer: Layer,
            /// Whose parts to count; `None` means the current card.
            owner: Option<ObjectRef>,
        },
        /// `the number of words of "a b c"`.
        Chunks {
            /// The unit to count.
            kind: ChunkKind,
            /// The text to count in.
            source: Expr,
        },
    }
}

ast_node! {
    /// Prefix operators.
    #[derive(Copy, Eq)]
    pub enum UnaryOp {
        /// Arithmetic negation.
        Negate,
        /// Logical negation.
        Not,
    }
}

ast_node! {
    /// Infix operators.
    #[derive(Copy, Eq)]
    pub enum BinaryOp {
        /// `+`
        Add,
        /// `-`
        Subtract,
        /// `*`
        Multiply,
        /// `/`
        Divide,
        /// `div`: integer division.
        IntegerDivide,
        /// `mod`
        Modulo,
        /// `^`
        Power,
        /// `&`: text concatenation.
        Concat,
        /// `&&`: concatenation with a space between.
        ConcatSpace,
        /// `=` / `is`
        Equal,
        /// `<>` / `is not` / `≠`
        NotEqual,
        /// `<`
        Less,
        /// `>`
        Greater,
        /// `<=` / `≤`
        LessOrEqual,
        /// `>=` / `≥`
        GreaterOrEqual,
        /// `contains`
        Contains,
        /// `is in`: the reverse of `contains`.
        IsIn,
        /// `starts with`
        StartsWith,
        /// `ends with`
        EndsWith,
        /// `and`
        And,
        /// `or`
        Or,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinals_map_to_positions() {
        assert_eq!(Ordinal::First.index(3), Some(0));
        assert_eq!(Ordinal::Last.index(3), Some(2));
        assert_eq!(Ordinal::Middle.index(3), Some(1));
        assert_eq!(Ordinal::First.index(0), None);
        assert_eq!(Ordinal::Any.index(3), None);
    }

    #[test]
    fn handlers_are_found_case_insensitively() {
        let script = Script {
            handlers: vec![Handler {
                kind: HandlerKind::Message,
                name: "mouseUp".into(),
                parameters: vec![],
                body: vec![],
                line: 1,
            }],
        };
        assert!(script.handler(HandlerKind::Message, "MOUSEUP").is_some());
        assert!(script.handler(HandlerKind::Function, "mouseUp").is_none());
    }
}
