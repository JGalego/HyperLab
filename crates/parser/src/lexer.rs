//! Turning source text into [`Token`]s.
//!
//! The lexer knows nothing about keywords: `repeat` and `myVariable` are both
//! [`TokenKind::Word`]. Keyword recognition happens in the parser, which is
//! why adding a command never means touching this file.

use crate::{
    error::{ParseError, ParseResult},
    token::{Token, TokenKind},
};

/// Every multi-character symbol, longest first so that `<=` wins over `<`.
const SYMBOLS: &[&str] = &[
    "&&", "<=", ">=", "<>", "&", "+", "-", "*", "/", "^", "=", "<", ">", "(", ")", ",",
];

/// Splits HyperTalk source into tokens.
///
/// # Errors
///
/// Returns a [`ParseError`] for unterminated strings and characters that
/// cannot begin a token.
pub fn tokenize(source: &str) -> ParseResult<Vec<Token>> {
    Lexer::new(source).run()
}

struct Lexer {
    chars: Vec<char>,
    position: usize,
    line: u32,
    column: u32,
}

impl Lexer {
    fn new(source: &str) -> Self {
        Self {
            chars: source.chars().collect(),
            position: 0,
            line: 1,
            column: 1,
        }
    }

    fn run(mut self) -> ParseResult<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            self.skip_inline_space();
            let (line, column) = (self.line, self.column);
            let Some(c) = self.peek() else {
                tokens.push(Token::new(TokenKind::EndOfScript, line, column));
                return Ok(tokens);
            };
            match c {
                '\n' => {
                    self.advance();
                    self.line += 1;
                    self.column = 1;
                    tokens.push(Token::new(TokenKind::Newline, line, column));
                }
                '"' => {
                    let text = self.read_string()?;
                    tokens.push(Token::new(TokenKind::Text(text), line, column));
                }
                c if c.is_ascii_digit() => {
                    let number = self.read_number()?;
                    tokens.push(Token::new(TokenKind::Number(number), line, column));
                }
                c if is_word_start(c) => {
                    let word = self.read_word();
                    tokens.push(Token::new(TokenKind::word(word), line, column));
                }
                _ => {
                    if let Some(symbol) = self.read_symbol() {
                        tokens.push(Token::new(TokenKind::Symbol(symbol), line, column));
                    } else {
                        return Err(ParseError::new(
                            format!("I do not know what to do with \"{c}\""),
                            line,
                            column,
                        ));
                    }
                }
            }
        }
    }

    /// Skips spaces, comments and line continuations, but never a newline.
    fn skip_inline_space(&mut self) {
        loop {
            match self.peek() {
                Some(' ' | '\t' | '\r') => {
                    self.advance();
                }
                // `--` starts a comment that runs to the end of the line.
                Some('-') if self.peek_at(1) == Some('-') => {
                    while self.peek().is_some_and(|c| c != '\n') {
                        self.advance();
                    }
                }
                // `\` and the classic `¬` continue a statement on the next
                // line, so the newline that follows is swallowed.
                Some('\\' | '¬') if self.rest_of_line_is_empty(1) => {
                    while self.peek().is_some_and(|c| c != '\n') {
                        self.advance();
                    }
                    if self.peek() == Some('\n') {
                        self.advance();
                        self.line += 1;
                        self.column = 1;
                    }
                }
                _ => return,
            }
        }
    }

    /// Whether everything after `offset` characters up to the newline is
    /// blank, which is what makes a `\` a line continuation rather than a
    /// stray character.
    fn rest_of_line_is_empty(&self, offset: usize) -> bool {
        let mut index = self.position + offset;
        while let Some(&c) = self.chars.get(index) {
            match c {
                '\n' => return true,
                ' ' | '\t' | '\r' => index += 1,
                _ => return false,
            }
        }
        true
    }

    fn read_string(&mut self) -> ParseResult<String> {
        let (line, column) = (self.line, self.column);
        self.advance(); // opening quote
        let mut text = String::new();
        loop {
            match self.peek() {
                None | Some('\n') => {
                    return Err(ParseError::new(
                        "this string is missing its closing quote",
                        line,
                        column,
                    ));
                }
                Some('"') => {
                    self.advance();
                    return Ok(text);
                }
                // Classic HyperTalk had no escapes at all; HyperLab accepts
                // the handful people now expect, and `quote` still works.
                Some('\\') => {
                    self.advance();
                    let escaped = match self.peek() {
                        Some('n') => '\n',
                        Some('t') => '\t',
                        Some(c) => c,
                        None => {
                            return Err(ParseError::new(
                                "this string is missing its closing quote",
                                line,
                                column,
                            ));
                        }
                    };
                    text.push(escaped);
                    self.advance();
                }
                Some(c) => {
                    text.push(c);
                    self.advance();
                }
            }
        }
    }

    fn read_number(&mut self) -> ParseResult<f64> {
        let (line, column) = (self.line, self.column);
        let start = self.position;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.advance();
        }
        // A dot is part of the number only when a digit follows it, so that
        // `3.` and future member syntax stay unambiguous.
        if self.peek() == Some('.') && self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            self.advance();
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.advance();
            }
        }
        let text: String = self.chars[start..self.position].iter().collect();
        text.parse()
            .map_err(|_| ParseError::new(format!("\"{text}\" is not a number"), line, column))
    }

    fn read_word(&mut self) -> String {
        let start = self.position;
        while self.peek().is_some_and(is_word_part) {
            self.advance();
        }
        self.chars[start..self.position].iter().collect()
    }

    fn read_symbol(&mut self) -> Option<&'static str> {
        // The typographic operators of the classic language are accepted and
        // folded onto their ASCII equivalents.
        match self.peek() {
            Some('≠') => {
                self.advance();
                return Some("<>");
            }
            Some('≤') => {
                self.advance();
                return Some("<=");
            }
            Some('≥') => {
                self.advance();
                return Some(">=");
            }
            _ => {}
        }
        for symbol in SYMBOLS {
            if self.matches(symbol) {
                for _ in 0..symbol.chars().count() {
                    self.advance();
                }
                return Some(symbol);
            }
        }
        None
    }

    fn matches(&self, text: &str) -> bool {
        text.chars()
            .enumerate()
            .all(|(offset, c)| self.peek_at(offset) == Some(c))
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.position).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.position + offset).copied()
    }

    fn advance(&mut self) {
        self.position += 1;
        self.column += 1;
    }
}

/// Whether `c` can start a word. Underscores are allowed so that scripts can
/// use them in variable names.
fn is_word_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

/// Whether `c` can continue a word.
fn is_word_part(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        tokenize(source)
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn words_keep_their_spelling_and_gain_a_lower_case_form() {
        let tokens = tokenize("mouseUp").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::word("mouseUp"));
        assert_eq!(tokens[0].keyword(), "mouseup");
    }

    #[test]
    fn newlines_are_tokens_because_statements_are_lines() {
        assert_eq!(
            kinds("beep\nbeep"),
            vec![
                TokenKind::word("beep"),
                TokenKind::Newline,
                TokenKind::word("beep"),
                TokenKind::EndOfScript,
            ]
        );
    }

    #[test]
    fn comments_run_to_the_end_of_the_line() {
        assert_eq!(
            kinds("beep -- make a noise\nbeep"),
            vec![
                TokenKind::word("beep"),
                TokenKind::Newline,
                TokenKind::word("beep"),
                TokenKind::EndOfScript,
            ]
        );
    }

    #[test]
    fn a_trailing_backslash_joins_two_lines() {
        assert_eq!(
            kinds("put 1 \\\n+ 2"),
            vec![
                TokenKind::word("put"),
                TokenKind::Number(1.0),
                TokenKind::Symbol("+"),
                TokenKind::Number(2.0),
                TokenKind::EndOfScript,
            ]
        );
    }

    #[test]
    fn a_backslash_inside_a_line_is_not_a_continuation() {
        assert!(tokenize("put 1 \\ 2").is_err());
    }

    #[test]
    fn strings_understand_the_usual_escapes() {
        assert_eq!(
            kinds(r#""a\"b""#),
            vec![TokenKind::Text("a\"b".into()), TokenKind::EndOfScript]
        );
        assert_eq!(
            kinds(r#""a\nb""#),
            vec![TokenKind::Text("a\nb".into()), TokenKind::EndOfScript]
        );
    }

    #[test]
    fn an_unterminated_string_reports_where_it_started() {
        let error = tokenize("put \"oops\ninto x").unwrap_err();
        assert_eq!(error.line, 1);
        assert!(error.message.contains("closing quote"));
    }

    #[test]
    fn numbers_stop_before_a_lone_dot() {
        assert_eq!(
            kinds("3.5"),
            vec![TokenKind::Number(3.5), TokenKind::EndOfScript]
        );
    }

    #[test]
    fn long_symbols_win_over_short_ones() {
        assert_eq!(
            kinds("a && b <= c"),
            vec![
                TokenKind::word("a"),
                TokenKind::Symbol("&&"),
                TokenKind::word("b"),
                TokenKind::Symbol("<="),
                TokenKind::word("c"),
                TokenKind::EndOfScript,
            ]
        );
    }

    #[test]
    fn classic_typographic_operators_are_accepted() {
        assert_eq!(
            kinds("a ≠ b"),
            vec![
                TokenKind::word("a"),
                TokenKind::Symbol("<>"),
                TokenKind::word("b"),
                TokenKind::EndOfScript,
            ]
        );
    }

    #[test]
    fn positions_are_reported_from_one() {
        let tokens = tokenize("beep\n  beep").unwrap();
        assert_eq!((tokens[0].line, tokens[0].column), (1, 1));
        assert_eq!((tokens[2].line, tokens[2].column), (2, 3));
    }
}
