//! Turning [`Token`]s into an [`ast::Script`].
//!
//! The parser is a hand-written recursive-descent parser. It is deliberately
//! ordinary: HyperTalk's grammar is small, and a parser generator would make
//! the error messages worse and the code harder to follow fifty years from
//! now.
//!
//! Two rules keep it honest:
//!
//! * **Nothing is evaluated here.** The parser never looks at a stack, never
//!   resolves a name and never decides whether `the length of x` is a
//!   property or a function.
//! * **Statements are lines.** A statement ends at the end of its line unless
//!   the line ends with `\`, which the lexer has already dealt with.

use crate::{
    ast::{
        ArithmeticCommand, BinaryOp, Block, Branch, Chunk, ChunkKind, Container, ContainerBase,
        CountTarget, Destination, ExitTarget, Expr, Handler, HandlerKind, Layer, ObjectRef,
        Ordinal, PartKind, Preposition, RepeatControl, Script, Specifier, Statement, StatementKind,
        UnaryOp,
    },
    error::{ParseError, ParseResult},
    lexer::tokenize,
    token::{Token, TokenKind},
};

/// Words that introduce a chunk expression and so cannot be variable names.
const CHUNK_WORDS: &[&str] = &[
    "char",
    "chars",
    "character",
    "characters",
    "word",
    "words",
    "item",
    "items",
    "line",
    "lines",
];

/// Words that name an object, and so start an object reference.
const OBJECT_WORDS: &[&str] = &[
    "me",
    "this",
    "card",
    "cd",
    "background",
    "bkgnd",
    "bg",
    "stack",
    "button",
    "btn",
    "field",
    "fld",
];

/// Named constants. The runtime gives them values; the parser only needs to
/// know they are not variables.
const CONSTANTS: &[&str] = &[
    "empty", "true", "false", "quote", "return", "space", "tab", "comma", "colon", "pi",
    "linefeed", "newline",
];

/// Units a command may end with, as in `wait 2 seconds`. They are passed to
/// the command as an ordinary trailing argument; the runtime decides what
/// they mean.
const UNIT_WORDS: &[&str] = &[
    "tick",
    "ticks",
    "second",
    "seconds",
    "sec",
    "secs",
    "millisecond",
    "milliseconds",
];

/// Words that join parts of a statement together and so never begin an
/// expression of their own.
const CLAUSE_WORDS: &[&str] = &[
    "of", "to", "into", "before", "after", "from", "by", "with", "then", "and", "or", "is", "in",
    "contains", "starts", "ends", "div", "mod", "else", "end", "down", "times", "while", "until",
];

/// Commands whose name is two words.
///
/// Only the *shape* is grammar. `ask assistant "…"` would otherwise read as
/// `ask` applied to a variable called `assistant`, and then fall over on the
/// string after it; joining the two words here hands the runtime an ordinary
/// command whose name happens to contain a space, and leaves the runtime to
/// decide what it means.
const TWO_WORD_COMMANDS: &[(&str, &str)] = &[("ask", "assistant")];

/// Adjectives that may precede a function name, as in `the long date`.
const QUALIFIERS: &[&str] = &[
    "long",
    "short",
    "abbrev",
    "abbreviated",
    "english",
    "numeric",
];

/// Parses a whole script.
///
/// A script is a sequence of handlers; anything outside a handler is an
/// error, exactly as in HyperCard.
///
/// # Errors
///
/// Returns a [`ParseError`] describing the first problem found.
pub fn parse(source: &str) -> ParseResult<Script> {
    Parser::new(tokenize(source)?).parse_script()
}

/// Parses a single expression, for `do`, `the value of` and for tests.
///
/// # Errors
///
/// Returns a [`ParseError`] if the text is not one complete expression.
pub fn parse_expression(source: &str) -> ParseResult<Expr> {
    let mut parser = Parser::new(tokenize(source)?);
    let expression = parser.parse_expression()?;
    if parser.peek().is_end_of_line() {
        Ok(expression)
    } else {
        Err(parser.error_here("I did not expect anything after this expression"))
    }
}

/// The recursive-descent parser.
struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    // ------------------------------------------------------------- plumbing

    fn peek(&self) -> &Token {
        self.peek_at(0)
    }

    fn peek_at(&self, offset: usize) -> &Token {
        let index = (self.position + offset).min(self.tokens.len() - 1);
        &self.tokens[index]
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.position.min(self.tokens.len() - 1)].clone();
        if self.position < self.tokens.len() - 1 {
            self.position += 1;
        }
        token
    }

    fn line(&self) -> u32 {
        self.peek().line
    }

    fn error_here(&self, message: impl Into<String>) -> ParseError {
        let token = self.peek();
        ParseError::new(message, token.line, token.column)
    }

    fn unexpected(&self, expected: &str) -> ParseError {
        let token = self.peek();
        self.error_here(format!("I expected {expected} but found {}", token.kind))
    }

    /// Consumes the next token if it is `keyword`.
    fn accept(&mut self, keyword: &str) -> bool {
        if self.peek().is(keyword) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Consumes the next token if it is any of `keywords`, returning which.
    fn accept_any(&mut self, keywords: &[&str]) -> Option<String> {
        if self.peek().is_any(keywords) {
            Some(self.advance().keyword().to_string())
        } else {
            None
        }
    }

    /// Consumes the next token if it is one of `keywords`, ignoring which.
    ///
    /// Used for noise words: `go to next card` and `go next` mean the same.
    fn skip_any(&mut self, keywords: &[&str]) {
        let _ = self.accept_any(keywords);
    }

    fn accept_symbol(&mut self, symbol: &str) -> bool {
        if self.peek().is_symbol(symbol) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, keyword: &str) -> ParseResult<()> {
        if self.accept(keyword) {
            Ok(())
        } else {
            Err(self.unexpected(&format!("\"{keyword}\"")))
        }
    }

    fn expect_symbol(&mut self, symbol: &str) -> ParseResult<()> {
        if self.accept_symbol(symbol) {
            Ok(())
        } else {
            Err(self.unexpected(&format!("\"{symbol}\"")))
        }
    }

    /// Consumes a word and returns it as written.
    fn expect_word(&mut self, what: &str) -> ParseResult<String> {
        match &self.peek().kind {
            TokenKind::Word { text, .. } => {
                let text = text.clone();
                self.advance();
                Ok(text)
            }
            _ => Err(self.unexpected(what)),
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek().kind, TokenKind::Newline) {
            self.advance();
        }
    }

    fn at_end(&self) -> bool {
        matches!(self.peek().kind, TokenKind::EndOfScript)
    }

    /// Requires that the current statement has ended.
    fn expect_end_of_line(&mut self) -> ParseResult<()> {
        match self.peek().kind {
            TokenKind::Newline => {
                self.advance();
                Ok(())
            }
            TokenKind::EndOfScript => Ok(()),
            _ => Err(self.unexpected("the end of the line")),
        }
    }

    // -------------------------------------------------------------- scripts

    fn parse_script(&mut self) -> ParseResult<Script> {
        let mut handlers = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_end() {
                return Ok(Script { handlers });
            }
            let kind = if self.accept("on") {
                HandlerKind::Message
            } else if self.accept("function") {
                HandlerKind::Function
            } else {
                return Err(self.error_here(
                    "a script contains only handlers; every statement must be inside \
                     \"on …\" or \"function …\"",
                ));
            };
            handlers.push(self.parse_handler(kind)?);
        }
    }

    fn parse_handler(&mut self, kind: HandlerKind) -> ParseResult<Handler> {
        let line = self.line();
        let name = self.expect_word("a handler name")?;
        let mut parameters = Vec::new();
        while !self.peek().is_end_of_line() {
            parameters.push(self.expect_word("a parameter name")?);
            if !self.accept_symbol(",") {
                break;
            }
        }
        self.expect_end_of_line()?;

        let body = self.parse_block(&["end"])?;
        self.expect("end")?;
        let closing = self.expect_word("the handler name after \"end\"")?;
        if !closing.eq_ignore_ascii_case(&name) {
            return Err(self.error_here(format!(
                "this handler starts with \"{name}\" but ends with \"{closing}\""
            )));
        }
        self.expect_end_of_line()?;

        Ok(Handler {
            kind,
            name,
            parameters,
            body,
            line,
        })
    }

    /// Parses statements until a line starts with one of `terminators`, which
    /// is left for the caller to consume.
    fn parse_block(&mut self, terminators: &[&str]) -> ParseResult<Block> {
        let mut statements = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_end() || self.peek().is_any(terminators) {
                return Ok(statements);
            }
            let statement = self.parse_statement()?;
            statements.push(statement);
            self.expect_end_of_line()?;
        }
    }

    // ----------------------------------------------------------- statements

    /// Parses one statement, leaving the line terminator in place.
    fn parse_statement(&mut self) -> ParseResult<Statement> {
        let line = self.line();
        let kind = match self.peek().keyword() {
            "put" => self.parse_put()?,
            "set" => self.parse_set()?,
            "get" => {
                self.advance();
                StatementKind::Get(self.parse_expression()?)
            }
            "add" | "subtract" | "multiply" | "divide" => self.parse_arithmetic()?,
            "if" => self.parse_if()?,
            "repeat" => self.parse_repeat()?,
            "exit" => self.parse_exit()?,
            "next" if self.peek_at(1).is("repeat") => {
                self.advance();
                self.advance();
                StatementKind::NextRepeat
            }
            "pass" => {
                self.advance();
                StatementKind::Pass(self.expect_word("a message name")?)
            }
            "return" => {
                self.advance();
                if self.peek().is_end_of_line() {
                    StatementKind::Return(None)
                } else {
                    StatementKind::Return(Some(self.parse_expression()?))
                }
            }
            "global" => self.parse_global()?,
            "go" => self.parse_go()?,
            "send" => self.parse_send()?,
            "end" | "else" => {
                return Err(self.error_here(format!(
                    "\"{}\" does not belong here",
                    self.peek().keyword()
                )));
            }
            _ => self.parse_command()?,
        };
        Ok(Statement::new(kind, line))
    }

    fn parse_put(&mut self) -> ParseResult<StatementKind> {
        self.expect("put")?;
        let value = self.parse_expression()?;
        let preposition = if self.accept("into") {
            Preposition::Into
        } else if self.accept("before") {
            Preposition::Before
        } else if self.accept("after") {
            Preposition::After
        } else {
            // `put x` on its own writes to the message box.
            return Ok(StatementKind::Put {
                value,
                target: None,
                preposition: Preposition::Into,
            });
        };
        let target = self.parse_container()?;
        Ok(StatementKind::Put {
            value,
            target: Some(target),
            preposition,
        })
    }

    fn parse_set(&mut self) -> ParseResult<StatementKind> {
        self.expect("set")?;
        self.accept("the");
        let property = self.expect_word("a property name")?.to_ascii_lowercase();
        let object = if self.accept("of") {
            Some(self.parse_object_ref()?)
        } else {
            None
        };
        self.expect("to")?;
        let value = self.parse_expression()?;
        Ok(StatementKind::Set {
            property,
            object,
            value,
        })
    }

    fn parse_arithmetic(&mut self) -> ParseResult<StatementKind> {
        let word = self.advance();
        let (operator, value, target) = match word.keyword() {
            "add" => {
                let value = self.parse_expression()?;
                self.expect("to")?;
                (ArithmeticCommand::Add, value, self.parse_container()?)
            }
            "subtract" => {
                let value = self.parse_expression()?;
                self.expect("from")?;
                (ArithmeticCommand::Subtract, value, self.parse_container()?)
            }
            "multiply" => {
                let target = self.parse_container()?;
                self.expect("by")?;
                (
                    ArithmeticCommand::Multiply,
                    self.parse_expression()?,
                    target,
                )
            }
            _ => {
                let target = self.parse_container()?;
                self.expect("by")?;
                (ArithmeticCommand::Divide, self.parse_expression()?, target)
            }
        };
        Ok(StatementKind::Arithmetic {
            operator,
            value,
            target,
        })
    }

    fn parse_if(&mut self) -> ParseResult<StatementKind> {
        self.expect("if")?;
        let condition = self.parse_expression()?;
        // `then` may sit on the next line, which classic scripts often do.
        if matches!(self.peek().kind, TokenKind::Newline) && self.peek_at(1).is("then") {
            self.advance();
        }
        self.expect("then")?;

        let mut branches = Vec::new();
        let mut otherwise = None;

        if self.peek().is_end_of_line() {
            // Multi-line form: a block, optional `else if` chain, `end if`.
            let body = self.parse_block(&["end", "else"])?;
            branches.push(Branch { condition, body });
            loop {
                if self.accept("else") {
                    if self.accept("if") {
                        let condition = self.parse_expression()?;
                        if matches!(self.peek().kind, TokenKind::Newline)
                            && self.peek_at(1).is("then")
                        {
                            self.advance();
                        }
                        self.expect("then")?;
                        let body = self.parse_block(&["end", "else"])?;
                        branches.push(Branch { condition, body });
                    } else if self.peek().is_end_of_line() {
                        otherwise = Some(self.parse_block(&["end"])?);
                        break;
                    } else {
                        // `else <statement>` on one line.
                        let statement = self.parse_statement()?;
                        otherwise = Some(vec![statement]);
                        break;
                    }
                } else {
                    break;
                }
            }
            self.expect("end")?;
            self.expect("if")?;
        } else {
            // Single-line form: `if x then beep`.
            let statement = self.parse_statement()?;
            branches.push(Branch {
                condition,
                body: vec![statement],
            });
            // An `else` may follow on the next line, as in classic HyperTalk.
            if matches!(self.peek().kind, TokenKind::Newline) && self.peek_at(1).is("else") {
                self.advance();
            }
            if self.accept("else") {
                if self.peek().is_end_of_line() {
                    otherwise = Some(self.parse_block(&["end"])?);
                    self.expect("end")?;
                    self.expect("if")?;
                } else {
                    otherwise = Some(vec![self.parse_statement()?]);
                }
            }
        }

        Ok(StatementKind::If {
            branches,
            otherwise,
        })
    }

    fn parse_repeat(&mut self) -> ParseResult<StatementKind> {
        self.expect("repeat")?;
        let control = if self.peek().is_end_of_line() || self.accept("forever") {
            RepeatControl::Forever
        } else if self.accept("while") {
            RepeatControl::While(self.parse_expression()?)
        } else if self.accept("until") {
            RepeatControl::Until(self.parse_expression()?)
        } else if self.accept("with") {
            let variable = self.expect_word("a loop variable")?;
            self.expect_symbol("=")?;
            let from = self.parse_expression()?;
            let down = self.accept("down");
            self.expect("to")?;
            let to = self.parse_expression()?;
            RepeatControl::With {
                variable,
                from,
                to,
                down,
            }
        } else {
            self.accept("for");
            let count = self.parse_expression()?;
            self.accept("times");
            RepeatControl::Times(count)
        };
        self.expect_end_of_line()?;

        let body = self.parse_block(&["end"])?;
        self.expect("end")?;
        self.expect("repeat")?;
        Ok(StatementKind::Repeat { control, body })
    }

    fn parse_exit(&mut self) -> ParseResult<StatementKind> {
        self.expect("exit")?;
        if self.accept("repeat") {
            Ok(StatementKind::Exit(ExitTarget::Repeat))
        } else if self.accept("to") {
            let target = self.expect_word("\"HyperLab\"")?;
            if target.eq_ignore_ascii_case("hyperlab") || target.eq_ignore_ascii_case("hypercard") {
                Ok(StatementKind::Exit(ExitTarget::Everything))
            } else {
                Err(self.error_here(format!("I do not know how to exit to \"{target}\"")))
            }
        } else {
            let name = self.expect_word("a handler name")?;
            Ok(StatementKind::Exit(ExitTarget::Handler(name)))
        }
    }

    fn parse_global(&mut self) -> ParseResult<StatementKind> {
        self.expect("global")?;
        let mut names = vec![self.expect_word("a global variable name")?];
        while self.accept_symbol(",") {
            names.push(self.expect_word("a global variable name")?);
        }
        Ok(StatementKind::Global(names))
    }

    fn parse_go(&mut self) -> ParseResult<StatementKind> {
        self.expect("go")?;
        self.accept("to");
        if self.accept("back") {
            return Ok(StatementKind::Go(Destination::Back));
        }
        if let Some(ordinal) = self.accept_ordinal() {
            // `go next` and `go to next card` mean the same thing.
            self.skip_any(&["card", "cd"]);
            return Ok(StatementKind::Go(Destination::Card(Specifier::Ordinal(
                ordinal,
            ))));
        }
        self.accept("this");
        self.skip_any(&["card", "cd"]);
        let specifier = self.parse_specifier()?;
        Ok(StatementKind::Go(Destination::Card(specifier)))
    }

    fn parse_send(&mut self) -> ParseResult<StatementKind> {
        self.expect("send")?;
        let message = self.parse_expression()?;
        self.expect("to")?;
        let target = self.parse_object_ref()?;
        Ok(StatementKind::Send { message, target })
    }

    /// Anything that is not built-in syntax: `answer "hi"`, `beep`, or a call
    /// to a handler defined somewhere in the message path.
    fn parse_command(&mut self) -> ParseResult<StatementKind> {
        let mut name = self.expect_word("a command")?;
        let mut arguments = Vec::new();

        if TWO_WORD_COMMANDS
            .iter()
            .any(|(first, second)| name.eq_ignore_ascii_case(first) && self.peek().is(second))
        {
            let second = self.expect_word("a command")?;
            name = format!("{name} {second}");
        }

        // `doThing(1, 2)` is accepted alongside `doThing 1, 2`. If the
        // parenthesised reading does not consume the whole line it was really
        // a parenthesised first argument, so we put the tokens back.
        if self.peek().is_symbol("(") {
            let saved = self.position;
            if let Ok(args) = self.parse_parenthesised_arguments() {
                if self.peek().is_end_of_line() {
                    return Ok(StatementKind::Command {
                        name,
                        arguments: args,
                    });
                }
            }
            self.position = saved;
        }

        // `else` never starts an argument: it belongs to the `if` that this
        // command may be the body of, as in `if x then beep else beep`.
        if !self.peek().is_end_of_line() && !self.peek().is_any(&["with", "else"]) {
            loop {
                arguments.push(self.parse_expression()?);
                if !self.accept_symbol(",") {
                    break;
                }
            }
        }
        // `wait 2 seconds`: a trailing unit is just one more argument.
        if let Some(unit) = self.accept_any(UNIT_WORDS) {
            arguments.push(Expr::Variable(unit));
        }
        // `ask "Name?" with "Bob"` supplies a trailing argument.
        if self.accept("with") {
            arguments.push(self.parse_expression()?);
        }
        Ok(StatementKind::Command { name, arguments })
    }

    fn parse_parenthesised_arguments(&mut self) -> ParseResult<Vec<Expr>> {
        self.expect_symbol("(")?;
        let mut arguments = Vec::new();
        if !self.peek().is_symbol(")") {
            loop {
                arguments.push(self.parse_expression()?);
                if !self.accept_symbol(",") {
                    break;
                }
            }
        }
        self.expect_symbol(")")?;
        Ok(arguments)
    }

    // ----------------------------------------------------------- containers

    /// Parses somewhere a value can be stored.
    fn parse_container(&mut self) -> ParseResult<Container> {
        let chunks = self.parse_chunk_prefixes()?;
        let base = self.parse_container_base()?;
        Ok(Container { base, chunks })
    }

    /// Reads any leading `word 2 of`, `char 1 to 3 of` … prefixes.
    fn parse_chunk_prefixes(&mut self) -> ParseResult<Vec<Chunk>> {
        let mut chunks = Vec::new();
        while let Some(kind) = self.peek_chunk_kind() {
            self.advance();
            let start = Box::new(self.parse_expression()?);
            let end = if self.accept("to") {
                Some(Box::new(self.parse_expression()?))
            } else {
                None
            };
            self.expect("of")?;
            chunks.push(Chunk { kind, start, end });
        }
        Ok(chunks)
    }

    /// Whether the current token starts a chunk, and which kind.
    fn peek_chunk_kind(&self) -> Option<ChunkKind> {
        if !self.peek().is_any(CHUNK_WORDS) {
            return None;
        }
        // `word` is a chunk only when something follows it on the same line;
        // otherwise it is an ordinary name.
        if self.peek_at(1).is_end_of_line() {
            return None;
        }
        Some(match self.peek().keyword() {
            "char" | "chars" | "character" | "characters" => ChunkKind::Char,
            "word" | "words" => ChunkKind::Word,
            "item" | "items" => ChunkKind::Item,
            _ => ChunkKind::Line,
        })
    }

    fn parse_container_base(&mut self) -> ParseResult<ContainerBase> {
        if self.accept("it") {
            return Ok(ContainerBase::It);
        }
        self.accept("the");
        if self.peek().is_any(&["message", "msg"]) {
            self.advance();
            self.skip_any(&["box", "window"]);
            return Ok(ContainerBase::MessageBox);
        }
        if self.peek().is_any(OBJECT_WORDS) || self.peek().is("target") {
            return Ok(ContainerBase::Object(self.parse_object_ref()?));
        }
        let name = self.expect_word("a container")?;
        Ok(ContainerBase::Variable(name))
    }

    // -------------------------------------------------------------- objects

    fn parse_object_ref(&mut self) -> ParseResult<ObjectRef> {
        self.accept("the");
        if self.accept("me") {
            return Ok(ObjectRef::Me);
        }
        if self.accept("target") {
            return Ok(ObjectRef::Target);
        }
        if self.peek().is("this") {
            self.advance();
            return if self.accept("stack") {
                Ok(ObjectRef::Stack)
            } else if self.accept_any(&["background", "bkgnd", "bg"]).is_some() {
                Ok(ObjectRef::Background(Box::new(Specifier::Current)))
            } else {
                self.skip_any(&["card", "cd"]);
                Ok(ObjectRef::Card(Box::new(Specifier::Current)))
            };
        }
        if self.accept("stack") {
            return Ok(ObjectRef::Stack);
        }

        // A leading `card` or `background` may qualify a part instead of
        // naming one: `card field 1` is a field, not a card.
        let layer = if self.peek().is_any(&["card", "cd"])
            && self.peek_at(1).is_any(&["button", "btn", "field", "fld"])
        {
            self.advance();
            Layer::Card
        } else if self.peek().is_any(&["background", "bkgnd", "bg"])
            && self.peek_at(1).is_any(&["button", "btn", "field", "fld"])
        {
            self.advance();
            Layer::Background
        } else {
            Layer::Unspecified
        };

        if let Some(word) = self.accept_any(&["button", "btn", "field", "fld"]) {
            let kind = if word.starts_with('b') {
                PartKind::Button
            } else {
                PartKind::Field
            };
            let specifier = Box::new(self.parse_specifier()?);
            let owner = if self.accept("of") {
                Some(Box::new(self.parse_object_ref()?))
            } else {
                None
            };
            return Ok(ObjectRef::Part {
                kind,
                layer,
                specifier,
                owner,
            });
        }

        if self.accept_any(&["card", "cd"]).is_some() {
            return Ok(ObjectRef::Card(Box::new(self.parse_specifier()?)));
        }
        if self.accept_any(&["background", "bkgnd", "bg"]).is_some() {
            return Ok(ObjectRef::Background(Box::new(self.parse_specifier()?)));
        }
        Err(self.unexpected("an object such as `card 1` or `field \"Name\"`"))
    }

    /// Reads which one of several objects is meant.
    ///
    /// The expression here is parsed at the tightest level, so that
    /// `field "Name" & "!"` is `(field "Name") & "!"` and not a field with a
    /// very odd name. Anything more elaborate needs brackets:
    /// `field (base & suffix)`.
    fn parse_specifier(&mut self) -> ParseResult<Specifier> {
        if self.accept("id") {
            return Ok(Specifier::Id(self.parse_unary()?));
        }
        if let Some(ordinal) = self.accept_ordinal() {
            return Ok(Specifier::Ordinal(ordinal));
        }
        if self.peek().is_end_of_line() || !self.starts_expression() {
            return Ok(Specifier::Current);
        }
        Ok(Specifier::Value(self.parse_unary()?))
    }

    /// Whether the current token could begin an expression.
    ///
    /// Keywords that only ever join clauses (`of`, `to`, `then`, …) cannot,
    /// which is how `go to card` knows it has run out of specifier.
    fn starts_expression(&self) -> bool {
        match &self.peek().kind {
            TokenKind::Number(_) | TokenKind::Text(_) => true,
            TokenKind::Symbol(symbol) => *symbol == "(" || *symbol == "-",
            TokenKind::Word { lower, .. } => !CLAUSE_WORDS.contains(&lower.as_str()),
            TokenKind::Newline | TokenKind::EndOfScript => false,
        }
    }

    fn accept_ordinal(&mut self) -> Option<Ordinal> {
        let ordinal = match self.peek().keyword() {
            "first" => Ordinal::First,
            "second" => Ordinal::Second,
            "third" => Ordinal::Third,
            "fourth" => Ordinal::Fourth,
            "fifth" => Ordinal::Fifth,
            "last" => Ordinal::Last,
            "mid" | "middle" => Ordinal::Middle,
            "any" => Ordinal::Any,
            "next" => Ordinal::Next,
            "prev" | "previous" => Ordinal::Previous,
            _ => return None,
        };
        self.advance();
        Some(ordinal)
    }

    // ---------------------------------------------------------- expressions

    /// Parses an expression. Precedence, loosest first: `or`, `and`, `not`,
    /// comparisons, `&`, `+ -`, `* / div mod`, `^`, prefix `-`.
    fn parse_expression(&mut self) -> ParseResult<Expr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_and()?;
        while self.accept("or") {
            let right = self.parse_and()?;
            left = binary(BinaryOp::Or, left, right);
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_not()?;
        while self.accept("and") {
            let right = self.parse_not()?;
            left = binary(BinaryOp::And, left, right);
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> ParseResult<Expr> {
        if self.accept("not") {
            let operand = self.parse_not()?;
            return Ok(Expr::Unary {
                operator: UnaryOp::Not,
                operand: Box::new(operand),
            });
        }
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_concat()?;
        loop {
            let operator = if self.accept_symbol("=") {
                BinaryOp::Equal
            } else if self.accept_symbol("<>") {
                BinaryOp::NotEqual
            } else if self.accept_symbol("<=") {
                BinaryOp::LessOrEqual
            } else if self.accept_symbol(">=") {
                BinaryOp::GreaterOrEqual
            } else if self.accept_symbol("<") {
                BinaryOp::Less
            } else if self.accept_symbol(">") {
                BinaryOp::Greater
            } else if self.accept("contains") {
                BinaryOp::Contains
            } else if self.peek().is("starts") && self.peek_at(1).is("with") {
                self.advance();
                self.advance();
                BinaryOp::StartsWith
            } else if self.peek().is("ends") && self.peek_at(1).is("with") {
                self.advance();
                self.advance();
                BinaryOp::EndsWith
            } else if self.peek().is("is") {
                self.advance();
                let negated = self.accept("not");
                let operator = if self.accept("in") {
                    BinaryOp::IsIn
                } else if negated {
                    BinaryOp::NotEqual
                } else {
                    BinaryOp::Equal
                };
                let right = self.parse_concat()?;
                left = binary(operator, left, right);
                if negated && operator == BinaryOp::IsIn {
                    left = Expr::Unary {
                        operator: UnaryOp::Not,
                        operand: Box::new(left),
                    };
                }
                continue;
            } else {
                return Ok(left);
            };
            let right = self.parse_concat()?;
            left = binary(operator, left, right);
        }
    }

    fn parse_concat(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_additive()?;
        loop {
            let operator = if self.accept_symbol("&&") {
                BinaryOp::ConcatSpace
            } else if self.accept_symbol("&") {
                BinaryOp::Concat
            } else {
                return Ok(left);
            };
            let right = self.parse_additive()?;
            left = binary(operator, left, right);
        }
    }

    fn parse_additive(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let operator = if self.accept_symbol("+") {
                BinaryOp::Add
            } else if self.accept_symbol("-") {
                BinaryOp::Subtract
            } else {
                return Ok(left);
            };
            let right = self.parse_multiplicative()?;
            left = binary(operator, left, right);
        }
    }

    fn parse_multiplicative(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_power()?;
        loop {
            let operator = if self.accept_symbol("*") {
                BinaryOp::Multiply
            } else if self.accept_symbol("/") {
                BinaryOp::Divide
            } else if self.accept("div") {
                BinaryOp::IntegerDivide
            } else if self.accept("mod") {
                BinaryOp::Modulo
            } else {
                return Ok(left);
            };
            let right = self.parse_power()?;
            left = binary(operator, left, right);
        }
    }

    fn parse_power(&mut self) -> ParseResult<Expr> {
        let left = self.parse_unary()?;
        if self.accept_symbol("^") {
            // Right-associative: 2^3^2 is 2^(3^2).
            let right = self.parse_power()?;
            return Ok(binary(BinaryOp::Power, left, right));
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> ParseResult<Expr> {
        if self.accept_symbol("-") {
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary {
                operator: UnaryOp::Negate,
                operand: Box::new(operand),
            });
        }
        if self.accept("not") {
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary {
                operator: UnaryOp::Not,
                operand: Box::new(operand),
            });
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> ParseResult<Expr> {
        // Chunks bind tighter than anything else: `word 2 of x & y` is
        // `(word 2 of x) & y`.
        if self.peek_chunk_kind().is_some() {
            let chunks = self.parse_chunk_prefixes()?;
            let source = Box::new(self.parse_unary()?);
            return Ok(Expr::Chunk { chunks, source });
        }

        match self.peek().kind.clone() {
            TokenKind::Number(value) => {
                self.advance();
                Ok(Expr::Number(value))
            }
            TokenKind::Text(text) => {
                self.advance();
                Ok(Expr::Text(text))
            }
            TokenKind::Symbol("(") => {
                self.advance();
                let inner = self.parse_expression()?;
                self.expect_symbol(")")?;
                Ok(inner)
            }
            TokenKind::Word { text, lower } => self.parse_word_expression(&text, &lower),
            _ => Err(self.unexpected("a value")),
        }
    }

    fn parse_word_expression(&mut self, text: &str, lower: &str) -> ParseResult<Expr> {
        match lower {
            "the" => {
                self.advance();
                self.parse_the()
            }
            "it" => {
                self.advance();
                Ok(Expr::It)
            }
            "there" => self.parse_there_is(),
            _ if CONSTANTS.contains(&lower) => {
                self.advance();
                Ok(Expr::Constant(lower.to_string()))
            }
            _ if OBJECT_WORDS.contains(&lower) => Ok(Expr::Object(self.parse_object_ref()?)),
            _ => {
                self.advance();
                if self.peek().is_symbol("(") {
                    let arguments = self.parse_parenthesised_arguments()?;
                    return Ok(Expr::Call {
                        name: text.to_string(),
                        arguments,
                    });
                }
                Ok(Expr::Variable(text.to_string()))
            }
        }
    }

    /// Parses `there is a card "Home"` and `there is no field "Notes"`.
    fn parse_there_is(&mut self) -> ParseResult<Expr> {
        self.expect("there")?;
        self.expect("is")?;
        let negated = if self.accept("no") {
            true
        } else {
            self.skip_any(&["a", "an"]);
            false
        };
        let object = Box::new(self.parse_object_ref()?);
        Ok(Expr::Exists { object, negated })
    }

    /// Parses everything that follows `the`.
    fn parse_the(&mut self) -> ParseResult<Expr> {
        if self.peek().is("number") && self.peek_at(1).is("of") {
            self.advance();
            self.advance();
            return Ok(Expr::Count(Box::new(self.parse_count_target()?)));
        }
        if self.peek().is("target") {
            self.advance();
            return Ok(Expr::Object(ObjectRef::Target));
        }
        if self.peek().is_any(OBJECT_WORDS) {
            return Ok(Expr::Object(self.parse_object_ref()?));
        }

        let mut name = self
            .expect_word("a property or function name")?
            .to_ascii_lowercase();
        // `the long date`, `the short name of me`.
        if QUALIFIERS.contains(&name.as_str()) {
            let second = self.expect_word("a property or function name")?;
            name = format!("{name} {}", second.to_ascii_lowercase());
        }
        if self.accept("of") {
            let operand = Box::new(self.parse_unary()?);
            Ok(Expr::Of { name, operand })
        } else {
            Ok(Expr::The(name))
        }
    }

    fn parse_count_target(&mut self) -> ParseResult<CountTarget> {
        let layer = if self.peek().is_any(&["card", "cd"])
            && self
                .peek_at(1)
                .is_any(&["buttons", "btns", "fields", "flds"])
        {
            self.advance();
            Layer::Card
        } else if self.peek().is_any(&["background", "bkgnd", "bg"])
            && self
                .peek_at(1)
                .is_any(&["buttons", "btns", "fields", "flds"])
        {
            self.advance();
            Layer::Background
        } else {
            Layer::Unspecified
        };

        let word = self.expect_word("something to count")?.to_ascii_lowercase();
        let kind = match word.as_str() {
            "cards" | "cds" => return Ok(CountTarget::Cards),
            "backgrounds" | "bkgnds" | "bgs" => return Ok(CountTarget::Backgrounds),
            "buttons" | "btns" => PartKind::Button,
            "fields" | "flds" => PartKind::Field,
            "chars" | "characters" => return self.parse_chunk_count(ChunkKind::Char),
            "words" => return self.parse_chunk_count(ChunkKind::Word),
            "items" => return self.parse_chunk_count(ChunkKind::Item),
            "lines" => return self.parse_chunk_count(ChunkKind::Line),
            other => {
                return Err(self.error_here(format!("I do not know how to count \"{other}\"")));
            }
        };
        let owner = if self.accept("of") {
            Some(self.parse_object_ref()?)
        } else {
            None
        };
        Ok(CountTarget::Parts { kind, layer, owner })
    }

    fn parse_chunk_count(&mut self, kind: ChunkKind) -> ParseResult<CountTarget> {
        self.expect("of")?;
        let source = self.parse_unary()?;
        Ok(CountTarget::Chunks { kind, source })
    }
}

/// Builds a binary node, boxing its operands.
fn binary(operator: BinaryOp, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        operator,
        left: Box::new(left),
        right: Box::new(right),
    }
}
