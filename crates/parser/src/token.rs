//! Tokens produced by the lexer.

use std::fmt;

/// A token and where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// What the token is.
    pub kind: TokenKind,
    /// Source line, counting from one.
    pub line: u32,
    /// Source column, counting from one.
    pub column: u32,
}

impl Token {
    /// Builds a token.
    #[must_use]
    pub const fn new(kind: TokenKind, line: u32, column: u32) -> Self {
        Self { kind, line, column }
    }

    /// The token's word, lower-cased, or `""` for anything else.
    ///
    /// HyperTalk is case-insensitive, so every keyword test goes through here.
    #[must_use]
    pub fn keyword(&self) -> &str {
        match &self.kind {
            TokenKind::Word { lower, .. } => lower,
            _ => "",
        }
    }

    /// Whether this token is the given keyword.
    #[must_use]
    pub fn is(&self, keyword: &str) -> bool {
        self.keyword() == keyword
    }

    /// Whether this token is any of the given keywords.
    #[must_use]
    pub fn is_any(&self, keywords: &[&str]) -> bool {
        let word = self.keyword();
        !word.is_empty() && keywords.contains(&word)
    }

    /// Whether this token is the given symbol.
    #[must_use]
    pub fn is_symbol(&self, symbol: &str) -> bool {
        matches!(&self.kind, TokenKind::Symbol(s) if *s == symbol)
    }

    /// Whether this token ends a line or the script.
    #[must_use]
    pub fn is_end_of_line(&self) -> bool {
        matches!(self.kind, TokenKind::Newline | TokenKind::EndOfScript)
    }
}

/// The kinds of token HyperTalk source is made of.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// A bare word: a keyword, a variable, a command or a property name.
    Word {
        /// As written, for error messages and for names the runtime shows.
        text: String,
        /// Lower-cased, for comparisons.
        lower: String,
    },
    /// A number literal.
    Number(f64),
    /// A quoted string, with the quotes removed and escapes resolved.
    Text(String),
    /// An operator or a piece of punctuation.
    Symbol(&'static str),
    /// The end of a line: HyperTalk statements are line-delimited.
    Newline,
    /// The end of the source.
    EndOfScript,
}

impl TokenKind {
    /// Builds a word token, computing its lower-cased form.
    #[must_use]
    pub fn word(text: impl Into<String>) -> Self {
        let text = text.into();
        let lower = text.to_ascii_lowercase();
        Self::Word { text, lower }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Word { text, .. } => write!(f, "\"{text}\""),
            Self::Number(n) => write!(f, "{n}"),
            Self::Text(s) => write!(f, "\"{s}\""),
            Self::Symbol(s) => write!(f, "\"{s}\""),
            Self::Newline => f.write_str("the end of the line"),
            Self::EndOfScript => f.write_str("the end of the script"),
        }
    }
}
