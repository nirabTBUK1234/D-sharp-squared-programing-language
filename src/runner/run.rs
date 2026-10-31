use std::{
    collections::HashMap,
    error::Error,
    fs,
    io::{self, BufRead, Write},
};

use crate::parser::parsers::{
    ConsoleCall, is_name, parse_assignment, parse_console_call, parse_name_and_eq_value,
    parse_prompt, parse_value,
};
use crate::value::Value;

const CONSOLE_METHODS: &str = "console.debug, console.print, console.println, console.input, console.clear, console.warn, console.error";

pub fn run_file(path: &str) -> Result<(), Box<dyn Error>> {
    match fs::read_to_string(path) {
        Err(msg) => return Err(msg.to_string().into()),
        Ok(contents) => Ok(run(&contents)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum VarKind {
    Let,
    Mut,
    Const,
    Static,
}

impl VarKind {
    fn name(self) -> &'static str {
        match self {
            VarKind::Let => "let",
            VarKind::Mut => "mut",
            VarKind::Const => "const",
            VarKind::Static => "static",
        }
    }

    fn can_reassign(self) -> bool {
        matches!(self, VarKind::Mut)
    }

    fn can_delete(self) -> bool {
        matches!(self, VarKind::Let | VarKind::Mut)
    }
}

struct Variable {
    kind: VarKind,
    value: Value,
}

pub struct Env {
    variables: HashMap<String, Variable>,
    types: HashMap<String, String>,
}

impl Env {
    pub(crate) fn new() -> Self {
        Env {
            variables: HashMap::new(),
            types: HashMap::new(),
        }
    }

    fn declare(&mut self, kind: VarKind, name: String, value: Value) -> Result<(), String> {
        if let Some(existing) = self.variables.get(&name) {
            if existing.kind.can_reassign() && kind == VarKind::Mut {
                self.variables.insert(name, Variable { kind, value });
                return Ok(());
            }
            return Err(format!(
                "cannot redeclare '{}' (already a {} variable)",
                name,
                existing.kind.name()
            ));
        }

        self.variables.insert(name, Variable { kind, value });
        Ok(())
    }

    pub(crate) fn get_value(&self, name: &str) -> Result<Value, String> {
        match self.variables.get(name) {
            Some(variable) => Ok(variable.value.clone()),
            None => Err(format!("unknown variable '{name}'")),
        }
    }

    fn delete(&mut self, name: &str) -> Result<(), String> {
        match self.variables.get(name) {
            None => Err(format!("unknown variable '{name}'")),
            Some(variable) if !variable.kind.can_delete() => Err(format!(
                "cannot delete {} variable '{name}'",
                variable.kind.name()
            )),
            Some(_) => {
                self.variables.remove(name);
                Ok(())
            }
        }
    }

    fn define_type(&mut self, name: String, target: String) -> Result<(), String> {
        if self.types.contains_key(&name) {
            return Err(format!("type '{name}' is already defined"));
        }
        self.types.insert(name, target);
        Ok(())
    }
}

fn run_variable_line(env: &mut Env, line: &[String]) -> Result<(), String> {
    let keyword = line[0].as_str();

    match keyword {
        "let" => {
            let rest = &line[1..];
            let (kind, rest) = if rest.first().map(|s| s.as_str()) == Some("mut") {
                (VarKind::Mut, &rest[1..])
            } else {
                (VarKind::Let, rest)
            };
            let (name, value) = parse_assignment(rest, env)?;
            env.declare(kind, name, value)
        }
        "mut" => {
            let (name, value) = parse_assignment(&line[1..], env)?;
            env.declare(VarKind::Mut, name, value)
        }
        "const" => {
            let (name, value) = parse_assignment(&line[1..], env)?;
            env.declare(VarKind::Const, name, value)
        }
        "static" => {
            let (name, value) = parse_assignment(&line[1..], env)?;
            env.declare(VarKind::Static, name, value)
        }
        "del" => {
            if line.len() != 2 {
                return Err("expected: del name".to_string());
            }
            env.delete(&line[1])
        }
        "type" => {
            let (name, target) = parse_name_and_eq_value(&line[1..])?;
            if !is_name(target) {
                return Err(format!("'{target}' is not a valid type name"));
            }
            env.define_type(name, target.to_string())
        }
        _ => Err(format!(
            "unknown statement '{keyword}'"
        )),
    }
}

/// Runs one `console.<method>(...)` call.
fn run_console_call(env: &Env, call: &ConsoleCall) -> Result<(), String> {
    let method = call.method.as_str();
    let name = format!("console.{method}");

    let expected_args = match method {
        "input" | "clear" => 0,
        "debug" | "print" | "println" | "warn" | "error" => 1,
        _ => {
            return Err(format!(
                "unknown console method '{name}' (available: {CONSOLE_METHODS})"
            ));
        }
    };

    if call.args.len() != expected_args {
        return Err(format!(
            "{name}() takes {expected_args} argument{}, but {} given",
            if expected_args == 1 { "" } else { "s" },
            call.args.len()
        ));
    }

    match method {
        "clear" => return clear_screen(),
        "input" => {
            // A bare `console.input()` reads one line and throws it away, which
            // makes it usable as a "press Enter to continue" pause. Use
            // `let answer = console.input()` to keep the text.
            return read_input().map(|_| ());
        }
        _ => {}
    }

    let value = parse_value(&call.args[0], env)?;

    match method {
        "println" => println!("{value}"),
        "print" => {
            print!("{value}");
            io::stdout()
                .flush()
                .map_err(|e| format!("couldn't flush stdout: {e}"))?;
        }
        "debug" => eprintln!("[debug] {value}"),
        "warn" => eprintln!("{value}"),
        "error" => eprintln!("{value}"),
        _ => unreachable!("the method list above and these arms are kept in sync"),
    }

    Ok(())
}

/// Reads one line from stdin for `console.input()`.
pub(crate) fn read_input() -> Result<Value, String> {
    let mut buffer = String::new();

    let bytes = io::stdin()
        .lock()
        .read_line(&mut buffer)
        .map_err(|e| format!("couldn't read input: {e}"))?;

    if bytes == 0 {
        return Err("console.input() reached the end of the input".to_string());
    }

    let text = buffer.trim_end_matches(['\n', '\r']).to_string();
    Ok(Value::Text(text))
}

fn clear_screen() -> Result<(), String> {
    // builtin_functions.json maps this to `terminal::clear()`; without the
    // `terminal` crate we use the standard ANSI clear-and-home sequence.
    print!("\x1b[2J\x1b[H");
    io::stdout()
        .flush()
        .map_err(|e| format!("couldn't clear the screen: {e}"))
}

pub fn run(contents: &str) {
    let mut env = Env::new();
    run_source(&mut env, contents);
}

/// Runs a whole script against an existing environment.
fn run_source(env: &mut Env, contents: &str) {
    for (line_number, line) in parse_prompt(contents) {
        if line.is_empty() {
            continue;
        }

        if let Err(msg) = run_line(env, &line) {
            eprintln!("ERROR: line {line_number}: {msg}");
        }
    }
}

/// Runs one already-tokenised statement.
fn run_line(env: &mut Env, line: &[String]) -> Result<(), String> {
    match parse_console_call(line) {
        Ok(Some(call)) => run_console_call(env, &call),
        Ok(None) => run_variable_line(env, line),
        Err(msg) => Err(msg),
    }
}

pub fn run_prompt() -> Result<(), String> {
    // One environment for the whole session, so variables survive between
    // inputs. Type `quit` (or `exit`) or press Ctrl-D to leave.
    let mut env = Env::new();

    loop {
        print!("> ");
        io::stdout()
            .flush()
            .map_err(|e| format!("couldn't print the prompt: {e}"))?;

        let Some(input) = read_repl_line()? else {
            break;
        };

        let command = input.trim();

        if is_quit_command(command) {
            break;
        }
        if command.is_empty() {
            continue;
        }

        for (_, line) in parse_prompt(&input) {
            if line.is_empty() {
                continue;
            }
            if let Err(msg) = run_line(&mut env, &line) {
                // Each input is its own buffer, so a line number would just say
                // "line 1" every time; report the error as-is instead.
                eprintln!("ERROR: {msg}");
            }
        }
    }

    Ok(())
}

fn is_quit_command(command: &str) -> bool {
    let command = command.trim();
    command.eq_ignore_ascii_case("quit") || command.eq_ignore_ascii_case("exit")
}

/// Reads one line for the REPL. `Ok(None)` means end of input (Ctrl-D).
fn read_repl_line() -> Result<Option<String>, String> {
    let mut buffer = String::new();

    let bytes = io::stdin()
        .lock()
        .read_line(&mut buffer)
        .map_err(|e| format!("couldn't read input: {e}"))?;

    if bytes == 0 {
        return Ok(None);
    }

    Ok(Some(buffer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::SetItem;

    fn words(line: &[&str]) -> Vec<String> {
        line.iter().map(|word| word.to_string()).collect()
    }

    #[test]
    fn let_mut_const_static_del_and_type() {
        let mut env = Env::new();

        run_variable_line(&mut env, &words(&["let", "x", "=", "1"])).unwrap();
        run_variable_line(&mut env, &words(&["let", "mut", "y", "=", "2"])).unwrap();
        run_variable_line(&mut env, &words(&["mut", "y", "=", "3"])).unwrap();
        run_variable_line(&mut env, &words(&["const", "C", "=", "4"])).unwrap();
        run_variable_line(&mut env, &words(&["static", "S", "=", "5"])).unwrap();
        run_variable_line(&mut env, &words(&["type", "Id", "=", "int"])).unwrap();
        run_variable_line(&mut env, &words(&["del", "x"])).unwrap();

        assert!(env.get_value("x").is_err());
        assert_eq!(env.get_value("y").unwrap(), Value::Int(3));
        assert_eq!(env.get_value("C").unwrap(), Value::Int(4));
        assert_eq!(env.get_value("S").unwrap(), Value::Int(5));
        assert_eq!(env.types.get("Id").map(String::as_str), Some("int"));

        assert!(run_variable_line(&mut env, &words(&["let", "y", "=", "0"])).is_err());
        assert!(run_variable_line(&mut env, &words(&["del", "C"])).is_err());
        assert!(run_variable_line(&mut env, &words(&["del", "S"])).is_err());
    }

    #[test]
    fn hashset_and_matrix() {
        let mut env = Env::new();

        run_variable_line(&mut env, &words(&["let", "nums", "=", "hashset", "1", "2", "2"])).unwrap();
        run_variable_line(
            &mut env,
            &words(&[
                "let", "grid", "=", "matrix", "row1", "{", "a", "1", "b", "2", "}", "row2", "{",
                "a", "3", "}",
            ]),
        )
        .unwrap();

        match env.get_value("nums").unwrap() {
            Value::HashSet(items) => {
                assert!(items.contains(&SetItem::Int(1)));
                assert!(items.contains(&SetItem::Int(2)));
                assert_eq!(items.len(), 2);
            }
            other => panic!("expected hashset, got {other:?}"),
        }

        match env.get_value("grid").unwrap() {
            Value::Matrix(rows) => {
                assert_eq!(rows["row1"]["a"], Value::Int(1));
                assert_eq!(rows["row1"]["b"], Value::Int(2));
                assert_eq!(rows["row2"]["a"], Value::Int(3));
            }
            other => panic!("expected matrix, got {other:?}"),
        }
    }

    #[test]
    fn repl_environment_survives_between_inputs() {
        let mut env = Env::new();

        // Each of these stands in for one line typed at the prompt.
        run_source(&mut env, "let counter = 1");
        run_source(&mut env, "let copy = counter");
        run_source(&mut env, "const LIMIT = 10");

        assert_eq!(env.get_value("counter").unwrap(), Value::Int(1));
        assert_eq!(env.get_value("copy").unwrap(), Value::Int(1));
        assert_eq!(env.get_value("LIMIT").unwrap(), Value::Int(10));
    }

    #[test]
    fn quit_command_is_recognised() {
        assert!(is_quit_command("quit"));
        assert!(is_quit_command("  Quit  "));
        assert!(is_quit_command("exit"));
        assert!(!is_quit_command("quit()"));
        assert!(!is_quit_command(""));
    }
}
