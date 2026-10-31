use std::iter::Peekable;
use std::str::Chars;

use crate::error::Error;

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    // punctuation
    LeftBrace,
    RightBrace,
    LeftParen,
    RightParen,
    Comma,
    Semicolon,
    Newline,
    Dot,

    // operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Assign,
    Is,
    IsNot,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    And,
    Or,
    Not,

    // keywords
    Let,
    Mut,
    Const,
    Static,
    Del,
    Type,
    If,
    Else,
    While,
    Loop,
    For,
    In,
    Break,
    Continue,
    True,
    False,
    Hashset,
    Matrix,

    // literals
    Identifier(String),
    Int(i64),
    Float(f64),
    Text(String),

    Eof,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
}

pub fn lex(source: &str) -> Result<Vec<Token>, Error> {
    let mut tokens: Vec<Token> = Vec::new();
    let mut chars = source.chars().peekable();
    let mut line = 1usize;

    while let Some(&ch) = chars.peek() {
        let token_line = line;

        match ch {
            ' ' | '\t' | '\r' => {
                chars.next();
            }
            '\n' => {
                chars.next();
                tokens.push(Token {
                    kind: TokenKind::Newline,
                    line,
                });
                line += 1;
            }
            '{' => single(&mut chars, &mut tokens, TokenKind::LeftBrace, token_line),
            '}' => single(&mut chars, &mut tokens, TokenKind::RightBrace, token_line),
            '(' => single(&mut chars, &mut tokens, TokenKind::LeftParen, token_line),
            ')' => single(&mut chars, &mut tokens, TokenKind::RightParen, token_line),
            ',' => single(&mut chars, &mut tokens, TokenKind::Comma, token_line),
            ';' => single(&mut chars, &mut tokens, TokenKind::Semicolon, token_line),
            '.' => single(&mut chars, &mut tokens, TokenKind::Dot, token_line),
            '+' => single(&mut chars, &mut tokens, TokenKind::Plus, token_line),
            '-' => single(&mut chars, &mut tokens, TokenKind::Minus, token_line),
            '*' => single(&mut chars, &mut tokens, TokenKind::Star, token_line),
            '/' => single(&mut chars, &mut tokens, TokenKind::Slash, token_line),
            '%' => single(&mut chars, &mut tokens, TokenKind::Percent, token_line),
            '=' => {
                chars.next();
                let kind = if chars.peek() == Some(&'=') {
                    chars.next();
                    TokenKind::Is
                } else {
                    TokenKind::Assign
                };
                tokens.push(Token {
                    kind,
                    line: token_line,
                });
            }
            '!' => {
                chars.next();
                let kind = if chars.peek() == Some(&'=') {
                    chars.next();
                    TokenKind::IsNot
                } else {
                    TokenKind::Not
                };
                tokens.push(Token {
                    kind,
                    line: token_line,
                });
            }
            '>' => {
                chars.next();
                let kind = if chars.peek() == Some(&'=') {
                    chars.next();
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::Greater
                };
                tokens.push(Token {
                    kind,
                    line: token_line,
                });
            }
            '<' => {
                chars.next();
                let kind = if chars.peek() == Some(&'=') {
                    chars.next();
                    TokenKind::LessEqual
                } else {
                    TokenKind::Less
                };
                tokens.push(Token {
                    kind,
                    line: token_line,
                });
            }
            '&' => {
                chars.next();
                if chars.peek() == Some(&'&') {
                    chars.next();
                    tokens.push(Token {
                        kind: TokenKind::And,
                        line: token_line,
                    });
                } else {
                    return Err(Error::new(token_line, "use '&&' or 'and' for and"));
                }
            }
            '|' => {
                chars.next();
                if chars.peek() == Some(&'|') {
                    chars.next();
                    tokens.push(Token {
                        kind: TokenKind::Or,
                        line: token_line,
                    });
                } else {
                    return Err(Error::new(token_line, "use '||' or 'or' for or"));
                }
            }
            '0'..='9' => {
                let kind = number(&mut chars, token_line)?;
                tokens.push(Token {
                    kind,
                    line: token_line,
                });
            }
            '"' | '\'' => {
                let kind = text(&mut chars, &mut line)?;
                tokens.push(Token {
                    kind,
                    line: token_line,
                });
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let word = identifier(&mut chars);
                tokens.push(Token {
                    kind: keyword(&word),
                    line: token_line,
                });
            }
            other => {
                return Err(Error::new(
                    token_line,
                    format!("unexpected character '{other}'"),
                ));
            }
        }
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        line,
    });
    Ok(tokens)
}

fn single(
    chars: &mut Peekable<Chars<'_>>,
    tokens: &mut Vec<Token>,
    kind: TokenKind,
    line: usize,
) {
    chars.next();
    tokens.push(Token { kind, line });
}

fn identifier(chars: &mut Peekable<Chars<'_>>) -> String {
    let mut word = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_alphanumeric() || c == '_' {
            word.push(c);
            chars.next();
        } else {
            break;
        }
    }
    word
}

fn keyword(word: &str) -> TokenKind {
    match word {
        "let" => TokenKind::Let,
        "mut" => TokenKind::Mut,
        "const" => TokenKind::Const,
        "static" => TokenKind::Static,
        "del" => TokenKind::Del,
        "type" => TokenKind::Type,
        "if" => TokenKind::If,
        "else" => TokenKind::Else,
        "while" => TokenKind::While,
        "loop" => TokenKind::Loop,
        "for" => TokenKind::For,
        "in" => TokenKind::In,
        "break" => TokenKind::Break,
        "continue" => TokenKind::Continue,
        "true" => TokenKind::True,
        "false" => TokenKind::False,
        "hashset" | "set" => TokenKind::Hashset,
        "matrix" | "matrex" => TokenKind::Matrix,
        "and" => TokenKind::And,
        "or" => TokenKind::Or,
        "not" => TokenKind::Not,
        "is" => TokenKind::Is,
        "is_not" => TokenKind::IsNot,
        "greater" => TokenKind::Greater,
        "less" => TokenKind::Less,
        "greater_equal" => TokenKind::GreaterEqual,
        "less_equal" => TokenKind::LessEqual,
        _ => TokenKind::Identifier(word.to_string()),
    }
}

fn number(chars: &mut Peekable<Chars<'_>>, line: usize) -> Result<TokenKind, Error> {
    let mut text = String::new();

    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            text.push(c);
            chars.next();
        } else {
            break;
        }
    }

    let mut is_float = false;
    if chars.peek() == Some(&'.') {
        chars.next();
        is_float = true;
        text.push('.');

        let mut digits = 0;
        while let Some(&c) = chars.peek() {
            if c.is_ascii_digit() {
                text.push(c);
                chars.next();
                digits += 1;
            } else {
                break;
            }
        }

        if digits == 0 {
            return Err(Error::new(line, format!("invalid number '{text}'")));
        }
    }

    if is_float {
        text.parse::<f64>()
            .map(TokenKind::Float)
            .map_err(|_| Error::new(line, format!("invalid number '{text}'")))
    } else {
        text.parse::<i64>()
            .map(TokenKind::Int)
            .map_err(|_| Error::new(line, format!("number is too large: {text}")))
    }
}

fn text(chars: &mut Peekable<Chars<'_>>, line: &mut usize) -> Result<TokenKind, Error> {
    let quote = chars.next().unwrap_or('"');
    let mut text = String::new();

    loop {
        match chars.next() {
            None => return Err(Error::new(*line, "unterminated text literal")),
            Some('\n') => {
                *line += 1;
                text.push('\n');
            }
            Some('\\') => match chars.next() {
                Some('n') => text.push('\n'),
                Some('t') => text.push('\t'),
                Some('r') => text.push('\r'),
                Some('\\') => text.push('\\'),
                Some('"') => text.push('"'),
                Some('\'') => text.push('\''),
                Some(other) => {
                    text.push('\\');
                    text.push(other);
                }
                None => return Err(Error::new(*line, "unterminated text literal")),
            },
            Some(c) if c == quote => break,
            Some(c) => text.push(c),
        }
    }

    Ok(TokenKind::Text(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        lex(source)
            .unwrap()
            .into_iter()
            .map(|token| token.kind)
            .filter(|kind| *kind != TokenKind::Newline)
            .collect()
    }

    #[test]
    fn lexes_operators_and_keywords() {
        assert_eq!(
            kinds("if a is_not b and not c { }"),
            vec![
                TokenKind::If,
                TokenKind::Identifier("a".into()),
                TokenKind::IsNot,
                TokenKind::Identifier("b".into()),
                TokenKind::And,
                TokenKind::Not,
                TokenKind::Identifier("c".into()),
                TokenKind::LeftBrace,
                TokenKind::RightBrace,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_symbolic_aliases() {
        assert_eq!(
            kinds("a != b && !c || d >= 1"),
            vec![
                TokenKind::Identifier("a".into()),
                TokenKind::IsNot,
                TokenKind::Identifier("b".into()),
                TokenKind::And,
                TokenKind::Not,
                TokenKind::Identifier("c".into()),
                TokenKind::Or,
                TokenKind::Identifier("d".into()),
                TokenKind::GreaterEqual,
                TokenKind::Int(1),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn decodes_escapes_in_text() {
        assert_eq!(
            kinds(r#"console.println("a\nb")"#),
            vec![
                TokenKind::Identifier("console".into()),
                TokenKind::Dot,
                TokenKind::Identifier("println".into()),
                TokenKind::LeftParen,
                TokenKind::Text("a\nb".into()),
                TokenKind::RightParen,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn tracks_line_numbers_across_newlines() {
        let tokens = lex("let a = 1\nlet b = 2\n").unwrap();
        assert_eq!(tokens[0].line, 1);

        let second_let = tokens
            .iter()
            .enumerate()
            .filter(|(_, token)| token.kind == TokenKind::Let)
            .map(|(index, _)| index)
            .nth(1)
            .unwrap();
        assert_eq!(tokens[second_let].line, 2);
    }

    #[test]
    fn rejects_unterminated_text() {
        let error = lex(r#"let a = "oops"#).unwrap_err();
        assert!(error.message.contains("unterminated text"), "{error}");
    }
}
