use std::{
    collections::{HashMap, HashSet},
    error::Error as StdError,
    fs,
    io::{self, BufRead, Write},
    rc::Rc,
};

use crate::error::Error;
use crate::parser::ast::{BinaryOp, Callee, Expr, Parameter, Stmt, StmtKind, UnaryOp};
use crate::parser::parsers::Parser;
use crate::value::{SetItem, StructValue, Value};

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

/// Python-style scoping: a scope is created per function call. Blocks
/// (`if`, loops, `{ ... }`) do not introduce scopes, so names assigned in
/// them stay visible afterwards.
pub struct Env {
    scopes: Vec<HashMap<String, Value>>,
    functions: Vec<HashMap<String, Rc<Function>>>,
    structs: Vec<HashMap<String, StructDef>>,
}

struct Function {
    params: Vec<Parameter>,
    body: Vec<Stmt>,
}

#[derive(Clone)]
struct StructDef {
    fields: Vec<FieldDef>,
    methods: HashMap<String, Rc<Function>>,
}

#[derive(Clone)]
struct FieldDef {
    name: String,
    default: Option<Expr>,
}

impl Env {
    pub fn new() -> Self {
        Env {
            scopes: vec![HashMap::new()],
            functions: vec![HashMap::new()],
            structs: vec![HashMap::new()],
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.functions.push(HashMap::new());
        self.structs.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
        self.functions.pop();
        self.structs.pop();
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

    fn define_struct(&mut self, name: String, definition: StructDef) {
        self.structs
            .last_mut()
            .expect("there is always a scope")
            .insert(name, definition);
    }

    fn get_struct(&self, name: &str) -> Option<StructDef> {
        self.structs
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    fn add_method(
        &mut self,
        struct_name: &str,
        method_name: String,
        function: Rc<Function>,
    ) -> Result<(), String> {
        for scope in self.structs.iter_mut().rev() {
            if let Some(definition) = scope.get_mut(struct_name) {
                definition.methods.insert(method_name, function);
                return Ok(());
            }
        }

        Err(format!("unknown struct '{struct_name}'"))
    }

    fn define(&mut self, name: String, value: Value) {
        self.scopes
            .last_mut()
            .expect("there is always a scope")
            .insert(name, value);
    }

    pub fn get_value(&self, name: &str) -> Result<Value, String> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
            .ok_or_else(|| format!("unknown variable '{name}'"))
    }

    /// `name = value`: updates the nearest existing binding, or creates one in
    /// the current scope.
    pub fn assign(&mut self, name: &str, value: Value) {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return;
            }
        }

        self.define(name.to_string(), value);
    }

    pub fn delete(&mut self, name: &str) -> Result<(), String> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.remove(name).is_some() {
                return Ok(());
            }
        }

        Err(format!("unknown variable '{name}'"))
    }

    /// `name[key] = value`
    pub fn assign_index(&mut self, name: &str, key: SetItem, value: Value) -> Result<(), String> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(existing) = scope.get_mut(name) {
                return match existing {
                    Value::Map(entries) => {
                        entries.insert(key, value);
                        Ok(())
                    }
                    other => Err(format!(
                        "cannot assign to a lookup on '{}': it is a {}, not a hashmap",
                        name,
                        other.type_name()
                    )),
                };
            }
        }

        Err(format!("unknown variable '{name}'"))
    }

    /// `name.field = value`
    pub fn assign_field(&mut self, name: &str, field: &str, value: Value) -> Result<(), String> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(existing) = scope.get_mut(name) {
                return match existing {
                    Value::Struct(struct_value) => {
                        if !struct_value.fields.contains_key(field) {
                            return Err(format!(
                                "struct '{}' has no field '{field}'",
                                struct_value.type_name
                            ));
                        }
                        struct_value.fields.insert(field.to_string(), value);
                        Ok(())
                    }
                    other => Err(format!(
                        "cannot assign field '{field}' on '{}': it is a {}, not a struct",
                        name,
                        other.type_name()
                    )),
                };
            }
        }

        Err(format!("unknown variable '{name}'"))
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

    fn exec(&mut self, statement: &Stmt) -> Result<Flow, Error> {
        let line = statement.line;

        match &statement.kind {
            StmtKind::Assign { name, value } => {
                let value = self.eval(value, line)?;
                self.env.assign(name, value);
                Ok(Flow::Normal)
            }
            StmtKind::IndexAssign { name, index, value } => {
                let value = self.eval(value, line)?;
                let index = self.eval(index, line)?;
                let key = map_key(index, line)?;
                self.env
                    .assign_index(name, key, value)
                    .map_err(|message| Error::new(line, message))?;
                Ok(Flow::Normal)
            }
            StmtKind::FieldAssign { name, field, value } => {
                let value = self.eval(value, line)?;
                self.env
                    .assign_field(name, field, value)
                    .map_err(|message| Error::new(line, message))?;
                Ok(Flow::Normal)
            }
            StmtKind::Struct { name, fields } => {
                let definition = StructDef {
                    fields: fields
                        .iter()
                        .map(|field| FieldDef {
                            name: field.name.clone(),
                            default: field.default.clone(),
                        })
                        .collect(),
                    methods: HashMap::new(),
                };
                self.env.define_struct(name.clone(), definition);
                Ok(Flow::Normal)
            }
            StmtKind::Impl { name, methods } => {
                for method in methods {
                    let function = Rc::new(Function {
                        params: method.params.clone(),
                        body: method.body.clone(),
                    });
                    self.env
                        .add_method(name, method.name.clone(), function)
                        .map_err(|message| Error::new(line, message))?;
                }
                Ok(Flow::Normal)
            }
            StmtKind::Delete { name } => {
                self.env
                    .delete(name)
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
                    None => Value::None,
                };

                Ok(Flow::Return(value))
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                if self.eval_condition(condition, line)? {
                    self.exec_statements(then_branch)
                } else if let Some(branch) = else_branch {
                    self.exec_statements(branch)
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
                    self.env.assign(name, item);

                    match self.exec_loop_body(body)? {
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
            StmtKind::Block(statements) => self.exec_statements(statements),
            StmtKind::Expression(expr) => {
                self.eval(expr, line)?;
                Ok(Flow::Normal)
            }
        }
    }

    fn exec_loop_body(&mut self, body: &[Stmt]) -> Result<Flow, Error> {
        self.loop_depth += 1;
        let result = self.exec_statements(body);
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
            Expr::None => Ok(Value::None),
            Expr::Text(text) => Ok(Value::Text(text.clone())),
            Expr::Var(name) => self
                .env
                .get_value(name)
                .map_err(|message| Error::new(line, message)),
            Expr::Map(entries) => {
                let mut map = HashMap::new();

                for (key, value) in entries {
                    let key = map_key(self.eval(key, line)?, line)?;
                    let value = self.eval(value, line)?;
                    map.insert(key, value);
                }

                Ok(Value::Map(map))
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
            Expr::Index { object, index } => {
                let object = self.eval(object, line)?;
                let index = self.eval(index, line)?;
                index_value(object, index, line)
            }
            Expr::Field { object, name } => {
                let object = self.eval(object, line)?;

                match object {
                    Value::Struct(struct_value) => struct_value
                        .fields
                        .get(name)
                        .cloned()
                        .ok_or_else(|| {
                            Error::new(
                                line,
                                format!(
                                    "struct '{}' has no field '{name}'",
                                    struct_value.type_name
                                ),
                            )
                        }),
                    other => Err(Error::new(
                        line,
                        format!("cannot read field '{name}' from a {}", other.type_name()),
                    )),
                }
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
                        if let Expr::Var(name) = receiver.as_ref()
                            && name == "console"
                            && self.env.get_value(name).is_err()
                        {
                            return call_builtin(name, method, &values, line);
                        }

                        let receiver_name = match receiver.as_ref() {
                            Expr::Var(name) => Some(name.clone()),
                            _ => None,
                        };
                        let receiver = self.eval(receiver, line)?;
                        self.call_method(
                            receiver,
                            receiver_name.as_deref(),
                            method,
                            values,
                            line,
                        )
                    }
                    Callee::Function(name) => self.call_named(name, values, line),
                }
            }
        }
    }

    /// Calls a user-defined function, or a Python-style builtin such as
    /// `print(...)` / `input(...)` when no function of that name exists.
    fn call_named(&mut self, name: &str, values: Vec<Value>, line: usize) -> Result<Value, Error> {
        if self.env.get_function(name).is_some() {
            return self.call_function(name, values, line);
        }

        if let Some(definition) = self.env.get_struct(name) {
            return self.construct_struct(name, &definition, values, line);
        }

        match name {
            "hashSet" | "hashset" | "set" => {
                let mut items = HashSet::new();

                for value in values {
                    let item = SetItem::from_value(value)
                        .map_err(|message| Error::new(line, message))?;
                    items.insert(item);
                }

                Ok(Value::HashSet(items))
            }
            "print" => {
                let parts: Vec<String> = values.iter().map(ToString::to_string).collect();
                println!("{}", parts.join(" "));
                Ok(Value::None)
            }
            "input" => {
                if !values.is_empty() {
                    return Err(Error::new(line, "input() takes no arguments"));
                }
                read_input(line)
            }
            _ => Err(Error::new(line, format!("unknown function '{name}'"))),
        }
    }

    fn call_function(
        &mut self,
        name: &str,
        values: Vec<Value>,
        line: usize,
    ) -> Result<Value, Error> {
        let function = self
            .env
            .get_function(name)
            .expect("call_named checked that the function exists");

        let label = format!("function '{name}'");
        self.call_function_object(&label, &function, values, line)
            .map(|(value, _)| value)
    }

    fn call_method(
        &mut self,
        receiver: Value,
        receiver_name: Option<&str>,
        method: &str,
        mut values: Vec<Value>,
        line: usize,
    ) -> Result<Value, Error> {
        let receiver_type = receiver.type_name().to_string();

        let Value::Struct(struct_value) = receiver else {
            return Err(Error::new(
                line,
                format!(
                    "cannot call method '{method}' on a {receiver_type} (only structs have methods)"
                ),
            ));
        };

        let definition = self
            .env
            .get_struct(&struct_value.type_name)
            .ok_or_else(|| {
                Error::new(
                    line,
                    format!("unknown struct '{}'", struct_value.type_name),
                )
            })?;

        let function = definition.methods.get(method).cloned().ok_or_else(|| {
            Error::new(
                line,
                format!(
                    "struct '{}' has no method '{method}'",
                    struct_value.type_name
                ),
            )
        })?;

        let label = format!("method '{}.{method}'", struct_value.type_name);
        values.insert(0, Value::Struct(struct_value));
        let (result, final_self) = self.call_function_object(&label, &function, values, line)?;

        // Python-style methods mutate the receiver, even though ordinary
        // assignment copies values: write the final `self` back into the
        // variable that was used for the call.
        if let (Some(name), Some(self_value)) = (receiver_name, final_self) {
            self.env.assign(name, self_value);
        }

        Ok(result)
    }

    fn construct_struct(
        &mut self,
        name: &str,
        definition: &StructDef,
        values: Vec<Value>,
        line: usize,
    ) -> Result<Value, Error> {
        let required = definition
            .fields
            .iter()
            .filter(|field| field.default.is_none())
            .count();
        let maximum = definition.fields.len();

        if values.len() < required || values.len() > maximum {
            let expected = if required == maximum {
                format!("{required}")
            } else {
                format!("between {required} and {maximum}")
            };

            return Err(Error::new(
                line,
                format!(
                    "struct '{name}' takes {expected} argument(s), but {} given",
                    values.len()
                ),
            ));
        }

        let mut fields = HashMap::new();
        let mut values = values.into_iter();

        for field in &definition.fields {
            let value = match values.next() {
                Some(value) => value,
                None => match &field.default {
                    Some(default) => self.eval(default, line)?,
                    None => {
                        return Err(Error::new(
                            line,
                            format!("missing field '{}' for struct '{name}'", field.name),
                        ));
                    }
                },
            };

            fields.insert(field.name.clone(), value);
        }

        Ok(Value::Struct(StructValue {
            type_name: name.to_string(),
            fields,
        }))
    }

    fn call_function_object(
        &mut self,
        label: &str,
        function: &Function,
        values: Vec<Value>,
        line: usize,
    ) -> Result<(Value, Option<Value>), Error> {
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
                    "{label} takes {expected} argument(s), but {} given",
                    values.len()
                ),
            ));
        }

        self.env.push_scope();

        let result = match self.bind_parameters(function, values, line) {
            Ok(()) => {
                let loop_depth = std::mem::replace(&mut self.loop_depth, 0);
                self.function_depth += 1;

                let flow = self.exec_statements(&function.body);

                self.loop_depth = loop_depth;
                self.function_depth -= 1;

                match flow? {
                    Flow::Return(value) => Ok(value),
                    Flow::Normal => Ok(Value::None),
                    Flow::Break | Flow::Continue => Err(Error::new(
                        line,
                        "'break' or 'continue' escaped a function body",
                    )),
                }
            }
            Err(error) => Err(error),
        };

        let final_self = self.env.get_value("self").ok();
        self.env.pop_scope();
        result.map(|value| (value, final_self))
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

            self.env.define(param.name.clone(), value);
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
            Ok(Value::None)
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

            Ok(Value::None)
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
        return Err(Error::new(line, "input() reached the end of the input"));
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
        BinaryOp::Equal => Ok(Value::Bool(equal(&left, &right))),
        BinaryOp::NotEqual => Ok(Value::Bool(!equal(&left, &right))),
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

/// Python compares `1 == 1.0` as equal even though they are different types.
fn equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
        (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
        _ => left == right,
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

        return result
            .map(Value::Int)
            .ok_or_else(|| Error::new(line, "integer overflow"));
    }

    let action = match op {
        BinaryOp::Subtract => "subtract",
        BinaryOp::Multiply => "multiply",
        BinaryOp::Divide => "divide",
        BinaryOp::Remainder => "take the remainder of",
        _ => unreachable!("only arithmetic operations reach here"),
    };

    let Some((a, b)) = float_pair(&left, &right) else {
        return Err(type_error(action, &left, &right, line));
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
        Value::Map(entries) => {
            let mut keys: Vec<&SetItem> = entries.keys().collect();
            keys.sort_by_key(|key| key.to_string());
            Ok(keys.into_iter().map(set_item_to_value).collect())
        }
        Value::Matrix(rows) => {
            let mut names: Vec<String> = rows.keys().cloned().collect();
            names.sort();
            Ok(names.into_iter().map(Value::Text).collect())
        }
        Value::Text(text) => Ok(text.chars().map(|c| Value::Text(c.to_string())).collect()),
        other => Err(Error::new(
            line,
            format!(
                "cannot loop over a {} (use a hashset, hashmap, matrix, or text)",
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

fn map_key(value: Value, line: usize) -> Result<SetItem, Error> {
    let type_name = value.type_name().to_string();

    match SetItem::from_value(value) {
        Ok(key) => Ok(key),
        Err(_) => Err(Error::new(
            line,
            format!("hashmap keys must be int, float, bool, or text, got {type_name}"),
        )),
    }
}

fn index_value(object: Value, index: Value, line: usize) -> Result<Value, Error> {
    match object {
        Value::Map(entries) => {
            let key = map_key(index, line)?;
            entries
                .get(&key)
                .cloned()
                .ok_or_else(|| Error::new(line, format!("hashmap key {key} not found")))
        }
        Value::Text(text) => {
            let type_name = index.type_name().to_string();
            let Value::Int(position) = index else {
                return Err(Error::new(
                    line,
                    format!("text index must be an int, got {type_name}"),
                ));
            };

            let chars: Vec<char> = text.chars().collect();
            let resolved = if position < 0 {
                chars.len() as i64 + position
            } else {
                position
            };

            if resolved < 0 || resolved as usize >= chars.len() {
                return Err(Error::new(
                    line,
                    format!(
                        "text index {position} is out of range (length {})",
                        chars.len()
                    ),
                ));
            }

            Ok(Value::Text(chars[resolved as usize].to_string()))
        }
        other => Err(Error::new(
            line,
            format!(
                "cannot index a {} (only hashmap and text support [ ])",
                other.type_name()
            ),
        )),
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
    fn variables_are_created_by_assignment() {
        let env = env_after("x = 1\nx = x + 2\ny = x * 10");
        assert_eq!(env.get_value("x").unwrap(), Value::Int(3));
        assert_eq!(env.get_value("y").unwrap(), Value::Int(30));
    }

    #[test]
    fn unknown_variables_report_the_line() {
        let error = error_of("a = 1\nb = missing + 1");
        assert!(error.message.contains("unknown variable 'missing'"), "{error}");
        assert_eq!(error.line, 2);
    }

    #[test]
    fn del_removes_a_name() {
        let env = env_after("x = 1\ndel x");
        assert!(env.get_value("x").is_err());

        let error = error_of("del nope");
        assert!(error.message.contains("unknown variable 'nope'"), "{error}");
    }

    #[test]
    fn while_loop_accumulates() {
        let env = env_after(
            "sum = 0\n\
             i = 0\n\
             while i < 5 {\n\
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
            "total = 0\n\
             i = 0\n\
             loop {\n\
             \x20   i = i + 1\n\
             \x20   if i == 3 { continue }\n\
             \x20   if i > 5 { break }\n\
             \x20   total = total + i\n\
             }",
        );

        // 1 + 2 + 4 + 5, skipping 3, stopping after 5.
        assert_eq!(env.get_value("total").unwrap(), Value::Int(12));
    }

    #[test]
    fn for_loops_over_text_and_sets() {
        let env = env_after(
            "letters = 0\n\
             for ch in \"abc\" {\n\
             \x20   letters = letters + 1\n\
             }\n\
             seen = 0\n\
             for n in hashSet(7) {\n\
             \x20   seen = n\n\
             }",
        );

        assert_eq!(env.get_value("letters").unwrap(), Value::Int(3));
        assert_eq!(env.get_value("seen").unwrap(), Value::Int(7));
    }

    #[test]
    fn blocks_do_not_create_scopes() {
        let env = env_after(
            "x = 1\n\
             if True {\n\
             \x20   x = 2\n\
             \x20   inside = 3\n\
             }\n\
             for n in \"ab\" { }\n\
             {\n\
             \x20   grouped = 4\n\
             }",
        );

        assert_eq!(env.get_value("x").unwrap(), Value::Int(2));
        assert_eq!(env.get_value("inside").unwrap(), Value::Int(3));
        assert_eq!(env.get_value("n").unwrap(), Value::Text("b".into()));
        assert_eq!(env.get_value("grouped").unwrap(), Value::Int(4));
    }

    #[test]
    fn if_elif_else_picks_one_branch() {
        let env = env_after(
            "def label(score) {\n\
             \x20   if score >= 90 {\n\
             \x20       return \"A\"\n\
             \x20   }\n\
             \x20   elif score >= 80 {\n\
             \x20       return \"B\"\n\
             \x20   } else {\n\
             \x20       return \"C\"\n\
             \x20   }\n\
             }\n\
             a = label(95)\n\
             b = label(85)\n\
             c = label(10)",
        );

        assert_eq!(env.get_value("a").unwrap(), Value::Text("A".into()));
        assert_eq!(env.get_value("b").unwrap(), Value::Text("B".into()));
        assert_eq!(env.get_value("c").unwrap(), Value::Text("C".into()));
    }

    #[test]
    fn else_if_is_a_nested_condition() {
        let env = env_after(
            "score = 85\n\
             grade = \"\"\n\
             if score >= 90 {\n\
             \x20   grade = \"A\"\n\
             } else if score >= 80 {\n\
             \x20   grade = \"B\"\n\
             } else {\n\
             \x20   grade = \"C\"\n\
             }",
        );

        assert_eq!(env.get_value("grade").unwrap(), Value::Text("B".into()));
    }

    #[test]
    fn operators_and_precedence() {
        let env = env_after(
            "arithmetic = 1 + 2 * 3\n\
             compare = 1 > 2\n\
             logic = True and not False\n\
             text = \"a\" + \"b\"\n\
             mixed = 1 == 1.0\n\
             different = 1 != \"1\"",
        );

        assert_eq!(env.get_value("arithmetic").unwrap(), Value::Int(7));
        assert_eq!(env.get_value("compare").unwrap(), Value::Bool(false));
        assert_eq!(env.get_value("logic").unwrap(), Value::Bool(true));
        assert_eq!(env.get_value("text").unwrap(), Value::Text("ab".into()));
        assert_eq!(env.get_value("mixed").unwrap(), Value::Bool(true));
        assert_eq!(env.get_value("different").unwrap(), Value::Bool(true));
    }

    #[test]
    fn none_is_a_value() {
        let env = env_after("a = None\nb = None == None\nc = None == 0");
        assert_eq!(env.get_value("a").unwrap(), Value::None);
        assert_eq!(env.get_value("b").unwrap(), Value::Bool(true));
        assert_eq!(env.get_value("c").unwrap(), Value::Bool(false));
    }

    #[test]
    fn break_outside_loop_is_an_error() {
        let error = error_of("break");
        assert!(error.message.contains("outside of a loop"), "{error}");
        assert_eq!(error.line, 1);
    }

    #[test]
    fn division_by_zero_reports_the_line() {
        let error = error_of("a = 1\nb = a / 0");
        assert!(error.message.contains("division by zero"), "{error}");
        assert_eq!(error.line, 2);
    }

    #[test]
    fn braced_matrix_literals() {
        let env = env_after(
            "grid = mat{row1: {a: 1, b: 2}, row2: {a: 3}}\n\
             nested = mat{outer: {inner: mat{x: {y: 2}}}, \"row 1\": {\"a b\": 1,},}",
        );

        match env.get_value("grid").unwrap() {
            Value::Matrix(rows) => {
                assert_eq!(rows["row1"]["a"], Value::Int(1));
                assert_eq!(rows["row1"]["b"], Value::Int(2));
                assert_eq!(rows["row2"]["a"], Value::Int(3));
            }
            other => panic!("expected matrix, got {other:?}"),
        }

        match env.get_value("nested").unwrap() {
            Value::Matrix(rows) => {
                match &rows["outer"]["inner"] {
                    Value::Matrix(inner) => assert_eq!(inner["x"]["y"], Value::Int(2)),
                    other => panic!("expected a nested matrix, got {other:?}"),
                }
                assert_eq!(rows["row 1"]["a b"], Value::Int(1));
            }
            other => panic!("expected matrix, got {other:?}"),
        }

        assert_eq!(
            env.get_value("grid").unwrap().to_string(),
            "mat{row1: {a: 1, b: 2}, row2: {a: 3}}"
        );
    }

    #[test]
    fn hashset_and_matrix_still_work() {
        let env = env_after(
            "nums = hashSet(1, 2, 2)\n\
             grid = mat{row1: {a: 1, b: 2}, row2: {a: 3}}",
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
    fn hashset_constructor_and_display() {
        let env = env_after("nums = hashSet(3, 1, 2, 2)\nempty = hashSet()\nalias = set(1)");

        match env.get_value("nums").unwrap() {
            Value::HashSet(items) => {
                assert_eq!(items.len(), 3);
                assert!(items.contains(&SetItem::Int(1)));
                assert!(items.contains(&SetItem::Int(2)));
                assert!(items.contains(&SetItem::Int(3)));
            }
            other => panic!("expected hashset, got {other:?}"),
        }

        assert_eq!(
            env.get_value("nums").unwrap().to_string(),
            "hashSet(1, 2, 3)"
        );
        assert_eq!(env.get_value("empty").unwrap().to_string(), "hashSet()");
        assert_eq!(env.get_value("alias").unwrap().to_string(), "hashSet(1)");
    }

    #[test]
    fn hashmaps_support_literals_indexing_and_assignment() {
        let env = env_after(
            "scores = {\"a\": 1, \"b\": 2}\n\
             first = scores[\"a\"]\n\
             scores[\"c\"] = 3\n\
             scores[\"b\"] = scores[\"b\"] + 10\n\
             nested = {\"inner\": {\"x\": 1}}\n\
             x = nested[\"inner\"][\"x\"]\n\
             empty = {}",
        );

        assert_eq!(env.get_value("first").unwrap(), Value::Int(1));
        assert_eq!(env.get_value("x").unwrap(), Value::Int(1));
        assert_eq!(env.get_value("empty").unwrap().to_string(), "{}");

        match env.get_value("scores").unwrap() {
            Value::Map(entries) => {
                assert_eq!(entries.len(), 3);
                assert_eq!(entries[&SetItem::Text("a".into())], Value::Int(1));
                assert_eq!(entries[&SetItem::Text("b".into())], Value::Int(12));
                assert_eq!(entries[&SetItem::Text("c".into())], Value::Int(3));
            }
            other => panic!("expected hashmap, got {other:?}"),
        }

        assert_eq!(
            env.get_value("scores").unwrap().to_string(),
            "{\"a\": 1, \"b\": 12, \"c\": 3}"
        );
    }

    #[test]
    fn hashmap_keys_can_be_scalars() {
        let env = env_after("m = {1: \"one\", True: \"yes\", 1.5: \"float\"}");

        match env.get_value("m").unwrap() {
            Value::Map(entries) => {
                assert_eq!(entries[&SetItem::Int(1)], Value::Text("one".into()));
                assert_eq!(entries[&SetItem::Bool(true)], Value::Text("yes".into()));
                assert_eq!(
                    entries[&SetItem::Float(1.5f64.to_bits())],
                    Value::Text("float".into())
                );
            }
            other => panic!("expected hashmap, got {other:?}"),
        }
    }

    #[test]
    fn text_supports_indexing() {
        let env = env_after("word = \"abc\"\nfirst = word[0]\nlast = word[-1]");
        assert_eq!(env.get_value("first").unwrap(), Value::Text("a".into()));
        assert_eq!(env.get_value("last").unwrap(), Value::Text("c".into()));
    }

    #[test]
    fn for_loops_over_hashmap_keys() {
        let env = env_after(
            "d = {\"a\": 1, \"b\": 2}\n\
             keys = \"\"\n\
             for k in d {\n\
             \x20   keys = keys + k\n\
             }",
        );

        assert_eq!(env.get_value("keys").unwrap(), Value::Text("ab".into()));
    }

    #[test]
    fn hashmap_errors_report_the_line() {
        let error = error_of("d = {\"a\": 1}\nmissing = d[\"b\"]");
        assert!(error.message.contains("not found"), "{error}");
        assert_eq!(error.line, 2);

        let error = error_of("x = 1\nx[\"a\"] = 2");
        assert!(error.message.contains("not a hashmap"), "{error}");
        assert_eq!(error.line, 2);

        let error = error_of("d = {None: 1}");
        assert!(error.message.contains("hashmap keys must be"), "{error}");

        let error = error_of("x = 1\ny = x[0]");
        assert!(error.message.contains("cannot index a int"), "{error}");
        assert_eq!(error.line, 2);
    }

    #[test]
    fn functions_support_parameters_defaults_and_recursion() {
        let env = env_after(
            "def fact(n) {\n\
             \x20   if n <= 1 { return 1 }\n\
             \x20   return n * fact(n - 1)\n\
             }\n\
             def greet(name, greeting = \"Hello\") {\n\
             \x20   return greeting + \", \" + name\n\
             }\n\
             result = fact(5)\n\
             message = greet(\"Nirab\")\n\
             short = greet(\"Nirab\", \"Hi\")",
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
    fn return_leaves_loops_and_blocks() {
        let env = env_after(
            "def first_match(needle) {\n\
             \x20   for ch in \"abcdef\" {\n\
             \x20       if ch == needle { return ch }\n\
             \x20   }\n\
             \x20   return \"none\"\n\
             }\n\
             found = first_match(\"c\")\n\
             missing = first_match(\"z\")",
        );

        assert_eq!(env.get_value("found").unwrap(), Value::Text("c".into()));
        assert_eq!(
            env.get_value("missing").unwrap(),
            Value::Text("none".into())
        );
    }

    #[test]
    fn function_locals_stay_inside_the_call() {
        let env = env_after(
            "def outer() {\n\
             \x20   inner = 1\n\
             \x20   return inner\n\
             }\n\
             ok = outer()",
        );

        assert_eq!(env.get_value("ok").unwrap(), Value::Int(1));
        assert!(env.get_value("inner").is_err());
    }

    #[test]
    fn definitions_inside_blocks_survive() {
        let env = env_after("if True {\n    def later() { return 2 }\n}\nv = later()");
        assert_eq!(env.get_value("v").unwrap(), Value::Int(2));
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
    fn functions_return_none_without_a_return_statement() {
        let env = env_after("def do_nothing() { }\nresult = do_nothing()");
        assert_eq!(env.get_value("result").unwrap(), Value::None);
    }

    #[test]
    fn print_returns_none() {
        let env = env_after("result = print(\"hello\", 1)");
        assert_eq!(env.get_value("result").unwrap(), Value::None);
    }

    #[test]
    fn repl_environment_survives_between_inputs() {
        let mut env = Env::new();

        run_with_env(&mut env, "counter = 1").unwrap();
        run_with_env(&mut env, "copy = counter").unwrap();
        run_with_env(&mut env, "counter = counter + 1").unwrap();

        assert_eq!(env.get_value("counter").unwrap(), Value::Int(2));
        assert_eq!(env.get_value("copy").unwrap(), Value::Int(1));
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
        assert!(!input_is_complete("if True {"));
        assert!(!input_is_complete("if True {\n    a = 1\n"));
        assert!(input_is_complete("if True {\n    a = 1\n}"));
        assert!(input_is_complete("a = 1"));
        // Braces and quotes inside text must not start a continuation.
        assert!(input_is_complete(r#"print("{")"#));
        assert!(!input_is_complete(r#"a = "unfinished"#));
    }

    #[test]
    fn structs_have_fields_defaults_and_methods() {
        let env = env_after(
            "struct Point {\n\
             x = 0\n\
             y = 0\n\
             }\n\
             impl Point {\n\
             def sum(self) {\n\
             return self.x + self.y\n\
             }\n\
             def shift(self, dx, dy) {\n\
             self.x = self.x + dx\n\
             self.y = self.y + dy\n\
             return self\n\
             }\n\
             }\n\
             p = Point(1, 2)\n\
             total = p.sum()\n\
             p.x = 10\n\
             q = Point()\n\
             p.shift(1, 2)\n\
             after = p.sum()",
        );

        assert_eq!(env.get_value("total").unwrap(), Value::Int(3));
        assert_eq!(env.get_value("after").unwrap(), Value::Int(15));
        assert_eq!(env.get_value("p").unwrap().to_string(), "Point(x=11, y=4)");
        assert_eq!(env.get_value("q").unwrap().to_string(), "Point(x=0, y=0)");
    }

    #[test]
    fn struct_and_method_errors_report_the_line() {
        let error = error_of("struct P { x }\np = P(1)\np.nope()");
        assert!(error.message.contains("has no method 'nope'"), "{error}");
        assert_eq!(error.line, 3);

        let error = error_of("struct P { x }\np = P(1)\np.nope");
        assert!(error.message.contains("has no field 'nope'"), "{error}");
        assert_eq!(error.line, 3);

        let error = error_of("x = 1\nx.foo()");
        assert!(
            error.message.contains("cannot call method 'foo' on a int"),
            "{error}"
        );
        assert_eq!(error.line, 2);

        let error = error_of("struct P { x }\nP()");
        assert!(error.message.contains("takes 1 argument(s)"), "{error}");
        assert_eq!(error.line, 2);

        let error = error_of("struct P { x }\np = P(1)\np.y = 2");
        assert!(error.message.contains("has no field 'y'"), "{error}");
        assert_eq!(error.line, 3);

        let error = error_of("impl Missing { def m(self) { } }");
        assert!(error.message.contains("unknown struct 'Missing'"), "{error}");
    }

    #[test]
    fn macros_expand_statements() {
        let env = env_after(
            "macro unless($cond, $body) {\n\
             if not ($cond) {\n\
             $body\n\
             }\n\
             }\n\
             result = 0\n\
             unless(False, { result = 1 })\n\
             unless(True, { result = 2 })",
        );

        assert_eq!(env.get_value("result").unwrap(), Value::Int(1));
    }

    #[test]
    fn macros_are_textual_and_can_repeat_code() {
        let env = env_after(
            "counter = 0\n\
             def bump() { counter = counter + 1 }\n\
             macro twice($body) {\n\
             $body\n\
             $body\n\
             }\n\
             twice({ bump() })",
        );

        assert_eq!(env.get_value("counter").unwrap(), Value::Int(2));
    }

    #[test]
    fn structs_work_inside_functions_and_collections() {
        let env = env_after(
            "struct Pair { left right }\n\
             def make(a, b) { return Pair(a, b) }\n\
             pair = make(\"x\", 2)\n\
             texts = {\"pair\": pair}\n\
             inner = texts[\"pair\"].left",
        );

        assert_eq!(env.get_value("inner").unwrap(), Value::Text("x".into()));
        assert_eq!(
            env.get_value("pair").unwrap().to_string(),
            "Pair(left=\"x\", right=2)"
        );
    }
}
