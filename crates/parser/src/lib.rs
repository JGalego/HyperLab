//! A modern HyperTalk front end: lexer, parser and AST.
//!
//! This crate is deliberately standalone. It depends on nothing — not even on
//! HyperLab's object model — because a parser that cannot be tested on its
//! own is a parser nobody will maintain.
//!
//! ```
//! use hyperlab_parser::{parse, ast::{HandlerKind, StatementKind}};
//!
//! let script = parse(r#"
//!     on mouseUp
//!         put "Hello" into field "Greeting"
//!     end mouseUp
//! "#).unwrap();
//!
//! let handler = script.handler(HandlerKind::Message, "mouseUp").unwrap();
//! assert!(matches!(handler.body[0].kind, StatementKind::Put { .. }));
//! ```
//!
//! # What the runtime is responsible for
//!
//! The parser answers "what did the author write?" and never "what does it
//! mean?". It does not know which names are properties, which are functions,
//! or which commands exist. See [`ast`] for why.

#![warn(missing_docs)]

pub mod ast;
mod error;
mod lexer;
mod parser;
mod token;

pub use error::{ParseError, ParseResult};
pub use lexer::tokenize;
pub use parser::{parse, parse_expression};
pub use token::{Token, TokenKind};
