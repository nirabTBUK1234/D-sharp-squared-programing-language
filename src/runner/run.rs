use std::{
    collections::HashMap,
    error::Error as StdError,
    fs,
    io::{self, BufRead, Write},
    rc::Rc,
};

use crate::error::Error;
use crate::parser::ast::{BinaryOp, Callee, Expr, Parameter, Stmt, StmtKind, UnaryOp, VarKind};
use crate::parser::parsers::Parser;
use crate::value::{SetItem, Value};

const CONSOLE_METHODS: &str = "console.debug, console.print, console.println, console.input, console.clear, console.warn, console.error";

pub fn run_file(path: &str) -> Result<(), Box<dyn StdError>> {
    let contents = fs::read_to_string(path)?;
    run(&contents);
    Ok(())
}

/// Runs a script with a fresh environment, printing the first error it hits.
pub fn run(source: &str) {
    let mut env = Env::new();

    if let Err(error) = run_with_env(&mut env, source) {
        eprintln!("ERROR: {error}");
    }
}

/// Parses and runs `source` against an existing environment.
pub fn run_with_env(env: &mut Env, source: &str) -> Result<(), Error> {
    let statements = Parser::parse(source)?;
    Interpreter::new(env).run(&statements)
}

pub struct Env {
    scopes: Vec<HashMap<String, Variable>>,
    functions: Vec<HashMap<String, Rc<Function>>>,
    types: HashMap<String, String>,
}

struct Variable {
    kind: VarKind,
    value: Value,
}

struct Function {
    params: Vec<Parameter>,
    body: Vec<Stmt>,
}

impl Env {
    pub fn new() -> Self {
        Env {
            scopes: vec![HashMap::new()],
            functions: vec![HashMap::new()],
            types: HashMap::new(),
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.functions.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
        self.functions.pop();
    }

    pub fn define_function(&mut self, name: String, params: Vec<Parameter>, body: Vec<Stmt>) {
        let function = Rc::new(Function { params, body });
        self.functions
            .last_mut()
            .expect("there is always a scope")
            .insert(name, function);
    }

    fn get_function(&self, name: &str) -> Option<Rc<Function>> {
        self.functions
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    pub fn declare(&mut self, kind: VarKind, name: String, value: Value) -> Result<(), String> {
        let scope = self.scopes.last_mut().expect("there is always a scope");

        if let Some(existing) = scope.get(&name) {
            let existing_kind = existing.kind;

            if existing_kind.can_reassign() && kind == VarKind::Mut {
                scope.insert(name, Variable { kind, value });
                return Ok(());
            }

            return Err(format!(
                "cannot redeclare '{}' (already a {} variable)",
                name,
                existing_kind.name()
            ));
        }

        scope.insert(name, Variable { kind, value });
        Ok(())
    }

    fn get(&self, name: &str) -> Option<&Variable> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
    }

    pub fn get_value(&self, name: &str) -> Result<Value, String> {
        match self.get(name) {
            Some(variable) => Ok(variable.value.clone()),
            None => Err(format!("unknown variable '{name}'")),
        }
    }

    pub fn assign(&mut self, name: &str, value: Value) -> Result<(), String> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(variable) = scope.get_mut(name) {
                if !variable.kind.can_reassign() {
                    return Err(format!(
                        "cannot assign to {} variable '{name}' (declare it with 'mut')",
                        variable.kind.name()
                    ));
                }
                variable.value = value;
                return Ok(());
            }
        }

        Err(format!("unknown variable '{name}'"))
    }

    pub fn delete(&mut self, name: &str) -> Result<(), String> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(variable) = scope.get(name) {
                let kind = variable.kind;

                if !kind.can_delete() {
                    return Err(format!("cannot delete {} variable '{name}'", kind.name()));
                }

                scope.remove(name);
                return Ok(());
            }
        }

        Err(format!("unknown variable '{name}'"))
    }

    pub fn define_type(&mut self, name: String, target: String) -> Result<(), String> {
        if self.types.contains_key(&name) {
            return Err(format!("type '{name}' is already defined"));
        }
        self.types.insert(name, target);
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Flow {
    Normal,
    Break,
    Continue,
    Return(Value),
}

struct Interpreter<'a> {
    env: &'a mut Env,
    loop_depth: usize,
    function_depth: usize,
}

impl<'a> Interpreter<'a> {
    fn new(env: &'a mut Env) -> Self {
        Interpreter {
            env,
            loop_depth: 0,
            function_depth: 0,
        }
    }

    fn run(&mut self, statements: &[Stmt]) -> Result<(), Error> {
        self.exec_statements(statements)?;
        Ok(())
    }

    fn exec_statements(&mut self, statements: &[Stmt]) -> Result<Flow, Error> {
        for statement in statements {
            let flow = self.exec(statement)?;
            if flow != Flow::Normal {
                return Ok(flow);
            }
        }

        Ok(Flow::Normal)
    }

    fn exec_block(&mut self, statements: &[Stmt]) -> Result<Flow, Error> {
        self.env.push_scope();
        let result = self.exec_statements(statements);
        self.env.pop_scope();
        result
    }

    fn exec(&mut self, statement: &Stmt) -> Result<Flow, Error> {
        let line = statement.line;

        match &statement.kind {
            StmtKind::Declare { kind, name, value } => {
                let value = self.eval(value, line)?;
                self.env
                    .declare(*kind, name.clone(), value)
                    .map_err(|message| Error::new(line, message))?;
                Ok(Flow::Normal)
            }
            StmtKind::Assign { name, value } => {
                let value = self.eval(value, line)?;
                self.env
                    .assign(name, value)
                    .map_err(|message| Error::new(line, message))?;
                Ok(Flow::Normal)
            }
            StmtKind::Delete { name } => {
                self.env
                    .delete(name)
                    .map_err(|message| Error::new(line, message))?;
                Ok(Flow::Normal)
            }
            StmtKind::TypeAlias { name, target } => {
                self.env
                    .define_type(name.clone(), target.clone())
                    .map_err(|message| Error::new(line, message))?;
                Ok(Flow::Normal)
            }
            StmtKind::Function { name, params, body } => {
                self.env
                    .define_function(name.clone(), params.clone(), body.clone());
                Ok(Flow::Normal)
            }
            StmtKind::Return(value) => {
                if self.function_depth == 0 {
                    return Err(Error::new(line, "'return' outside of a function"));
                }

                let value = match value {
                    Some(expr) => self.eval(expr, line)?,
                    None => Value::Void,
                };

                Ok(Flow::Return(value))
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                if self.eval_condition(condition, line)? {
                    self.exec_block(then_branch)
                } else if let Some(branch) = else_branch {
                    self.exec_block(branch)
                } else {
                    Ok(Flow::Normal)
                }
            }
            StmtKind::While { condition, body } => {
                while self.eval_condition(condition, line)? {
                    match self.exec_loop_body(body)? {
                        Flow::Break => break,
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                        Flow::Continue | Flow::Normal => {}
                    }
                }
                Ok(Flow::Normal)
            }
            StmtKind::Loop { body } => {
                loop {
                    match self.exec_loop_body(body)? {
                        Flow::Break => break,
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                        Flow::Continue | Flow::Normal => {}
                    }
                }
                Ok(Flow::Normal)
            }
            StmtKind::For {
                name,
                iterable,
                body,
            } => {
                let iterable = self.eval(iterable, line)?;
                let items = iterate(&iterable, line)?;

                for item in items {
                    self.env.push_scope();
                    let declared = self.env.declare(VarKind::Let, name.clone(), item);
                    let flow = match declared {
                        Ok(()) => self.exec_loop_body(body),
                        Err(message) => Err(Error::new(line, message)),
                    };
                    self.env.pop_scope();

                    match flow? {
                        Flow::Break => break,
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                        Flow::Continue | Flow::Normal => {}
                    }
                }

                Ok(Flow::Normal)
            }
            StmtKind::Break => {
                if self.loop_depth == 0 {
                    return Err(Error::new(line, "'break' outside of a loop"));
                }
                Ok(Flow::Break)
            }
            StmtKind::Continue => {
                if self.loop_depth == 0 {
                    return Err(Error::new(line, "'continue' outside of a loop"));
                }
                Ok(Flow::Continue)
            }
            StmtKind::Block(statements) => self.exec_block(statements),
            StmtKind::Expression(expr) => {
                self.eval(expr, line)?;
                Ok(Flow::Normal)
            }
        }
    }

    fn exec_loop_body(&mut self, body: &[Stmt]) -> Result<Flow, Error> {
        self.loop_depth += 1;
        let result = self.exec_block(body);
        self.loop_depth -= 1;
        result
    }

    fn eval_condition(&mut self, condition: &Expr, line: usize) -> Result<bool, Error> {
        let value = self.eval(condition, line)?;
        match value {
            Value::Bool(value) => Ok(value),
            other => Err(Error::new(
                line,
                format!("condition must be a bool, got {}", other.type_name()),
            )),
        }
    }

    fn eval(&mut self, expr: &Expr, line: usize) -> Result<Value, Error> {
        match expr {
            Expr::Int(number) => Ok(Value::Int(*number)),
            Expr::Float(number) => Ok(Value::Float(*number)),
            Expr::Bool(value) => Ok(Value::Bool(*value)),
            Expr::Text(text) => Ok(Value::Text(text.clone())),
            Expr::Var(name) => self
                .env
                .get_value(name)
                .map_err(|message| Error::new(line, message)),
            Expr::HashSet(items) => {
                let mut set_items = std::collections::HashSet::new();

                for item in items {
                    let value = self.eval(item, line)?;
                    let set_item = SetItem::from_value(value)
                        .map_err(|message| Error::new(line, message))?;
                    set_items.insert(set_item);
                }

                Ok(Value::HashSet(set_items))
            }
            Expr::Matrix(rows) => {
                let mut matrix = HashMap::new();

                for (row_name, cells) in rows {
                    let mut row = HashMap::new();

                    for (cell_name, cell) in cells {
                        row.insert(cell_name.clone(), self.eval(cell, line)?);
                    }

                    matrix.insert(row_name.clone(), row);
                }

                Ok(Value::Matrix(matrix))
            }
            Expr::Unary { op, expr } => {
                let value = self.eval(expr, line)?;
                unary_op(*op, value, line)
            }
            Expr::Binary { op, left, right } => {
                if *op == BinaryOp::And {
                    let left_value = self.eval(left, line)?;
                    return match require_bool(&left_value, "and", line)? {
                        false => Ok(Value::Bool(false)),
                        true => {
                            let right_value = self.eval(right, line)?;
                            Ok(Value::Bool(require_bool(&right_value, "and", line)?))
                        }
                    };
                }

                if *op == BinaryOp::Or {
                    let left_value = self.eval(left, line)?;
                    return match require_bool(&left_value, "or", line)? {
                        true => Ok(Value::Bool(true)),
                        false => {
                            let right_value = self.eval(right, line)?;
                            Ok(Value::Bool(require_bool(&right_value, "or", line)?))
                        }
                    };
                }

                let left_value = self.eval(left, line)?;
                let right_value = self.eval(right, line)?;
                binary_op(*op, left_value, right_value, line)
            }
            Expr::Call { callee, args } => {
                let mut values = Vec::with_capacity(args.len());

                for arg in args {
                    values.push(self.eval(arg, line)?);
                }

                match callee {
                    Callee::Method { receiver, method } => {
                        call_builtin(receiver, method, &values, line)
                    }
                    Callee::Function(name) => self.call_function(name, values, line),
                }
            }
        }
    }

    fn call_function(
        &mut self,
        name: &str,
        values: Vec<Value>,
        line: usize,
    ) -> Result<Value, Error> {
        let Some(function) = self.env.get_function(name) else {
            return Err(Error::new(line, format!("unknown function '{name}'")));
        };

        let required = function
            .params
            .iter()
            .filter(|param| param.default.is_none())
            .count();
        let maximum = function.params.len();

        if values.len() < required || values.len() > maximum {
            let expected = if required == maximum {
                format!("{required}")
            } else {
                format!("between {required} and {maximum}")
            };

            return Err(Error::new(
                line,
                format!(
                    "function '{name}' takes {expected} argument(s), but {} given",
                    values.len()
                ),
            ));
        }

        self.env.push_scope();

        let result = match self.bind_parameters(&function, values, line) {
            Ok(()) => {
                let loop_depth = std::mem::replace(&mut self.loop_depth, 0);
                self.function_depth += 1;

                let flow = self.exec_statements(&function.body);

                self.loop_depth = loop_depth;
                self.function_depth -= 1;

                match flow? {
                    Flow::Return(value) => Ok(value),
                    Flow::Normal => Ok(Value::Void),
                    Flow::Break | Flow::Continue => Err(Error::new(
                        line,
                        "'break' or 'continue' escaped a function body",
                    )),
                }
            }
            Err(error) => Err(error),
        };

        self.env.pop_scope();
        result
    }

    fn bind_parameters(
        &mut self,
        function: &Function,
        values: Vec<Value>,
        line: usize,
    ) -> Result<(), Error> {
        let mut values = values.into_iter();

        for param in &function.params {
            let value = match values.next() {
                Some(value) => value,
                None => match &param.default {
                    Some(default) => self.eval(default, line)?,
                    None => {
                        return Err(Error::new(
                            line,
                            format!("missing argument '{}'", param.name),
                        ));
                    }
                },
            };

            self.env
                .declare(VarKind::Let, param.name.clone(), value)
                .map_err(|message| Error::new(line, message))?;
        }

        Ok(())
    }
}

fn call_builtin(receiver: &str, method: &str, args: &[Value], line: usize) -> Result<Value, Error> {
    match receiver {
        "console" => call_console(method, args, line),
        _ => Err(Error::new(
            line,
            format!("unknown call target '{receiver}' (only 'console' is implemented so far)"),
        )),
    }
}

fn call_console(method: &str, args: &[Value], line: usize) -> Result<Value, Error> {
    let expected = match method {
        "input" | "clear" => 0,
        "debug" | "print" | "println" | "warn" | "error" => 1,
        _ => {
            return Err(Error::new(
                line,
                format!(
                    "unknown console method 'console.{method}' (available: {CONSOLE_METHODS})"
                ),
            ));
        }
    };

    if args.len() != expected {
        return Err(Error::new(
            line,
            format!(
                "console.{method}() takes {expected} argument{}, but {} given",
                if expected == 1 { "" } else { "s" },
                args.len()
            ),
        ));
    }

    match method {
        "clear" => {
            clear_screen(line)?;
            Ok(Value::Void)
        }
        "input" => read_input(line),
        _ => {
            let value = &args[0];

            match method {
                "println" => println!("{value}"),
                "print" => {
                    print!("{value}");
                    io::stdout()
                        .flush()
                        .map_err(|e| Error::new(line, format!("couldn't flush stdout: {e}")))?;
                }
                "debug" => eprintln!("[debug] {value}"),
                "warn" | "error" => eprintln!("{value}"),
                _ => unreachable!("the method list above and these arms are kept in sync"),
            }

            Ok(Value::Void)
        }
    }
}

fn read_input(line: usize) -> Result<Value, Error> {
    let mut buffer = String::new();

    let bytes = io::stdin()
        .lock()
        .read_line(&mut buffer)
        .map_err(|e| Error::new(line, format!("couldn't read input: {e}")))?;

    if bytes == 0 {
        return Err(Error::new(
            line,
            "console.input() reached the end of the input",
        ));
    }

    let text = buffer.trim_end_matches(['\n', '\r']).to_string();
    Ok(Value::Text(text))
}

fn clear_screen(line: usize) -> Result<(), Error> {
    // builtin_functions.json maps this to `terminal::clear()`; without the
    // `terminal` crate we use the standard ANSI clear-and-home sequence.
    print!("\x1b[2J\x1b[H");
    io::stdout()
        .flush()
        .map_err(|e| Error::new(line, format!("couldn't clear the screen: {e}")))
}

fn require_bool(value: &Value, context: &str, line: usize) -> Result<bool, Error> {
    match value {
        Value::Bool(value) => Ok(*value),
        other => Err(Error::new(
            line,
            format!("'{context}' needs a bool, got {}", other.type_name()),
        )),
    }
}

fn unary_op(op: UnaryOp, value: Value, line: usize) -> Result<Value, Error> {
    match op {
        UnaryOp::Not => Ok(Value::Bool(!require_bool(&value, "not", line)?)),
        UnaryOp::Neg => match value {
            Value::Int(number) => number
                .checked_neg()
                .map(Value::Int)
                .ok_or_else(|| Error::new(line, "integer overflow")),
            Value::Float(number) => Ok(Value::Float(-number)),
            other => Err(Error::new(
                line,
                format!("cannot negate a {}", other.type_name()),
            )),
        },
    }
}

fn binary_op(op: BinaryOp, left: Value, right: Value, line: usize) -> Result<Value, Error> {
    match op {
        BinaryOp::Is => Ok(Value::Bool(left == right)),
        BinaryOp::IsNot => Ok(Value::Bool(left != right)),
        BinaryOp::Greater | BinaryOp::GreaterEqual | BinaryOp::Less | BinaryOp::LessEqual => {
            compare(op, &left, &right, line)
        }
        BinaryOp::Add => add(left, right, line),
        BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide | BinaryOp::Remainder => {
            arithmetic(op, left, right, line)
        }
        BinaryOp::And | BinaryOp::Or => unreachable!("and/or are evaluated with short-circuiting"),
    }
}

fn add(left: Value, right: Value, line: usize) -> Result<Value, Error> {
    match (&left, &right) {
        (Value::Int(a), Value::Int(b)) => a
            .checked_add(*b)
            .map(Value::Int)
            .ok_or_else(|| Error::new(line, "integer overflow")),
        (Value::Text(a), Value::Text(b)) => Ok(Value::Text(format!("{a}{b}"))),
        _ => match float_pair(&left, &right) {
            Some((a, b)) => Ok(Value::Float(a + b)),
            None => Err(type_error("add", &left, &right, line)),
        },
    }
}

fn arithmetic(op: BinaryOp, left: Value, right: Value, line: usize) -> Result<Value, Error> {
    if let (Value::Int(a), Value::Int(b)) = (&left, &right) {
        let (a, b) = (*a, *b);

        let result = match op {
            BinaryOp::Subtract => a.checked_sub(b),
            BinaryOp::Multiply => a.checked_mul(b),
            BinaryOp::Divide => {
                if b == 0 {
                    return Err(Error::new(line, "division by zero"));
                }
                Some(a / b)
            }
            BinaryOp::Remainder => {
                if b == 0 {
                    return Err(Error::new(line, "division by zero"));
                }
                Some(a % b)
            }
            _ => unreachable!("only arithmetic operations reach here"),
        };

        return result.map(Value::Int).ok_or_else(|| Error::new(line, "integer overflow"));
    }

    let Some((a, b)) = float_pair(&left, &right) else {
        return Err(type_error(op.name(), &left, &right, line));
    };

    let value = match op {
        BinaryOp::Subtract => a - b,
        BinaryOp::Multiply => a * b,
        BinaryOp::Divide => {
            if b == 0.0 {
                return Err(Error::new(line, "division by zero"));
            }
            a / b
        }
        BinaryOp::Remainder => {
            if b == 0.0 {
                return Err(Error::new(line, "division by zero"));
            }
            a % b
        }
        _ => unreachable!("only arithmetic operations reach here"),
    };

    Ok(Value::Float(value))
}

fn compare(op: BinaryOp, left: &Value, right: &Value, line: usize) -> Result<Value, Error> {
    let ordering = match (left, right) {
        (Value::Int(a), Value::Int(b)) => a.partial_cmp(b),
        (Value::Text(a), Value::Text(b)) => a.partial_cmp(b),
        _ => match float_pair(left, right) {
            Some((a, b)) => a.partial_cmp(&b),
            None => return Err(type_error("compare", left, right, line)),
        },
    };

    let Some(ordering) = ordering else {
        return Ok(Value::Bool(false));
    };

    let result = match op {
        BinaryOp::Greater => ordering.is_gt(),
        BinaryOp::GreaterEqual => ordering.is_ge(),
        BinaryOp::Less => ordering.is_lt(),
        BinaryOp::LessEqual => ordering.is_le(),
        _ => unreachable!("only comparisons reach here"),
    };

    Ok(Value::Bool(result))
}

impl BinaryOp {
    fn name(self) -> &'static str {
        match self {
            BinaryOp::Or => "or",
            BinaryOp::And => "and",
            BinaryOp::Is => "is",
            BinaryOp::IsNot => "is_not",
            BinaryOp::Greater => "greater",
            BinaryOp::GreaterEqual => "greater_equal",
            BinaryOp::Less => "less",
            BinaryOp::LessEqual => "less_equal",
            BinaryOp::Add => "add",
            BinaryOp::Subtract => "subtract",
            BinaryOp::Multiply => "multiply",
            BinaryOp::Divide => "divide",
            BinaryOp::Remainder => "remainder",
        }
    }
}

fn float_pair(left: &Value, right: &Value) -> Option<(f64, f64)> {
    let left = match left {
        Value::Int(number) => *number as f64,
        Value::Float(number) => *number,
        _ => return None,
    };

    let right = match right {
        Value::Int(number) => *number as f64,
        Value::Float(number) => *number,
        _ => return None,
    };

    Some((left, right))
}

fn type_error(action: &str, left: &Value, right: &Value, line: usize) -> Error {
    Error::new(
        line,
        format!(
            "cannot {action} {} and {}",
            left.type_name(),
            right.type_name()
        ),
    )
}

fn iterate(value: &Value, line: usize) -> Result<Vec<Value>, Error> {
    match value {
        Value::HashSet(items) => Ok(items.iter().map(set_item_to_value).collect()),
        Value::Matrix(rows) => {
            let mut names: Vec<String> = rows.keys().cloned().collect();
            names.sort();
            Ok(names.into_iter().map(Value::Text).collect())
        }
        Value::Text(text) => Ok(text.chars().map(|c| Value::Text(c.to_string())).collect()),
        other => Err(Error::new(
            line,
            format!(
                "cannot loop over a {} (use a hashset, matrix, or text)",
                other.type_name()
            ),
        )),
    }
}

fn set_item_to_value(item: &SetItem) -> Value {
    match item {
        SetItem::Int(number) => Value::Int(*number),
        SetItem::Float(bits) => Value::Float(f64::from_bits(*bits)),
        SetItem::Bool(value) => Value::Bool(*value),
        SetItem::Text(text) => Value::Text(text.clone()),
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

        // A full block can be typed across several lines; keep reading until the
        // braces balance, then run the whole input as one program.
        let mut source = input;
        while !input_is_complete(&source) {
            print!("  ");
            io::stdout()
                .flush()
                .map_err(|e| format!("couldn't print the prompt: {e}"))?;

            let Some(next) = read_repl_line()? else {
                break;
            };
            source.push_str(&next);
        }

        if let Err(error) = run_with_env(&mut env, &source) {
            eprintln!("ERROR: {error}");
        }
    }

    Ok(())
}

fn input_is_complete(source: &str) -> bool {
    let mut depth: i64 = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in source.chars() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' => quote = Some(ch),
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
    }

    // An open text literal also means the user is not done typing.
    depth <= 0 && quote.is_none()
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

    fn env_after(source: &str) -> Env {
        let mut env = Env::new();
        run_with_env(&mut env, source).unwrap();
        env
    }

    fn error_of(source: &str) -> Error {
        let mut env = Env::new();
        run_with_env(&mut env, source).unwrap_err()
    }

    #[test]
    fn let_mut_const_static_del_and_type() {
        let env = env_after(
            "let x = 1\n\
             let mut y = 2\n\
             mut y = 3\n\
             const C = 4\n\
             static S = 5\n\
             type Id = int\n\
             del x",
        );

        assert!(env.get_value("x").is_err());
        assert_eq!(env.get_value("y").unwrap(), Value::Int(3));
        assert_eq!(env.get_value("C").unwrap(), Value::Int(4));
        assert_eq!(env.get_value("S").unwrap(), Value::Int(5));
        assert_eq!(env.types.get("Id").map(String::as_str), Some("int"));
    }

    #[test]
    fn redeclare_and_delete_rules() {
        let mut env = Env::new();
        run_with_env(&mut env, "let y = 1").unwrap();

        let error = run_with_env(&mut env, "let y = 2").unwrap_err();
        assert!(error.message.contains("cannot redeclare"), "{error}");

        run_with_env(&mut env, "mut z = 1\nmut z = 2").unwrap();
        assert_eq!(env.get_value("z").unwrap(), Value::Int(2));

        let error = run_with_env(&mut env, "const C = 1\ndel C").unwrap_err();
        assert!(error.message.contains("cannot delete"), "{error}");
    }

    #[test]
    fn while_loop_accumulates() {
        let env = env_after(
            "let mut sum = 0\n\
             let mut i = 0\n\
             while i less 5 {\n\
             \x20   sum = sum + i\n\
             \x20   i = i + 1\n\
             }",
        );

        assert_eq!(env.get_value("sum").unwrap(), Value::Int(10));
        assert_eq!(env.get_value("i").unwrap(), Value::Int(5));
    }

    #[test]
    fn loop_break_and_continue() {
        let env = env_after(
            "let mut total = 0\n\
             let mut i = 0\n\
             loop {\n\
             \x20   i = i + 1\n\
             \x20   if i is 3 { continue }\n\
             \x20   if i greater 5 { break }\n\
             \x20   total = total + i\n\
             }",
        );

        // 1 + 2 + 4 + 5, skipping 3, stopping after 5.
        assert_eq!(env.get_value("total").unwrap(), Value::Int(12));
    }

    #[test]
    fn for_loops_over_text_and_sets() {
        let env = env_after(
            "let mut letters = 0\n\
             for ch in \"abc\" {\n\
             \x20   letters = letters + 1\n\
             }\n\
             let mut seen = 0\n\
             for n in hashset 7 {\n\
             \x20   seen = n\n\
             }",
        );

        assert_eq!(env.get_value("letters").unwrap(), Value::Int(3));
        assert_eq!(env.get_value("seen").unwrap(), Value::Int(7));
    }

    #[test]
    fn blocks_have_their_own_scope() {
        let env = env_after(
            "let x = 1\n\
             if true {\n\
             \x20   let x = 2\n\
             \x20   console.println(x)\n\
             }\n\
             {\n\
             \x20   let y = 3\n\
             }",
        );

        assert_eq!(env.get_value("x").unwrap(), Value::Int(1));
        assert!(env.get_value("y").is_err());
    }

    #[test]
    fn if_else_picks_one_branch() {
        let env = env_after(
            "let mut label = \"\"\n\
             let score = 7\n\
             if score greater 5 {\n\
             \x20   label = \"high\"\n\
             } else {\n\
             \x20   label = \"low\"\n\
             }",
        );

        assert_eq!(env.get_value("label").unwrap(), Value::Text("high".into()));
    }

    #[test]
    fn operators_and_precedence() {
        let env = env_after(
            "let arithmetic = 1 + 2 * 3\n\
             let compare = 1 greater 2\n\
             let logic = true and not false\n\
             let text = \"a\" + \"b\"",
        );

        assert_eq!(env.get_value("arithmetic").unwrap(), Value::Int(7));
        assert_eq!(env.get_value("compare").unwrap(), Value::Bool(false));
        assert_eq!(env.get_value("logic").unwrap(), Value::Bool(true));
        assert_eq!(env.get_value("text").unwrap(), Value::Text("ab".into()));
    }

    #[test]
    fn assignment_to_const_is_rejected() {
        let error = error_of("const C = 1\nC = 2");
        assert!(error.message.contains("cannot assign to const"), "{error}");
        assert_eq!(error.line, 2);
    }

    #[test]
    fn break_outside_loop_is_an_error() {
        let error = error_of("break");
        assert!(error.message.contains("outside of a loop"), "{error}");
        assert_eq!(error.line, 1);
    }

    #[test]
    fn division_by_zero_reports_the_line() {
        let error = error_of("let a = 1\nlet b = a / 0");
        assert!(error.message.contains("division by zero"), "{error}");
        assert_eq!(error.line, 2);
    }

    #[test]
    fn hashset_and_matrix_still_work() {
        let env = env_after(
            "let nums = hashset 1 2 2\n\
             let grid = matrix row1 { a 1 b 2 } row2 { a 3 }",
        );

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
    fn functions_support_parameters_defaults_and_recursion() {
        let env = env_after(
            "def fact(n) {\n\
             \x20   if n less_equal 1 { return 1 }\n\
             \x20   return n * fact(n - 1)\n\
             }\n\
             def greet(name, greeting = \"Hello\") {\n\
             \x20   return greeting + \", \" + name\n\
             }\n\
             let result = fact(5)\n\
             let message = greet(\"Nirab\")\n\
             let short = greet(\"Nirab\", \"Hi\")",
        );

        assert_eq!(env.get_value("result").unwrap(), Value::Int(120));
        assert_eq!(
            env.get_value("message").unwrap(),
            Value::Text("Hello, Nirab".into())
        );
        assert_eq!(
            env.get_value("short").unwrap(),
            Value::Text("Hi, Nirab".into())
        );
    }

    #[test]
    fn fn_is_an_alias_for_def() {
        let env = env_after("fn double(x) { return x * 2 }\nlet value = double(21)");
        assert_eq!(env.get_value("value").unwrap(), Value::Int(42));
    }

    #[test]
    fn return_leaves_loops_and_blocks() {
        let env = env_after(
            "def first_match(needle) {\n\
             \x20   for ch in \"abcdef\" {\n\
             \x20       if ch is needle { return ch }\n\
             \x20   }\n\
             \x20   return \"none\"\n\
             }\n\
             let found = first_match(\"c\")\n\
             let missing = first_match(\"z\")",
        );

        assert_eq!(env.get_value("found").unwrap(), Value::Text("c".into()));
        assert_eq!(
            env.get_value("missing").unwrap(),
            Value::Text("none".into())
        );
    }

    #[test]
    fn function_locals_and_definitions_are_scoped() {
        let env = env_after(
            "def outer() {\n\
             \x20   let inner = 1\n\
             \x20   return inner\n\
             }\n\
             let ok = outer()\n\
             if true {\n\
             \x20   def hidden() { return 2 }\n\
             }",
        );

        assert_eq!(env.get_value("ok").unwrap(), Value::Int(1));
        assert!(env.get_value("inner").is_err());
        assert!(env.get_function("hidden").is_none());
    }

    #[test]
    fn function_errors_point_at_the_call() {
        let error = error_of("def f(a, b) { return a }\n\nf(1)");
        assert!(error.message.contains("takes 2 argument(s)"), "{error}");
        assert_eq!(error.line, 3);

        let error = error_of("nope(1)");
        assert!(error.message.contains("unknown function 'nope'"), "{error}");

        let error = error_of("return 1");
        assert!(error.message.contains("'return' outside"), "{error}");
    }

    #[test]
    fn void_is_returned_without_a_return_statement() {
        let env = env_after("def do_nothing() { }\nlet result = do_nothing()");
        assert_eq!(env.get_value("result").unwrap(), Value::Void);
    }

    #[test]
    fn repl_environment_survives_between_inputs() {
        let mut env = Env::new();

        run_with_env(&mut env, "let counter = 1").unwrap();
        run_with_env(&mut env, "let copy = counter").unwrap();
        run_with_env(&mut env, "const LIMIT = 10").unwrap();

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

    #[test]
    fn incomplete_input_waits_for_more() {
        assert!(!input_is_complete("if true {"));
        assert!(!input_is_complete("if true {\n    let a = 1\n"));
        assert!(input_is_complete("if true {\n    let a = 1\n}"));
        assert!(input_is_complete("let a = 1"));
        // Braces and quotes inside text must not start a continuation.
        assert!(input_is_complete(r#"console.println("{")"#));
        assert!(!input_is_complete(r#"let a = "unfinished"#));
    }
}
