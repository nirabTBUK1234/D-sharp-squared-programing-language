use std::collections::{HashMap, HashSet};

use crate::runner::run::{Env, read_input};
use crate::value::{SetItem, Value};

/// A parsed `console.<method>(...)` call. The arguments are still raw tokens;
/// the runner turns them into values so that lookups and I/O stay in one place.
#[derive(Clone, Debug, PartialEq)]
pub struct ConsoleCall {
    pub method: String,
    pub args: Vec<String>,
}

pub fn parse_line(line: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut current_word = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in line.chars() {
        // Everything inside a quoted text literal is part of the same word,
        // spaces and `;` included.
        if let Some(open) = quote {
            current_word.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' && open == '"' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
            continue;
        }

        match ch {
            // `;` and Newline ends the line, unless inside text.
            ';' | '\n' => break,
            '"' | '\'' => {
                quote = Some(ch);
                current_word.push(ch);
            }
            ' ' | '\t' => {
                // Finished a word. Skip extra spaces.
                if !current_word.is_empty() {
                    words.push(current_word);
                    current_word = String::new();
                }
            }
            _ => current_word.push(ch),
        }
    }

    // The last word has no space after it, so save it too.
    if !current_word.is_empty() {
        words.push(current_word);
    }

    words
}

/// Splits one source line into statements on `;`, ignoring `;` inside text.
fn split_statements(line: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for (idx, ch) in line.char_indices() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' && open == '"' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' => quote = Some(ch),
            ';' => {
                parts.push(&line[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }

    parts.push(&line[start..]);
    parts
}

/// Returns every statement with the source line it came from, so errors can
/// point the user at the right place.
pub fn parse_prompt(prompt: &str) -> Vec<(usize, Vec<String>)> {
    let mut parsed_prompt: Vec<(usize, Vec<String>)> = Vec::new();

    for (idx, line) in prompt.lines().enumerate() {
        let line_number = idx + 1;
        for statement in split_statements(line) {
            if statement.trim().is_empty() {
                continue;
            }
            let words = parse_line(statement);
            if !words.is_empty() {
                parsed_prompt.push((line_number, words));
            }
        }
    }

    parsed_prompt
}

/// Recognises `console.<method>(args...)`.
///
/// `Ok(None)` means "this is not a console call", `Err` means it looks like one
/// but is malformed.
pub fn parse_console_call(line: &[String]) -> Result<Option<ConsoleCall>, String> {
    let text = line.join(" ");

    let Some(rest) = text.strip_prefix("console.") else {
        if text == "console" || text.starts_with("console ") {
            return Err("console is not a value: use console.<method>(...)".to_string());
        }
        return Ok(None);
    };

    let Some(open) = rest.find('(') else {
        return Err(format!(
            "expected '(' after the console method name, got 'console.{rest}'"
        ));
    };

    let method = &rest[..open];
    if method.is_empty() || !is_name(method) {
        return Err(format!("'console.{method}' is not a valid method name"));
    }

    let Some(close) = rest.rfind(')') else {
        return Err(format!(
            "console.{method}(...) is missing its closing ')'"
        ));
    };

    if close != rest.len() - 1 {
        return Err(format!(
            "unexpected text after console.{method}(...): '{}'",
            &rest[close + 1..].trim()
        ));
    }

    let mut args: Vec<String> = Vec::new();
    let args_text = &rest[open + 1..close];
    if !args_text.trim().is_empty() {
        for arg in split_args(args_text) {
            let arg = arg.trim();
            if arg.is_empty() {
                return Err(format!("console.{method}() has an empty argument"));
            }
            if !is_single_value(arg) {
                return Err(format!(
                    "console.{method}() expects one value per argument, got '{arg}'"
                ));
            }
            args.push(arg.to_string());
        }
    }

    Ok(Some(ConsoleCall {
        method: method.to_string(),
        args,
    }))
}

/// Splits an argument list on commas that are outside text literals, so
/// `console.println("Hello, world!")` stays a single argument.
fn split_args(text: &str) -> Vec<&str> {
    let mut args: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for (idx, ch) in text.char_indices() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' && open == '"' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' => quote = Some(ch),
            ',' => {
                args.push(&text[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }

    args.push(&text[start..]);
    args
}

fn is_single_value(text: &str) -> bool {
    if text.len() >= 2 {
        let bytes = text.as_bytes();
        if (bytes[0] == b'"' || bytes[0] == b'\'') && bytes[bytes.len() - 1] == bytes[0] {
            return true;
        }
    }
    !text.chars().any(char::is_whitespace)
}

fn find_val_in_compound_data_type<I, T>(compound_data_type: I, target: T) -> Vec<usize>
where
    I: IntoIterator,
    I::Item: PartialEq<T>,
    T: Copy,
{
    let mut indices: Vec<usize> = Vec::new();

    for (idx, val) in compound_data_type.into_iter().enumerate() {
        if val == target {
            indices.push(idx);
        }
    }

    indices
}

pub(crate) fn is_name(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub(crate) fn parse_value(token: &str, env: &Env) -> Result<Value, String> {
    if token == "true" {
        return Ok(Value::Bool(true));
    }
    if token == "false" {
        return Ok(Value::Bool(false));
    }
    if token == "hashset" || token == "set" {
        return Ok(Value::HashSet(HashSet::new()));
    }
    if token == "matrix" || token == "matrex" {
        return Ok(Value::Matrix(HashMap::new()));
    }
    if token == "console.input()" {
        return read_input();
    }
    if let Ok(number) = token.parse::<i64>() {
        return Ok(Value::Int(number));
    }
    if let Ok(number) = token.parse::<f64>() {
        return Ok(Value::Float(number));
    }

    let mut chars = token.chars();
    if let Some(open) = chars.next() {
        if open == '"' || open == '\'' {
            if token.len() < 2 || !token.ends_with(open) {
                return Err(format!("unterminated text literal: {token}"));
            }
            return Ok(Value::Text(token[1..token.len() - 1].to_string()));
        }
    }

    env.get_value(token)
}

fn parse_hashset(tokens: &[String], env: &Env) -> Result<Value, String> {
    let mut items = HashSet::new();
    for token in tokens {
        let item = SetItem::from_value(parse_value(token, env)?)?;
        items.insert(item);
    }
    Ok(Value::HashSet(items))
}

fn parse_matrix(tokens: &[String], env: &Env) -> Result<Value, String> {
    let mut rows: HashMap<String, HashMap<String, Value>> = HashMap::new();
    let mut i = 0;

    while i < tokens.len() {
        let outer_key = tokens[i].clone();
        if !is_name(&outer_key) {
            return Err(format!("'{outer_key}' is not a valid matrix row name"));
        }
        i += 1;

        if tokens.get(i).map(String::as_str) != Some("{") {
            return Err("expected: matrix row { key value key value }".to_string());
        }
        i += 1;

        let mut inner: HashMap<String, Value> = HashMap::new();
        while i < tokens.len() && tokens[i] != "}" {
            let inner_key = tokens[i].clone();
            if !is_name(&inner_key) {
                return Err(format!("'{inner_key}' is not a valid matrix column name"));
            }
            i += 1;
            let Some(value_token) = tokens.get(i) else {
                return Err(format!("matrix column '{inner_key}' is missing a value"));
            };
            inner.insert(inner_key, parse_value(value_token, env)?);
            i += 1;
        }

        if tokens.get(i).map(String::as_str) != Some("}") {
            return Err("matrix row is missing a closing }".to_string());
        }
        i += 1;
        rows.insert(outer_key, inner);
    }

    Ok(Value::Matrix(rows))
}

fn parse_value_tokens(tokens: &[String], env: &Env) -> Result<Value, String> {
    if tokens.is_empty() {
        return Err("missing value".to_string());
    }

    match tokens[0].as_str() {
        "hashset" | "set" => parse_hashset(&tokens[1..], env),
        "matrix" | "matrex" => parse_matrix(&tokens[1..], env),
        _ if tokens.len() == 1 => parse_value(&tokens[0], env),
        _ => Err("expected a single value, hashset, or matrix".to_string()),
    }
}

pub fn parse_name_and_eq_value(tokens: &[String]) -> Result<(String, &str), String> {
    let eq_at = find_val_in_compound_data_type(tokens, "=");
    if tokens.len() != 3 || eq_at != [1] {
        return Err("expected: name = value".to_string());
    }

    let name = tokens[0].clone();
    if !is_name(&name) {
        return Err(format!("'{name}' is not a valid name"));
    }

    Ok((name, tokens[2].as_str()))
}

pub fn parse_assignment(tokens: &[String], env: &Env) -> Result<(String, Value), String> {
    if tokens.len() < 3 {
        return Err("expected: name = value".to_string());
    }

    let eq_at = find_val_in_compound_data_type(tokens, "=");
    if eq_at.first() != Some(&1) {
        return Err("expected: name = value".to_string());
    }

    let name = tokens[0].clone();
    if !is_name(&name) {
        return Err(format!("'{name}' is not a valid name"));
    }

    let value = parse_value_tokens(&tokens[2..], env)?;
    Ok((name, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_quoted_text_together() {
        assert_eq!(
            parse_line(r#"console.println("hello world")"#),
            vec![r#"console.println("hello world")"#.to_string()]
        );
        assert_eq!(
            parse_line(r#"let msg = "hi there"; console.println(msg)"#),
            vec!["let".to_string(), "msg".into(), "=".into(), r#""hi there""#.into()]
        );
    }

    #[test]
    fn parses_console_calls() {
        let call = parse_console_call(&parse_line("console.println(x)"))
            .unwrap()
            .unwrap();
        assert_eq!(call.method, "println");
        assert_eq!(call.args, vec!["x".to_string()]);

        let call = parse_console_call(&parse_line("console.clear()"))
            .unwrap()
            .unwrap();
        assert_eq!(call.method, "clear");
        assert!(call.args.is_empty());

        let call = parse_console_call(&parse_line(r#"console.println("hi there")"#))
            .unwrap()
            .unwrap();
        assert_eq!(call.args, vec![r#""hi there""#.to_string()]);

        // A comma inside text must not split the argument list.
        let call = parse_console_call(&parse_line(r#"console.println("Hello, world!")"#))
            .unwrap()
            .unwrap();
        assert_eq!(call.args, vec![r#""Hello, world!""#.to_string()]);

        let call = parse_console_call(&parse_line(r#"console.println("a", "b")"#))
            .unwrap()
            .unwrap();
        assert_eq!(
            call.args,
            vec![r#""a""#.to_string(), r#""b""#.to_string()]
        );

        assert!(parse_console_call(&parse_line("let x = 1")).unwrap().is_none());
        assert!(parse_console_call(&parse_line("console.println(x)")).is_ok());
        assert!(parse_console_call(&parse_line("console.println(x")).is_err());
        assert!(parse_console_call(&parse_line("console.println()")).is_ok());
    }

    #[test]
    fn reports_unterminated_text() {
        let env = Env::new();
        let err = parse_value(r#""oops"#, &env).unwrap_err();
        assert!(err.contains("unterminated text literal"), "{err}");
    }
}
