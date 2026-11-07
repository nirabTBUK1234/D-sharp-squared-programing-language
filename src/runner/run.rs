use std::{
    collections::{HashMap, HashSet},
    error::Error as StdError,
    fs,
    io::{self, BufRead, Write},
    path::Path,
    rc::Rc,
};

use crate::error::Error;
use crate::parser::ast::{
    BinaryOp, Callee, Expr, Method, Parameter, Stmt, StmtKind, TypeParameter, UnaryOp,
};
use crate::parser::parsers::Parser;
use crate::value::{ListValue, SetItem, StructValue, TypeSpec, Value, repr_value};

const CONSOLE_METHODS: &str = "console.debug, console.print, console.println, console.input, console.clear, console.warn, console.error";

pub fn run_file(path: &str) -> Result<(), Box<dyn StdError>> {
    let mut env = Env::new();

    if let Err(error) = run_file_with_env(&mut env, Path::new(path)) {
        eprintln!("ERROR: {error}");
    }

    Ok(())
}

/// Runs a script from disk against an existing environment. Imports resolve
/// relative to the script's directory, and errors carry the file name.
pub fn run_file_with_env(env: &mut Env, path: &Path) -> Result<(), Error> {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let display = canonical.display().to_string();
    let statements = Parser::parse_file(&canonical)?;
    Interpreter::with_file(env, Some(display.clone()))
        .run(&statements)
        .map_err(|error| error.with_file(display))
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
    traits: Vec<HashMap<String, TraitDef>>,
    /// Canonical paths of files that have already run. Imports are global to
    /// the environment, even when their statements appear inside a block.
    imported: HashSet<String>,
}

struct Function {
    type_params: Vec<TypeParameter>,
    params: Vec<Parameter>,
    /// Optional `-> Type` return annotation, checked on every call.
    return_type: Option<String>,
    body: Vec<Stmt>,
    /// File that defined the function, used to label runtime errors.
    file: Option<String>,
}

#[derive(Clone)]
struct StructDef {
    fields: Vec<FieldDef>,
    /// Methods from `impl Type { ... }`. These win over trait methods.
    methods: HashMap<String, Rc<Function>>,
    /// Methods supplied by traits, including defaults.
    trait_methods: HashMap<String, Rc<Function>>,
    /// Which trait supplied each trait method, for conflict messages.
    trait_sources: HashMap<String, String>,
    traits: HashSet<String>,
}

#[derive(Clone)]
struct FieldDef {
    name: String,
    default: Option<Expr>,
}

#[derive(Clone)]
struct TraitDef {
    methods: HashMap<String, TraitMethodDef>,
}

#[derive(Clone)]
struct TraitMethodDef {
    params: Vec<Parameter>,
    return_type: Option<String>,
    body: Option<Vec<Stmt>>,
    file: Option<String>,
}

impl StructDef {
    fn method(&self, name: &str) -> Option<Rc<Function>> {
        self.methods
            .get(name)
            .or_else(|| self.trait_methods.get(name))
            .cloned()
    }
}

impl Env {
    pub fn new() -> Self {
        Env {
            scopes: vec![HashMap::new()],
            functions: vec![HashMap::new()],
            structs: vec![HashMap::new()],
            traits: vec![HashMap::new()],
            imported: HashSet::new(),
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.functions.push(HashMap::new());
        self.structs.push(HashMap::new());
        self.traits.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
        self.functions.pop();
        self.structs.pop();
        self.traits.pop();
    }

    fn is_imported(&self, key: &str) -> bool {
        self.imported.contains(key)
    }

    fn mark_imported(&mut self, key: String) {
        self.imported.insert(key);
    }

    fn unmark_imported(&mut self, key: &str) {
        self.imported.remove(key);
    }

    pub fn define_function(
        &mut self,
        name: String,
        type_params: Vec<TypeParameter>,
        params: Vec<Parameter>,
        return_type: Option<String>,
        body: Vec<Stmt>,
        file: Option<String>,
    ) {
        let function = Rc::new(Function {
            type_params,
            params,
            return_type,
            body,
            file,
        });
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

    fn define_trait(&mut self, name: String, definition: TraitDef) {
        self.traits
            .last_mut()
            .expect("there is always a scope")
            .insert(name, definition);
    }

    fn get_trait(&self, name: &str) -> Option<TraitDef> {
        self.traits
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    fn add_trait_methods(
        &mut self,
        struct_name: &str,
        trait_name: &str,
        methods: Vec<(String, Rc<Function>)>,
    ) -> Result<(), String> {
        for scope in self.structs.iter_mut().rev() {
            if let Some(definition) = scope.get_mut(struct_name) {
                for (method_name, function) in methods {
                    definition.trait_methods.insert(method_name.clone(), function);
                    definition
                        .trait_sources
                        .insert(method_name, trait_name.to_string());
                }
                definition.traits.insert(trait_name.to_string());
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
    pub fn assign_index(
        &mut self,
        name: &str,
        index: Value,
        value: Value,
        line: usize,
    ) -> Result<(), String> {
        let scope_index = self
            .scopes
            .iter()
            .rposition(|scope| scope.contains_key(name))
            .ok_or_else(|| format!("unknown variable '{name}'"))?;

        let list = match self.scopes[scope_index].get(name) {
            Some(Value::List(list)) => Some(list.clone()),
            Some(Value::Map(_)) => None,
            Some(other) => {
                return Err(format!(
                    "cannot assign to a lookup on '{}': it is a {}, not a hashmap or list",
                    name,
                    other.type_name()
                ));
            }
            None => unreachable!("the scope was found by contains_key"),
        };

        if let Some(list) = list {
            let index_type = index.type_name().to_string();
            let Value::Int(position) = index else {
                return Err(format!(
                    "list index must be an int, got {index_type}"
                ));
            };

            let len = list.items.borrow().len();
            let resolved = resolve_index(position, len).ok_or_else(|| {
                format!("list index {position} is out of range (length {len})")
            })?;

            if let Some(element_type) = list.element_type() {
                self.ensure_value_matches_spec(&value, &element_type, line)?;
            }

            list.items.borrow_mut()[resolved] = value;
            return Ok(());
        }

        let Some(Value::Map(entries)) = self.scopes[scope_index].get_mut(name) else {
            unreachable!("the entry was checked as a hashmap");
        };
        let key = map_key(index, line).map_err(|error| error.message)?;
        entries.insert(key, value);
        Ok(())
    }

    pub fn assign_slice(
        &mut self,
        name: &str,
        value: Value,
        start: Option<Value>,
        stop: Option<Value>,
        step: Option<Value>,
        line: usize,
    ) -> Result<(), String> {
        let scope_index = self
            .scopes
            .iter()
            .rposition(|scope| scope.contains_key(name))
            .ok_or_else(|| format!("unknown variable '{name}'"))?;

        let list = match self.scopes[scope_index].get(name) {
            Some(Value::List(list)) => list.clone(),
            Some(other) => {
                return Err(format!(
                    "cannot slice-assign to '{}': it is a {}, not a list",
                    name,
                    other.type_name()
                ));
            }
            None => unreachable!("the scope was found by contains_key"),
        };

        let new_items = iterate(&value, line).map_err(|error| error.message)?;
        if let Some(element_type) = list.element_type() {
            for item in &new_items {
                self.ensure_value_matches_spec(item, &element_type, line)?;
            }
        }

        let len = list.items.borrow().len();
        let (start, stop, step) =
            normalize_slice(len, start, stop, step, line).map_err(|error| error.message)?;
        let mut items = list.items.borrow_mut();

        if step == 1 {
            let start = start as usize;
            let stop = stop as usize;
            items.splice(start..stop, new_items);
        } else {
            let indices = indices_from_parts(start, stop, step);
            if indices.len() != new_items.len() {
                return Err(format!(
                    "attempt to assign sequence of size {} to extended slice of size {}",
                    new_items.len(),
                    indices.len()
                ));
            }

            for (index, item) in indices.into_iter().zip(new_items) {
                items[index] = item;
            }
        }

        Ok(())
    }

    pub fn delete_index(&mut self, name: &str, index: Value, line: usize) -> Result<(), String> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(existing) = scope.get_mut(name) {
                return match existing {
                    Value::Map(entries) => {
                        let key = map_key(index, line).map_err(|error| error.message)?;
                        if entries.remove(&key).is_some() {
                            Ok(())
                        } else {
                            Err(format!("hashmap key {key} not found"))
                        }
                    }
                    Value::List(list) => {
                        let index_type = index.type_name().to_string();
                        let Value::Int(position) = index else {
                            return Err(format!(
                                "list index must be an int, got {index_type}"
                            ));
                        };

                        let len = list.items.borrow().len();
                        let resolved = resolve_index(position, len).ok_or_else(|| {
                            format!("list index {position} is out of range (length {len})")
                        })?;
                        list.items.borrow_mut().remove(resolved);
                        Ok(())
                    }
                    other => Err(format!(
                        "cannot delete a lookup on '{}': it is a {}, not a hashmap or list",
                        name,
                        other.type_name()
                    )),
                };
            }
        }

        Err(format!("unknown variable '{name}'"))
    }

    pub fn delete_slice(
        &mut self,
        name: &str,
        start: Option<Value>,
        stop: Option<Value>,
        step: Option<Value>,
        line: usize,
    ) -> Result<(), String> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(existing) = scope.get_mut(name) {
                let existing_type = existing.type_name().to_string();
                let Value::List(list) = existing else {
                    return Err(format!(
                        "cannot slice-delete from '{}': it is a {}, not a list",
                        name, existing_type
                    ));
                };

                let len = list.items.borrow().len();
                let (start, stop, step) =
                    normalize_slice(len, start, stop, step, line).map_err(|error| error.message)?;
                let mut items = list.items.borrow_mut();

                if step == 1 {
                    items.drain(start as usize..stop as usize);
                } else {
                    let mut indices = indices_from_parts(start, stop, step);
                    indices.sort_unstable();
                    for index in indices.into_iter().rev() {
                        items.remove(index);
                    }
                }

                return Ok(());
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
    /// Checks that a type specification names only known structs/traits and
    /// recursively validates generic arguments.
    fn validate_type_spec(&self, spec: &TypeSpec) -> Result<(), String> {
        match spec {
            TypeSpec::List(Some(inner)) => self.validate_type_spec(inner),
            TypeSpec::Named(name) => {
                if self.get_trait(name).is_some() || self.get_struct(name).is_some() {
                    Ok(())
                } else {
                    Err(format!("unknown type annotation '{name}'"))
                }
            }
            _ => Ok(()),
        }
    }

    /// Checks a value against a type spec. Untyped lists that match are promoted
    /// to the expected element type, so later mutation is checked too.
    fn ensure_value_matches_spec(
        &self,
        value: &Value,
        spec: &TypeSpec,
        line: usize,
    ) -> Result<(), String> {
        match spec {
            TypeSpec::Any => Ok(()),
            TypeSpec::Int => ensure_simple(value, spec, line, matches!(value, Value::Int(_))),
            TypeSpec::Float => ensure_simple(value, spec, line, matches!(value, Value::Float(_))),
            TypeSpec::Number => ensure_simple(
                value,
                spec,
                line,
                matches!(value, Value::Int(_) | Value::Float(_)),
            ),
            TypeSpec::Bool => ensure_simple(value, spec, line, matches!(value, Value::Bool(_))),
            TypeSpec::Text => ensure_simple(value, spec, line, matches!(value, Value::Text(_))),
            TypeSpec::None => ensure_simple(value, spec, line, matches!(value, Value::None)),
            TypeSpec::HashSet => {
                ensure_simple(value, spec, line, matches!(value, Value::HashSet(_)))
            }
            TypeSpec::HashMap => ensure_simple(value, spec, line, matches!(value, Value::Map(_))),
            TypeSpec::Matrix => ensure_simple(value, spec, line, matches!(value, Value::Matrix(_))),
            TypeSpec::List(None) => {
                ensure_simple(value, spec, line, matches!(value, Value::List(_)))
            }
            TypeSpec::List(Some(inner)) => {
                let Value::List(list) = value else {
                    return Err(format!("expected {spec}, got {}", value.type_name()));
                };

                self.validate_type_spec(inner)?;

                if let Some(existing) = list.element_type() {
                    self.ensure_spec_compatible(&existing, inner, line)?;
                } else {
                    let items = list.items.borrow();
                    for (index, item) in items.iter().enumerate() {
                        self.ensure_value_matches_spec(item, inner, line)
                            .map_err(|message| format!("list element at index {index}: {message}"))?;
                    }
                    drop(items);
                    if !matches!(**inner, TypeSpec::Any) {
                        list.promote((**inner).clone());
                    }
                }

                Ok(())
            }
            TypeSpec::Named(name) => {
                if self.get_trait(name).is_some() {
                    match value {
                        Value::Struct(struct_value) => {
                            let implements = self
                                .get_struct(&struct_value.type_name)
                                .is_some_and(|definition| definition.traits.contains(name));
                            if implements {
                                Ok(())
                            } else {
                                Err(format!(
                                    "expected a value implementing trait '{name}', got struct '{}'",
                                    struct_value.type_name
                                ))
                            }
                        }
                        other => Err(format!(
                            "expected a value implementing trait '{name}', got {}",
                            other.type_name()
                        )),
                    }
                } else if self.get_struct(name).is_some() {
                    match value {
                        Value::Struct(struct_value) if struct_value.type_name == *name => Ok(()),
                        other => Err(format!("expected {name}, got {}", other.type_name())),
                    }
                } else {
                    Err(format!("unknown type annotation '{name}'"))
                }
            }
        }
    }

    /// Checks that an existing element type for a typed list can satisfy the
    /// expected element type.
    fn ensure_spec_compatible(
        &self,
        existing: &TypeSpec,
        expected: &TypeSpec,
        _line: usize,
    ) -> Result<(), String> {
        if existing == expected {
            return Ok(());
        }

        match expected {
            TypeSpec::Any => Ok(()),
            TypeSpec::Number => {
                if matches!(
                    existing,
                    TypeSpec::Int | TypeSpec::Float | TypeSpec::Number
                ) {
                    Ok(())
                } else {
                    Err(format!("list is typed {existing}, expected {expected}"))
                }
            }
            TypeSpec::List(None) => {
                if matches!(existing, TypeSpec::List(_)) {
                    Ok(())
                } else {
                    Err(format!("list is typed {existing}, expected {expected}"))
                }
            }
            TypeSpec::List(Some(expected_inner)) => match existing {
                TypeSpec::List(Some(existing_inner)) => {
                    self.ensure_spec_compatible(existing_inner, expected_inner, _line)
                }
                _ => Err(format!("list is typed {existing}, expected {expected}")),
            },
            TypeSpec::Named(trait_name) if self.get_trait(trait_name).is_some() => {
                match existing {
                    TypeSpec::Named(struct_name) => {
                        let implements = self
                            .get_struct(struct_name)
                            .is_some_and(|definition| definition.traits.contains(trait_name));
                        if implements {
                            Ok(())
                        } else {
                            Err(format!("list is typed {existing}, expected {expected}"))
                        }
                    }
                    _ => Err(format!("list is typed {existing}, expected {expected}")),
                }
            }
            _ => {
                if existing == expected {
                    Ok(())
                } else {
                    Err(format!("list is typed {existing}, expected {expected}"))
                }
            }
        }
    }
}

fn ensure_simple(
    value: &Value,
    spec: &TypeSpec,
    _line: usize,
    accepted: bool,
) -> Result<(), String> {
    if accepted {
        Ok(())
    } else {
        Err(format!("expected {spec}, got {}", value.type_name()))
    }
}

fn parse_type_spec(text: &str) -> Result<TypeSpec, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("empty type annotation".to_string());
    }

    if let Some(open) = text.find('[') {
        if !text.ends_with(']') {
            return Err(format!("invalid type annotation '{text}'"));
        }

        let name = text[..open].trim();
        if !name.eq_ignore_ascii_case("list") {
            return Err(format!(
                "only List[...] type arguments are supported, got '{text}'"
            ));
        }

        let arguments = split_type_arguments(&text[open + 1..text.len() - 1])?;
        if arguments.len() != 1 {
            return Err(format!(
                "List[...] needs exactly one type argument, got {}",
                arguments.len()
            ));
        }

        let inner = parse_type_spec(arguments[0])?;
        return Ok(TypeSpec::List(Some(Box::new(inner))));
    }

    Ok(match text.to_ascii_lowercase().as_str() {
        "any" => TypeSpec::Any,
        "int" => TypeSpec::Int,
        "float" => TypeSpec::Float,
        "number" => TypeSpec::Number,
        "bool" => TypeSpec::Bool,
        "text" | "string" => TypeSpec::Text,
        "none" => TypeSpec::None,
        "list" => TypeSpec::List(None),
        "hashset" | "set" => TypeSpec::HashSet,
        "hashmap" | "map" => TypeSpec::HashMap,
        "matrix" | "mat" => TypeSpec::Matrix,
        _ => TypeSpec::Named(text.to_string()),
    })
}

fn split_type_arguments(text: &str) -> Result<Vec<&str>, String> {
    let mut arguments = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;

    for (index, character) in text.char_indices() {
        match character {
            '[' => depth += 1,
            ']' => {
                if depth == 0 {
                    return Err(format!("unmatched ']' in type annotation '{text}'"));
                }
                depth -= 1;
            }
            ',' if depth == 0 => {
                let argument = text[start..index].trim();
                if argument.is_empty() {
                    return Err(format!("empty type argument in '{text}'"));
                }
                arguments.push(argument);
                start = index + 1;
            }
            _ => {}
        }
    }

    if depth != 0 {
        return Err(format!("unmatched '[' in type annotation '{text}'"));
    }

    let last = text[start..].trim();
    if last.is_empty() {
        return Err(format!("empty type argument in '{text}'"));
    }
    arguments.push(last);
    Ok(arguments)
}

fn resolve_type_spec(spec: TypeSpec, type_params: &[TypeParameter]) -> TypeSpec {
    match spec {
        TypeSpec::Named(name) => {
            if let Some(type_param) = type_params.iter().find(|type_param| type_param.name == name) {
                match &type_param.bound {
                    Some(bound) => TypeSpec::Named(bound.clone()),
                    None => TypeSpec::Any,
                }
            } else {
                TypeSpec::Named(name)
            }
        }
        TypeSpec::List(Some(inner)) => {
            TypeSpec::List(Some(Box::new(resolve_type_spec(*inner, type_params))))
        }
        other => other,
    }
}

fn is_iterable(value: &Value) -> bool {
    matches!(
        value,
        Value::List(_)
            | Value::HashSet(_)
            | Value::Map(_)
            | Value::Matrix(_)
            | Value::Text(_)
    )
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
    current_file: Option<String>,
}

impl<'a> Interpreter<'a> {
    fn new(env: &'a mut Env) -> Self {
        Interpreter {
            env,
            loop_depth: 0,
            function_depth: 0,
            current_file: None,
        }
    }

    fn with_file(env: &'a mut Env, file: Option<String>) -> Self {
        Interpreter {
            env,
            loop_depth: 0,
            function_depth: 0,
            current_file: file,
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
                self.env
                    .assign_index(name, index, value, line)
                    .map_err(|message| Error::new(line, message))?;
                Ok(Flow::Normal)
            }
            StmtKind::SliceAssign {
                name,
                start,
                stop,
                step,
                value,
            } => {
                let value = self.eval(value, line)?;
                let start = start
                    .as_ref()
                    .map(|expr| self.eval(expr, line))
                    .transpose()?;
                let stop = stop
                    .as_ref()
                    .map(|expr| self.eval(expr, line))
                    .transpose()?;
                let step = step
                    .as_ref()
                    .map(|expr| self.eval(expr, line))
                    .transpose()?;
                self.env
                    .assign_slice(name, value, start, stop, step, line)
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
                    trait_methods: HashMap::new(),
                    trait_sources: HashMap::new(),
                    traits: HashSet::new(),
                };
                self.env.define_struct(name.clone(), definition);
                Ok(Flow::Normal)
            }
            StmtKind::Trait { name, methods } => {
                let mut definition = TraitDef {
                    methods: HashMap::new(),
                };

                for method in methods {
                    definition.methods.insert(
                        method.name.clone(),
                        TraitMethodDef {
                            params: method.params.clone(),
                            return_type: method.return_type.clone(),
                            body: method.body.clone(),
                            file: self.current_file.clone(),
                        },
                    );
                }

                self.env.define_trait(name.clone(), definition);
                Ok(Flow::Normal)
            }
            StmtKind::Impl {
                trait_name,
                name,
                methods,
            } => {
                match trait_name {
                    None => {
                        for method in methods {
                            let function = Rc::new(Function {
                                type_params: Vec::new(),
                                params: method.params.clone(),
                                return_type: method.return_type.clone(),
                                body: method.body.clone(),
                                file: self.current_file.clone(),
                            });
                            self.env
                                .add_method(name, method.name.clone(), function)
                                .map_err(|message| Error::new(line, message))?;
                        }
                    }
                    Some(trait_name) => {
                        self.implement_trait(trait_name, name, methods, line)?;
                    }
                }

                Ok(Flow::Normal)
            }
            StmtKind::Delete { name } => {
                self.env
                    .delete(name)
                    .map_err(|message| Error::new(line, message))?;
                Ok(Flow::Normal)
            }
            StmtKind::DeleteIndex { name, index } => {
                let index = self.eval(index, line)?;
                self.env
                    .delete_index(name, index, line)
                    .map_err(|message| Error::new(line, message))?;
                Ok(Flow::Normal)
            }
            StmtKind::DeleteSlice {
                name,
                start,
                stop,
                step,
            } => {
                let start = start
                    .as_ref()
                    .map(|expr| self.eval(expr, line))
                    .transpose()?;
                let stop = stop
                    .as_ref()
                    .map(|expr| self.eval(expr, line))
                    .transpose()?;
                let step = step
                    .as_ref()
                    .map(|expr| self.eval(expr, line))
                    .transpose()?;
                self.env
                    .delete_slice(name, start, stop, step, line)
                    .map_err(|message| Error::new(line, message))?;
                Ok(Flow::Normal)
            }
            StmtKind::Function {
                name,
                type_params,
                params,
                return_type,
                body,
            } => {
                self.env.define_function(
                    name.clone(),
                    type_params.clone(),
                    params.clone(),
                    return_type.clone(),
                    body.clone(),
                    self.current_file.clone(),
                );
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
                names,
                iterable,
                body,
            } => {
                let iterable = self.eval(iterable, line)?;
                let rows: Vec<Vec<Value>> = if names.len() == 1 {
                    iterate(&iterable, line)?
                        .into_iter()
                        .map(|item| vec![item])
                        .collect()
                } else {
                    iterate_pairs(&iterable, line)?
                };

                for row in rows {
                    for (name, value) in names.iter().zip(row) {
                        self.env.assign(name, value);
                    }

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
            StmtKind::SourceFile {
                file,
                key,
                statements,
            } => {
                if self.env.is_imported(key) {
                    return Ok(Flow::Normal);
                }

                // Mark before running so a dynamically recursive import cannot
                // loop forever. A failed import is unmarked so the REPL can
                // retry it after the file is fixed.
                self.env.mark_imported(key.clone());
                let saved_file = self.current_file.replace(file.clone());
                let result = self.exec_statements(statements);
                self.current_file = saved_file;

                if result.is_err() {
                    self.env.unmark_imported(key);
                }

                result.map_err(|error| error.with_file(file.clone()))
            }
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
            Expr::List(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.eval(item, line)?);
                }
                Ok(Value::list(values))
            }
            Expr::TypedList {
                element_type,
                items,
            } => {
                let spec = parse_type_spec(element_type).map_err(|message| Error::new(line, message))?;
                self.env
                    .validate_type_spec(&spec)
                    .map_err(|message| Error::new(line, message))?;

                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.eval(item, line)?);
                }

                if values.len() == 1 && is_iterable(&values[0]) {
                    values = iterate(&values[0], line)?;
                }

                for (index, value) in values.iter().enumerate() {
                    self.env
                        .ensure_value_matches_spec(value, &spec, line)
                        .map_err(|message| {
                            Error::new(
                                line,
                                format!("List[{element_type}] item {index}: {message}"),
                            )
                        })?;
                }

                Ok(Value::typed_list(values, spec))
            }
            Expr::FormatCall { name, args } => {
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval(arg, line)?);
                }
                format_call(name, values, line)
            }
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
            Expr::Slice {
                object,
                start,
                stop,
                step,
            } => {
                let object = self.eval(object, line)?;
                let start = start
                    .as_ref()
                    .map(|expr| self.eval(expr, line))
                    .transpose()?;
                let stop = stop
                    .as_ref()
                    .map(|expr| self.eval(expr, line))
                    .transpose()?;
                let step = step
                    .as_ref()
                    .map(|expr| self.eval(expr, line))
                    .transpose()?;
                slice_value(object, start, stop, step, line)
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
            "list" | "List" => {
                if values.is_empty() {
                    Ok(Value::list(Vec::new()))
                } else if values.len() == 1 {
                    let items = iterate(&values[0], line)?;
                    let element_type = match &values[0] {
                        Value::List(list) => list.element_type(),
                        _ => None,
                    };
                    Ok(match element_type {
                        Some(element_type) => Value::typed_list(items, element_type),
                        None => Value::list(items),
                    })
                } else {
                    Err(Error::new(
                        line,
                        format!("list() takes at most 1 argument, but {} given", values.len()),
                    ))
                }
            }
            "len" => {
                if values.len() != 1 {
                    return Err(Error::new(
                        line,
                        format!("len() takes 1 argument, but {} given", values.len()),
                    ));
                }

                let length = length_of(&values[0]).ok_or_else(|| {
                    Error::new(
                        line,
                        format!("len() does not support {}", values[0].type_name()),
                    )
                })?;
                Ok(Value::Int(length as i64))
            }
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

        let struct_value = match receiver {
            Value::List(list) => return list_method(self.env, list, method, values, line),
            Value::Struct(struct_value) => struct_value,
            _ => {
                return Err(Error::new(
                    line,
                    format!(
                        "cannot call method '{method}' on a {receiver_type} (only lists and structs have methods)"
                    ),
                ));
            }
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

        let function = definition.method(method).ok_or_else(|| {
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

    fn implement_trait(
        &mut self,
        trait_name: &str,
        struct_name: &str,
        methods: &[Method],
        line: usize,
    ) -> Result<(), Error> {
        let trait_definition = self.env.get_trait(trait_name).ok_or_else(|| {
            Error::new(line, format!("unknown trait '{trait_name}'"))
        })?;
        let struct_definition = self.env.get_struct(struct_name).ok_or_else(|| {
            Error::new(line, format!("unknown struct '{struct_name}'"))
        })?;

        if struct_definition.traits.contains(trait_name) {
            return Err(Error::new(
                line,
                format!("struct '{struct_name}' already implements trait '{trait_name}'"),
            ));
        }

        let mut provided: HashMap<String, Rc<Function>> = HashMap::new();

        for method in methods {
            let Some(trait_method) = trait_definition.methods.get(&method.name) else {
                return Err(Error::new(
                    line,
                    format!("trait '{trait_name}' has no method '{}'", method.name),
                ));
            };

            if trait_method.params.len() != method.params.len() {
                return Err(Error::new(
                    line,
                    format!(
                        "method '{}' for trait '{trait_name}' takes {} parameter(s), but {} given",
                        method.name,
                        trait_method.params.len(),
                        method.params.len()
                    ),
                ));
            }

            if let (Some(provided_type), Some(required_type)) =
                (&method.return_type, &trait_method.return_type)
                && provided_type != required_type
            {
                return Err(Error::new(
                    line,
                    format!(
                        "method '{}' for trait '{trait_name}' must return {required_type}, but returns {provided_type}",
                        method.name
                    ),
                ));
            }

            provided.insert(
                method.name.clone(),
                Rc::new(Function {
                    type_params: Vec::new(),
                    params: method.params.clone(),
                    return_type: method
                        .return_type
                        .clone()
                        .or_else(|| trait_method.return_type.clone()),
                    body: method.body.clone(),
                    file: self.current_file.clone(),
                }),
            );
        }

        let mut final_methods = Vec::new();
        let mut missing = Vec::new();

        for (method_name, trait_method) in &trait_definition.methods {
            if let Some(function) = provided.get(method_name) {
                final_methods.push((method_name.clone(), function.clone()));
            } else if let Some(body) = &trait_method.body {
                final_methods.push((
                    method_name.clone(),
                    Rc::new(Function {
                        type_params: Vec::new(),
                        params: trait_method.params.clone(),
                        return_type: trait_method.return_type.clone(),
                        body: body.clone(),
                        file: trait_method.file.clone(),
                    }),
                ));
            } else {
                missing.push(method_name.clone());
            }
        }

        if !missing.is_empty() {
            missing.sort();
            return Err(Error::new(
                line,
                format!(
                    "struct '{struct_name}' is missing required method(s) for trait '{trait_name}': {}",
                    missing.join(", ")
                ),
            ));
        }

        for (method_name, _) in &final_methods {
            if let Some(source) = struct_definition.trait_sources.get(method_name)
                && source != trait_name
            {
                return Err(Error::new(
                    line,
                    format!(
                        "method '{method_name}' on struct '{struct_name}' already comes from trait '{source}', so it cannot also come from '{trait_name}'"
                    ),
                ));
            }
        }

        self.env
            .add_trait_methods(struct_name, trait_name, final_methods)
            .map_err(|message| Error::new(line, message))?;

        Ok(())
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

        let saved_file = std::mem::replace(&mut self.current_file, function.file.clone());

        let result = match self.bind_parameters(function, values, label, line) {
            Ok(()) => {
                if let Err(error) = self.check_type_bounds(function, label, line) {
                    Err(error)
                } else {
                    let loop_depth = std::mem::replace(&mut self.loop_depth, 0);
                    self.function_depth += 1;

                    let flow = self.exec_statements(&function.body);

                    self.loop_depth = loop_depth;
                    self.function_depth -= 1;

                    match flow {
                        Ok(Flow::Return(value)) => Ok(value),
                        Ok(Flow::Normal) => Ok(Value::None),
                        Ok(Flow::Break | Flow::Continue) => Err(Error::new(
                            line,
                            "'break' or 'continue' escaped a function body",
                        )),
                        Err(error) => Err(error),
                    }
                }
            }
            Err(error) => Err(error),
        };

        self.current_file = saved_file;

        let result = match result {
            Ok(value) => self
                .check_return_type(function, &value, line, label)
                .map(|()| value),
            Err(error) => Err(error),
        };

        let result = match &function.file {
            Some(file) => result.map_err(|error| error.with_file(file.clone())),
            None => result,
        };

        let final_self = self.env.get_value("self").ok();
        self.env.pop_scope();
        result.map(|value| (value, final_self))
    }

    fn check_type_bounds(
        &self,
        function: &Function,
        label: &str,
        line: usize,
    ) -> Result<(), Error> {
        for (index, type_param) in function.type_params.iter().enumerate() {
            let Some(bound) = &type_param.bound else {
                continue;
            };
            let Some(param) = function.params.get(index) else {
                return Err(Error::new(
                    line,
                    format!(
                        "type parameter '{}' has no matching parameter",
                        type_param.name
                    ),
                ));
            };

            if self.env.get_trait(bound).is_none() {
                return Err(Error::new(
                    line,
                    format!("{label} uses unknown trait '{bound}' in its type bound"),
                ));
            }

            // A type parameter nested inside a container, as in
            // `values: List[T]`, is checked by the parameter type annotation
            // against each element instead of against the whole list.
            if param
                .type_annotation
                .as_deref()
                .is_some_and(|annotation| annotation != type_param.name)
            {
                continue;
            }

            let value = self
                .env
                .get_value(&param.name)
                .map_err(|message| Error::new(line, message))?;

            match value {
                Value::Struct(struct_value) => {
                    let Some(definition) = self.env.get_struct(&struct_value.type_name) else {
                        return Err(Error::new(
                            line,
                            format!("unknown struct '{}'", struct_value.type_name),
                        ));
                    };

                    if !definition.traits.contains(bound) {
                        return Err(Error::new(
                            line,
                            format!(
                                "argument '{}' to {label} must implement trait '{bound}', but struct '{}' does not",
                                param.name, struct_value.type_name
                            ),
                        ));
                    }
                }
                other => {
                    return Err(Error::new(
                        line,
                        format!(
                            "argument '{}' to {label} must implement trait '{bound}', got {}",
                            param.name,
                            other.type_name()
                        ),
                    ));
                }
            }
        }

        Ok(())
    }

    fn bind_parameters(
        &mut self,
        function: &Function,
        values: Vec<Value>,
        label: &str,
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

            self.check_parameter_type(function, param, &value, label, line)?;
            self.env.define(param.name.clone(), value);
        }

        Ok(())
    }

    fn check_parameter_type(
        &self,
        function: &Function,
        param: &Parameter,
        value: &Value,
        label: &str,
        line: usize,
    ) -> Result<(), Error> {
        let Some(annotation) = &param.type_annotation else {
            return Ok(());
        };
        if annotation.contains('[') {
            let spec = parse_type_spec(annotation).map_err(|message| Error::new(line, message))?;
            let spec = resolve_type_spec(spec, &function.type_params);
            self.env
                .validate_type_spec(&spec)
                .map_err(|message| Error::new(line, message))?;
            self.env
                .ensure_value_matches_spec(value, &spec, line)
                .map_err(|message| {
                    Error::new(
                        line,
                        format!(
                            "argument '{}' to {label} must be {annotation}: {message}",
                            param.name
                        ),
                    )
                })?;
            return Ok(());
        }

        // A type parameter used as an annotation, as in `def f<T>(value: T)`,
        // accepts any value unless the type parameter has a trait bound.
        if let Some(type_param) = function
            .type_params
            .iter()
            .find(|type_param| type_param.name == *annotation)
        {
            let accepted = match &type_param.bound {
                None => true,
                Some(bound) => match value {
                    Value::Struct(struct_value) => self
                        .env
                        .get_struct(&struct_value.type_name)
                        .is_some_and(|definition| definition.traits.contains(bound)),
                    _ => false,
                },
            };

            return if accepted {
                Ok(())
            } else {
                Err(Error::new(
                    line,
                    format!(
                        "argument '{}' to {label} must implement trait '{}'",
                        param.name,
                        type_param.bound.as_deref().unwrap_or("")
                    ),
                ))
            };
        }

        let normalized = annotation.to_ascii_lowercase();
        let accepted = match normalized.as_str() {
            "any" => true,
            "int" => matches!(value, Value::Int(_)),
            "float" => matches!(value, Value::Float(_)),
            "number" => matches!(value, Value::Int(_) | Value::Float(_)),
            "bool" => matches!(value, Value::Bool(_)),
            "text" | "string" => matches!(value, Value::Text(_)),
            "none" => matches!(value, Value::None),
            "list" => matches!(value, Value::List(_)),
            "hashset" | "set" => matches!(value, Value::HashSet(_)),
            "hashmap" | "map" => matches!(value, Value::Map(_)),
            "matrix" | "mat" => matches!(value, Value::Matrix(_)),
            _ => {
                let expected = annotation.as_str();
                if self.env.get_trait(expected).is_some() {
                    match value {
                        Value::Struct(struct_value) => self
                            .env
                            .get_struct(&struct_value.type_name)
                            .is_some_and(|definition| definition.traits.contains(expected)),
                        _ => false,
                    }
                } else if self.env.get_struct(expected).is_some() {
                    matches!(value, Value::Struct(struct_value) if struct_value.type_name == expected)
                } else {
                    return Err(Error::new(
                        line,
                        format!(
                            "unknown type annotation '{}' on parameter '{}'",
                            expected, param.name
                        ),
                    ));
                }
            }
        };

        if accepted {
            Ok(())
        } else {
            Err(Error::new(
                line,
                format!(
                    "argument '{}' to {label} must be {annotation}, got {}",
                    param.name,
                    value.type_name()
                ),
            ))
        }
    }

    /// Checks a function or method result against its `-> Type` annotation.
    fn check_return_type(
        &self,
        function: &Function,
        value: &Value,
        line: usize,
        label: &str,
    ) -> Result<(), Error> {
        let Some(annotation) = &function.return_type else {
            return Ok(());
        };

        let spec = parse_type_spec(annotation).map_err(|message| Error::new(line, message))?;
        let spec = resolve_type_spec(spec, &function.type_params);
        self.env
            .validate_type_spec(&spec)
            .map_err(|message| Error::new(line, message))?;
        self.env
            .ensure_value_matches_spec(value, &spec, line)
            .map_err(|message| {
                Error::new(
                    line,
                    format!("{label} must return {annotation}: {message}"),
                )
            })
    }
}

fn format_call(name: &str, values: Vec<Value>, line: usize) -> Result<Value, Error> {
    let format_string = if values.is_empty() {
        ""
    } else {
        match &values[0] {
            Value::Text(text) => text.as_str(),
            other => {
                return Err(Error::new(
                    line,
                    format!(
                        "{name}!() format string must be text, got {}",
                        other.type_name()
                    ),
                ));
            }
        }
    };

    let args = if values.is_empty() { &[][..] } else { &values[1..] };
    let rendered = render_format(format_string, args, name, line)?;

    match name {
        "print" => {
            print!("{rendered}");
            io::stdout()
                .flush()
                .map_err(|error| Error::new(line, format!("couldn't flush stdout: {error}")))?;
        }
        "println" => println!("{rendered}"),
        "eprint" => {
            eprint!("{rendered}");
            io::stderr()
                .flush()
                .map_err(|error| Error::new(line, format!("couldn't flush stderr: {error}")))?;
        }
        "eprintln" => eprintln!("{rendered}"),
        _ => unreachable!("the parser only accepts known format macros"),
    }

    Ok(Value::None)
}

fn render_format(format: &str, args: &[Value], name: &str, line: usize) -> Result<String, Error> {
    let characters: Vec<char> = format.chars().collect();
    let mut output = String::new();
    let mut position = 0usize;
    let mut auto_index = 0usize;
    let mut used = vec![false; args.len()];

    while position < characters.len() {
        match characters[position] {
            '{' => {
                if characters.get(position + 1) == Some(&'{') {
                    output.push('{');
                    position += 2;
                    continue;
                }

                let start = position + 1;
                let mut end = start;
                while end < characters.len() && characters[end] != '}' {
                    end += 1;
                }

                if end == characters.len() {
                    return Err(Error::new(
                        line,
                        "unclosed '{' in the format string (use '{{' for a literal '{')",
                    ));
                }

                let spec: String = characters[start..end].iter().collect();
                let (argument_index, debug) =
                    parse_format_spec(&spec, &mut auto_index, args.len(), name, line)?;
                used[argument_index] = true;

                if debug {
                    output.push_str(&repr_value(&args[argument_index]));
                } else {
                    output.push_str(&args[argument_index].to_string());
                }

                position = end + 1;
            }
            '}' => {
                if characters.get(position + 1) == Some(&'}') {
                    output.push('}');
                    position += 2;
                } else {
                    return Err(Error::new(
                        line,
                        "single '}' in the format string (use '}}' for a literal '}')",
                    ));
                }
            }
            character => {
                output.push(character);
                position += 1;
            }
        }
    }

    for (index, was_used) in used.iter().enumerate() {
        if !was_used {
            return Err(Error::new(
                line,
                format!("{name}!() argument {} is never used", index + 1),
            ));
        }
    }

    Ok(output)
}

fn parse_format_spec(
    spec: &str,
    auto_index: &mut usize,
    argument_count: usize,
    name: &str,
    line: usize,
) -> Result<(usize, bool), Error> {
    let (index_part, debug) = if spec == ":?" {
        ("", true)
    } else if let Some(index) = spec.strip_suffix(":?") {
        (index, true)
    } else if spec.contains(':') {
        return Err(Error::new(
            line,
            format!("unsupported format specifier '{{{spec}}}' (use {{}} or {{:?}})"),
        ));
    } else {
        (spec, false)
    };

    let argument_index = if index_part.is_empty() {
        let index = *auto_index;
        *auto_index += 1;
        index
    } else {
        index_part.parse::<usize>().map_err(|_| {
            Error::new(
                line,
                format!("invalid argument index '{index_part}' in format string"),
            )
        })?
    };

    if argument_index >= argument_count {
        return Err(Error::new(
            line,
            format!(
                "format string references argument {argument_index}, but only {argument_count} given to {name}!"
            ),
        ));
    }

    Ok((argument_index, debug))
}

fn list_method(
    env: &Env,
    list: Rc<ListValue>,
    method: &str,
    args: Vec<Value>,
    line: usize,
) -> Result<Value, Error> {
    match method {
        "append" => {
            expect_arity("list.append", &args, 1, line)?;
            ensure_list_element(env, &list, &args[0], line)?;
            list.items.borrow_mut().push(args[0].clone());
            Ok(Value::None)
        }
        "extend" => {
            expect_arity("list.extend", &args, 1, line)?;
            let new_items = iterate(&args[0], line)?;
            for item in &new_items {
                ensure_list_element(env, &list, item, line)?;
            }
            list.items.borrow_mut().extend(new_items);
            Ok(Value::None)
        }
        "insert" => {
            expect_arity("list.insert", &args, 2, line)?;
            let index = int_arg("list.insert", &args[0], line)?;
            ensure_list_element(env, &list, &args[1], line)?;
            let len = list.items.borrow().len();
            let index = clamp_insert_index(index, len);
            list.items.borrow_mut().insert(index, args[1].clone());
            Ok(Value::None)
        }
        "remove" => {
            expect_arity("list.remove", &args, 1, line)?;
            let mut items = list.items.borrow_mut();
            let Some(index) = items
                .iter()
                .position(|item| equal(item, &args[0]))
            else {
                return Err(Error::new(
                    line,
                    format!("list.remove(x): {} is not in the list", args[0]),
                ));
            };
            items.remove(index);
            Ok(Value::None)
        }
        "pop" => {
            let index = if args.is_empty() {
                None
            } else if args.len() == 1 {
                Some(int_arg("list.pop", &args[0], line)?)
            } else {
                return Err(Error::new(
                    line,
                    format!("list.pop() takes at most 1 argument, but {} given", args.len()),
                ));
            };

            let mut items = list.items.borrow_mut();
            if items.is_empty() {
                return Err(Error::new(line, "pop from empty list"));
            }

            let position = match index {
                Some(index) => resolve_index(index, items.len()).ok_or_else(|| {
                    Error::new(
                        line,
                        format!(
                            "list.pop() index {index} is out of range (length {})",
                            items.len()
                        ),
                    )
                })?,
                None => items.len() - 1,
            };

            Ok(items.remove(position))
        }
        "clear" => {
            expect_arity("list.clear", &args, 0, line)?;
            list.items.borrow_mut().clear();
            Ok(Value::None)
        }
        "copy" => {
            expect_arity("list.copy", &args, 0, line)?;
            let items = list.items.borrow().clone();
            Ok(match list.element_type() {
                Some(element_type) => Value::typed_list(items, element_type),
                None => Value::list(items),
            })
        }
        "index" => {
            expect_arity("list.index", &args, 1, line)?;
            let items = list.items.borrow();
            let Some(index) = items.iter().position(|item| equal(item, &args[0])) else {
                return Err(Error::new(
                    line,
                    format!("list.index(x): {} is not in the list", args[0]),
                ));
            };
            Ok(Value::Int(index as i64))
        }
        "count" => {
            expect_arity("list.count", &args, 1, line)?;
            let count = list
                .items
                .borrow()
                .iter()
                .filter(|item| equal(item, &args[0]))
                .count();
            Ok(Value::Int(count as i64))
        }
        "reverse" => {
            expect_arity("list.reverse", &args, 0, line)?;
            list.items.borrow_mut().reverse();
            Ok(Value::None)
        }
        "sort" => {
            expect_arity("list.sort", &args, 0, line)?;
            sort_list(&mut list.items.borrow_mut(), line)?;
            Ok(Value::None)
        }
        _ => Err(Error::new(
            line,
            format!(
                "unknown list method '{method}' (available: append, clear, copy, count, extend, index, insert, pop, remove, reverse, sort)"
            ),
        )),
    }
}

fn ensure_list_element(
    env: &Env,
    list: &ListValue,
    value: &Value,
    line: usize,
) -> Result<(), Error> {
    if let Some(element_type) = list.element_type() {
        env.ensure_value_matches_spec(value, &element_type, line)
            .map_err(|message| Error::new(line, message))?;
    }
    Ok(())
}

fn expect_arity(label: &str, args: &[Value], expected: usize, line: usize) -> Result<(), Error> {
    if args.len() != expected {
        return Err(Error::new(
            line,
            format!(
                "{label}() takes {expected} argument(s), but {} given",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn int_arg(label: &str, value: &Value, line: usize) -> Result<i64, Error> {
    match value {
        Value::Int(number) => Ok(*number),
        other => Err(Error::new(
            line,
            format!("{label}() index must be an int, got {}", other.type_name()),
        )),
    }
}

fn clamp_insert_index(index: i64, len: usize) -> usize {
    let len = len as i64;
    let index = if index < 0 { index + len } else { index };
    index.clamp(0, len) as usize
}

fn sort_list(items: &mut [Value], line: usize) -> Result<(), Error> {
    if items.len() < 2 {
        return Ok(());
    }

    if items
        .iter()
        .all(|value| matches!(value, Value::Int(_) | Value::Float(_)))
    {
        items.sort_by(|left, right| {
            number_value(left)
                .partial_cmp(&number_value(right))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        return Ok(());
    }

    if items.iter().all(|value| matches!(value, Value::Text(_))) {
        items.sort_by(|left, right| match (left, right) {
            (Value::Text(left), Value::Text(right)) => left.cmp(right),
            _ => unreachable!("checked that all items are text"),
        });
        return Ok(());
    }

    if items.iter().all(|value| matches!(value, Value::Bool(_))) {
        items.sort_by_key(|value| match value {
            Value::Bool(value) => *value,
            _ => unreachable!("checked that all items are bools"),
        });
        return Ok(());
    }

    Err(Error::new(
        line,
        "list.sort() only supports lists of numbers, text, or bools",
    ))
}

fn number_value(value: &Value) -> f64 {
    match value {
        Value::Int(number) => *number as f64,
        Value::Float(number) => *number,
        _ => f64::NAN,
    }
}

fn length_of(value: &Value) -> Option<usize> {
    match value {
        Value::List(list) => Some(list.items.borrow().len()),
        Value::Text(text) => Some(text.chars().count()),
        Value::HashSet(items) => Some(items.len()),
        Value::Map(entries) => Some(entries.len()),
        Value::Matrix(rows) => Some(rows.len()),
        _ => None,
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
        BinaryOp::In => Ok(Value::Bool(membership(&left, &right, line)?)),
        BinaryOp::NotIn => Ok(Value::Bool(!membership(&left, &right, line)?)),
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

fn membership(value: &Value, container: &Value, line: usize) -> Result<bool, Error> {
    match container {
        Value::List(list) => Ok(list.items.borrow().iter().any(|item| equal(item, value))),
        Value::Text(text) => match value {
            Value::Text(needle) => Ok(text.contains(needle)),
            other => Err(Error::new(
                line,
                format!(
                    "text membership needs text on the left, got {}",
                    other.type_name()
                ),
            )),
        },
        Value::HashSet(items) => Ok(SetItem::from_value(value.clone())
            .is_ok_and(|item| items.contains(&item))),
        Value::Map(entries) => Ok(SetItem::from_value(value.clone())
            .is_ok_and(|key| entries.contains_key(&key))),
        Value::Matrix(rows) => match value {
            Value::Text(row) => Ok(rows.contains_key(row)),
            _ => Ok(false),
        },
        other => Err(Error::new(
            line,
            format!("cannot check membership in a {}", other.type_name()),
        )),
    }
}

fn add(left: Value, right: Value, line: usize) -> Result<Value, Error> {
    match (&left, &right) {
        (Value::Int(a), Value::Int(b)) => a
            .checked_add(*b)
            .map(Value::Int)
            .ok_or_else(|| Error::new(line, "integer overflow")),
        (Value::Text(a), Value::Text(b)) => Ok(Value::Text(format!("{a}{b}"))),
        (Value::List(a), Value::List(b)) => {
            let mut items = a.items.borrow().clone();
            items.extend(b.items.borrow().iter().cloned());
            let element_type = a.element_type();
            Ok(match (element_type, b.element_type()) {
                (Some(a_type), Some(b_type)) if a_type == b_type => {
                    Value::typed_list(items, a_type)
                }
                _ => Value::list(items),
            })
        }
        _ => match float_pair(&left, &right) {
            Some((a, b)) => Ok(Value::Float(a + b)),
            None => Err(type_error("add", &left, &right, line)),
        },
    }
}

fn repeat_list(list: &Rc<ListValue>, count: i64) -> Result<Value, Error> {
    if count <= 0 {
        return Ok(match list.element_type() {
            Some(element_type) => Value::typed_list(Vec::new(), element_type),
            None => Value::list(Vec::new()),
        });
    }

    let source = list.items.borrow();
    let mut result = Vec::with_capacity(source.len().saturating_mul(count as usize));

    for _ in 0..count {
        result.extend(source.iter().cloned());
    }

    Ok(match list.element_type() {
        Some(element_type) => Value::typed_list(result, element_type),
        None => Value::list(result),
    })
}

fn arithmetic(op: BinaryOp, left: Value, right: Value, line: usize) -> Result<Value, Error> {
    if op == BinaryOp::Multiply {
        if let (Value::List(items), Value::Int(count)) = (&left, &right) {
            return repeat_list(items, *count);
        }
        if let (Value::Int(count), Value::List(items)) = (&left, &right) {
            return repeat_list(items, *count);
        }
    }

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
        Value::List(list) => Ok(list.items.borrow().clone()),
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
                "cannot loop over a {} (use a list, hashset, hashmap, matrix, or text)",
                other.type_name()
            ),
        )),
    }
}

fn iterate_pairs(value: &Value, line: usize) -> Result<Vec<Vec<Value>>, Error> {
    match value {
        Value::Map(entries) => {
            let mut keys: Vec<&SetItem> = entries.keys().collect();
            keys.sort_by_key(|key| key.to_string());
            Ok(keys
                .into_iter()
                .map(|key| {
                    vec![
                        set_item_to_value(key),
                        entries.get(key).cloned().unwrap_or(Value::None),
                    ]
                })
                .collect())
        }
        other => Err(Error::new(
            line,
            format!(
                "two-variable 'for' loops need a hashmap, got {}",
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

fn resolve_index(position: i64, len: usize) -> Option<usize> {
    let len = len as i64;
    let resolved = if position < 0 {
        len + position
    } else {
        position
    };

    if resolved < 0 || resolved >= len {
        None
    } else {
        Some(resolved as usize)
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
        Value::List(list) => {
            let type_name = index.type_name().to_string();
            let Value::Int(position) = index else {
                return Err(Error::new(
                    line,
                    format!("list index must be an int, got {type_name}"),
                ));
            };

            let items = list.items.borrow();
            let resolved = resolve_index(position, items.len()).ok_or_else(|| {
                Error::new(
                    line,
                    format!(
                        "list index {position} is out of range (length {})",
                        items.len()
                    ),
                )
            })?;
            Ok(items[resolved].clone())
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
            let resolved = resolve_index(position, chars.len()).ok_or_else(|| {
                Error::new(
                    line,
                    format!(
                        "text index {position} is out of range (length {})",
                        chars.len()
                    ),
                )
            })?;
            Ok(Value::Text(chars[resolved].to_string()))
        }
        other => Err(Error::new(
            line,
            format!(
                "cannot index a {} (only hashmap, list, and text support [ ])",
                other.type_name()
            ),
        )),
    }
}

fn slice_value(
    object: Value,
    start: Option<Value>,
    stop: Option<Value>,
    step: Option<Value>,
    line: usize,
) -> Result<Value, Error> {
    match object {
        Value::List(list) => {
            let items = list.items.borrow();
            let (start, stop, step) = normalize_slice(items.len(), start, stop, step, line)?;
            let result: Vec<Value> = indices_from_parts(start, stop, step)
                .into_iter()
                .map(|index| items[index].clone())
                .collect();
            drop(items);
            Ok(match list.element_type() {
                Some(element_type) => Value::typed_list(result, element_type),
                None => Value::list(result),
            })
        }
        Value::Text(text) => {
            let chars: Vec<char> = text.chars().collect();
            let (start, stop, step) = normalize_slice(chars.len(), start, stop, step, line)?;
            let result = indices_from_parts(start, stop, step)
                .into_iter()
                .map(|index| chars[index])
                .collect();
            Ok(Value::Text(result))
        }
        other => Err(Error::new(
            line,
            format!("cannot slice a {}", other.type_name()),
        )),
    }
}

fn normalize_slice(
    len: usize,
    start: Option<Value>,
    stop: Option<Value>,
    step: Option<Value>,
    line: usize,
) -> Result<(i64, i64, i64), Error> {
    let len = len as i64;
    let step = match step {
        None => 1,
        Some(Value::Int(step)) => step,
        Some(other) => {
            return Err(Error::new(
                line,
                format!("slice step must be an int, got {}", other.type_name()),
            ));
        }
    };

    if step == 0 {
        return Err(Error::new(line, "slice step cannot be zero"));
    }

    let parse_bound = |value: Option<Value>, label: &str| -> Result<Option<i64>, Error> {
        match value {
            None => Ok(None),
            Some(Value::Int(value)) => Ok(Some(value)),
            Some(other) => Err(Error::new(
                line,
                format!("slice {label} must be an int, got {}", other.type_name()),
            )),
        }
    };

    let start = parse_bound(start, "start")?;
    let stop = parse_bound(stop, "stop")?;

    if step > 0 {
        Ok((
            start.map_or(0, |index| clamp_slice_positive(index, len)),
            stop.map_or(len, |index| clamp_slice_positive(index, len)),
            step,
        ))
    } else {
        Ok((
            start.map_or(len - 1, |index| clamp_slice_negative(index, len)),
            stop.map_or(-1, |index| clamp_slice_negative(index, len)),
            step,
        ))
    }
}

fn clamp_slice_positive(index: i64, len: i64) -> i64 {
    let index = if index < 0 { index + len } else { index };
    index.clamp(0, len)
}

fn clamp_slice_negative(index: i64, len: i64) -> i64 {
    let index = if index < 0 { index + len } else { index };
    index.clamp(-1, len - 1)
}

fn indices_from_parts(mut start: i64, stop: i64, step: i64) -> Vec<usize> {
    let mut indices = Vec::new();

    if step > 0 {
        while start < stop {
            indices.push(start as usize);
            start += step;
        }
    } else {
        while start > stop {
            indices.push(start as usize);
            start += step;
        }
    }

    indices
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
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn env_after(source: &str) -> Env {
        let mut env = Env::new();
        run_with_env(&mut env, source).unwrap();
        env
    }

    fn error_of(source: &str) -> Error {
        let mut env = Env::new();
        run_with_env(&mut env, source).unwrap_err()
    }

    fn temp_project(name: &str) -> (PathBuf, PathBuf) {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let directory = std::env::temp_dir().join(format!(
            "dsh2_import_{}_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            name
        ));
        fs::create_dir_all(&directory).unwrap();
        let main = directory.join("main.dsq");
        (directory, main)
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
    fn lists_support_literals_indexing_slicing_and_display() {
        let env = env_after(
            "a = [1, 2, 3]\n\
             first = a[0]\n\
             last = a[-1]\n\
             middle = a[1:3]\n\
             reversed = a[::-1]\n\
             stepped = a[::2]\n\
             from_start = a[1:]\n\
             to_stop = a[:2]\n\
             empty = []\n\
             mixed = [1, \"two\", [3]]",
        );

        assert_eq!(env.get_value("a").unwrap().to_string(), "[1, 2, 3]");
        assert_eq!(env.get_value("first").unwrap(), Value::Int(1));
        assert_eq!(env.get_value("last").unwrap(), Value::Int(3));
        assert_eq!(
            env.get_value("middle").unwrap(),
            Value::list(vec![Value::Int(2), Value::Int(3)])
        );
        assert_eq!(
            env.get_value("reversed").unwrap(),
            Value::list(vec![Value::Int(3), Value::Int(2), Value::Int(1)])
        );
        assert_eq!(
            env.get_value("stepped").unwrap(),
            Value::list(vec![Value::Int(1), Value::Int(3)])
        );
        assert_eq!(
            env.get_value("from_start").unwrap(),
            Value::list(vec![Value::Int(2), Value::Int(3)])
        );
        assert_eq!(
            env.get_value("to_stop").unwrap(),
            Value::list(vec![Value::Int(1), Value::Int(2)])
        );
        assert_eq!(env.get_value("empty").unwrap().to_string(), "[]");
        assert_eq!(
            env.get_value("mixed").unwrap().to_string(),
            "[1, \"two\", [3]]"
        );
    }

    #[test]
    fn lists_are_mutable_and_share_like_python() {
        let env = env_after(
            "a = [1, 2, 3]\n\
             b = a\n\
             b.append(4)\n\
             a[0] = 9\n\
             inner = [1]\n\
             outer = [inner]\n\
             outer[0].append(2)\n\
             alias_value = a\n\
             a.append(5)",
        );

        assert_eq!(env.get_value("a").unwrap().to_string(), "[9, 2, 3, 4, 5]");
        assert_eq!(env.get_value("b").unwrap().to_string(), "[9, 2, 3, 4, 5]");
        assert_eq!(
            env.get_value("alias_value").unwrap().to_string(),
            "[9, 2, 3, 4, 5]"
        );
        assert_eq!(env.get_value("inner").unwrap().to_string(), "[1, 2]");
        assert_eq!(env.get_value("outer").unwrap().to_string(), "[[1, 2]]");
    }

    #[test]
    fn lists_support_membership_tests() {
        let env = env_after(
            "a = [1, 2, 3]\n\
             has_two = 2 in a\n\
             has_four = 4 in a\n\
             missing = 4 not in a\n\
             text_has = \"b\" in \"abc\"\n\
             map_has = 2 in {1: \"a\", 2: \"b\"}\n\
             set_has = 2 in hashSet(1, 2, 3)",
        );

        assert_eq!(env.get_value("has_two").unwrap(), Value::Bool(true));
        assert_eq!(env.get_value("has_four").unwrap(), Value::Bool(false));
        assert_eq!(env.get_value("missing").unwrap(), Value::Bool(true));
        assert_eq!(env.get_value("text_has").unwrap(), Value::Bool(true));
        assert_eq!(env.get_value("map_has").unwrap(), Value::Bool(true));
        assert_eq!(env.get_value("set_has").unwrap(), Value::Bool(true));
    }

    #[test]
    fn list_slices_can_be_assigned() {
        let env = env_after(
            "a = [1, 2, 3, 4, 5]\n\
             a[1:3] = [7, 8, 9]\n\
             a[2:2] = [0]\n\
             a[::3] = [10, 20, 30]\n\
             a[::-1] = [100, 200, 300, 400, 500, 600, 700]\n\
             b = [1, 2, 3]\n\
             b[10:10] = [4]\n\
             c = [1, 2]\n\
             c[:] = [9]",
        );

        assert_eq!(
            env.get_value("a").unwrap().to_string(),
            "[700, 600, 500, 400, 300, 200, 100]"
        );
        assert_eq!(env.get_value("b").unwrap().to_string(), "[1, 2, 3, 4]");
        assert_eq!(env.get_value("c").unwrap().to_string(), "[9]");
    }

    #[test]
    fn del_removes_list_items_slices_and_map_entries() {
        let env = env_after(
            "a = [1, 2, 3, 4, 5]\n\
             del a[1]\n\
             del a[1:3]\n\
             b = [0, 1, 2, 3, 4, 5]\n\
             del b[::2]\n\
             d = {1: \"a\", 2: \"b\"}\n\
             del d[1]",
        );

        assert_eq!(env.get_value("a").unwrap().to_string(), "[1, 5]");
        assert_eq!(env.get_value("b").unwrap().to_string(), "[1, 3, 5]");
        assert_eq!(env.get_value("d").unwrap().to_string(), "{2: \"b\"}");

        let error = error_of("text = \"abc\"\ndel text[0]");
        assert!(error.message.contains("not a hashmap or list"), "{error}");
        assert_eq!(error.line, 2);
    }

    #[test]
    fn list_methods_match_python() {
        let env = env_after(
            "a = [3, 1, 2]\n\
             a.sort()\n\
             sorted = a.copy()\n\
             a.reverse()\n\
             a.append(4)\n\
             a.extend([5, 6])\n\
             a.insert(-1, 9)\n\
             removed = a.pop()\n\
             count = a.count(3)\n\
             index = a.index(9)\n\
             a.remove(9)\n\
             copied = a.copy()\n\
             copied.append(99)\n\
             values = a + [7]\n\
             repeated = [0] * 3\n\
             from_text = list(\"abc\")\n\
             length = len(a)",
        );

        assert_eq!(env.get_value("sorted").unwrap().to_string(), "[1, 2, 3]");
        assert_eq!(
            env.get_value("a").unwrap().to_string(),
            "[3, 2, 1, 4, 5]"
        );
        assert_eq!(env.get_value("removed").unwrap(), Value::Int(6));
        assert_eq!(env.get_value("count").unwrap(), Value::Int(1));
        assert_eq!(env.get_value("index").unwrap(), Value::Int(5));
        assert_eq!(env.get_value("copied").unwrap().to_string(), "[3, 2, 1, 4, 5, 99]");
        assert_eq!(
            env.get_value("values").unwrap().to_string(),
            "[3, 2, 1, 4, 5, 7]"
        );
        assert_eq!(
            env.get_value("repeated").unwrap().to_string(),
            "[0, 0, 0]"
        );
        assert_eq!(
            env.get_value("from_text").unwrap().to_string(),
            "[\"a\", \"b\", \"c\"]"
        );
        assert_eq!(env.get_value("length").unwrap(), Value::Int(5));
    }

    #[test]
    fn list_type_annotations_and_errors_report_the_line() {
        let env = env_after(
            "def first(xs: List) { xs[0] }\n\
             value = first([7, 8])",
        );
        assert_eq!(env.get_value("value").unwrap(), Value::Int(7));

        let error = error_of("a = [1]\nprint(a[\"x\"])");
        assert!(error.message.contains("list index must be an int"), "{error}");
        assert_eq!(error.line, 2);

        let error = error_of("a = [1]\nprint(a[3])");
        assert!(error.message.contains("out of range"), "{error}");
        assert_eq!(error.line, 2);

        let error = error_of("a = [1]\nprint(a[::0])");
        assert!(error.message.contains("slice step cannot be zero"), "{error}");
        assert_eq!(error.line, 2);

        let error = error_of("a = [1, 2, 3]\na[::2] = [1]");
        assert!(error.message.contains("extended slice"), "{error}");

        let error = error_of("a = []\na.pop()");
        assert!(error.message.contains("pop from empty list"), "{error}");

        let error = error_of("a = [1]\na.remove(2)");
        assert!(error.message.contains("is not in the list"), "{error}");

        let error = error_of("a = [1, \"x\"]\na.sort()");
        assert!(error.message.contains("only supports lists"), "{error}");
    }

    #[test]
    fn typed_lists_enforce_their_element_type() {
        let env = env_after(
            "xs = List[Int](1, 2, 3)\n\
             xs.append(4)\n\
             from_source = List[Int]([5, 6])\n\
             words = List[Text](\"a\", \"b\")\n\
             nested = List[List[Int]]([[1], [2, 3]])\n\
             nested[0].append(4)\n\
             def total(values: List[Int]) {\n\
             \x20   result = 0\n\
             \x20   for value in values { result = result + value }\n\
             \x20   result\n\
             }\n\
             plain = [10, 20]\n\
             sum = total(plain)\n\
             plain.append(30)",
        );

        assert_eq!(env.get_value("xs").unwrap().to_string(), "[1, 2, 3, 4]");
        assert_eq!(
            env.get_value("from_source").unwrap().to_string(),
            "[5, 6]"
        );
        assert_eq!(
            env.get_value("words").unwrap().to_string(),
            "[\"a\", \"b\"]"
        );
        assert_eq!(
            env.get_value("nested").unwrap().to_string(),
            "[[1, 4], [2, 3]]"
        );
        assert_eq!(env.get_value("sum").unwrap(), Value::Int(30));
        assert_eq!(
            env.get_value("plain").unwrap().to_string(),
            "[10, 20, 30]"
        );
    }

    #[test]
    fn typed_lists_reject_wrong_element_types() {
        let error = error_of("xs = List[Int](1, 2)\nxs.append(\"x\")");
        assert!(error.message.contains("expected Int, got text"), "{error}");
        assert_eq!(error.line, 2);

        let error = error_of("xs = List[Int](1, 2)\nxs[0] = \"x\"");
        assert!(error.message.contains("expected Int, got text"), "{error}");
        assert_eq!(error.line, 2);

        let error = error_of("xs = List[Int](1, 2)\nxs[0:1] = [\"x\"]");
        assert!(error.message.contains("expected Int, got text"), "{error}");
        assert_eq!(error.line, 2);

        let error = error_of("xs = List[Int](1, \"x\")");
        assert!(error.message.contains("item 1"), "{error}");
        assert!(error.message.contains("expected Int, got text"), "{error}");

        let error = error_of(
            "def total(values: List[Int]) {\n\
             \x20   return 0\n\
             }\n\
             total([1, \"x\"])",
        );
        assert!(error.message.contains("list element at index 1"), "{error}");
        assert_eq!(error.line, 4);

        let error = error_of(
            "def total(values: List[Text]) {\n\
             \x20   return 0\n\
             }\n\
             total(List[Int](1))",
        );
        assert!(
            error.message.contains("must be List[Text]"),
            "{error}"
        );

        let error = error_of("xs = List[Missing](1)");
        assert!(
            error.message.contains("unknown type annotation 'Missing'"),
            "{error}"
        );

        let error = error_of("xs = List[Int]()\nxs.append(True)");
        assert!(error.message.contains("expected Int, got bool"), "{error}");
    }

    #[test]
    fn typed_lists_support_structs_traits_and_type_parameters() {
        let env = env_after(
            "trait Named {\n\
             \x20   def name(self)\n\
             }\n\
             struct Person { name = \"\" }\n\
             impl Named for Person {\n\
             \x20   def name(self) { return self.name }\n\
             }\n\
             people = List[Named](Person(\"Ada\"), Person(\"Nirab\"))\n\
             people.append(Person(\"Grace\"))\n\
             def first_name(values: List[Person]) { return values[0].name }\n\
             first = first_name(List[Person](Person(\"Linus\")))\n\
             def any_name<T: Named>(values: List[T]) { return values[0].name() }\n\
             name = any_name(people)",
        );

        assert_eq!(
            env.get_value("first").unwrap(),
            Value::Text("Linus".into())
        );
        assert_eq!(env.get_value("name").unwrap(), Value::Text("Ada".into()));
        assert!(env
            .get_value("people")
            .unwrap()
            .to_string()
            .contains("Grace"));

        let error = error_of(
            "trait Named { def name(self) }\n\
             struct Person { name = \"\" }\n\
             impl Named for Person { def name(self) { return self.name } }\n\
             people = List[Named](Person(\"Ada\"))\n\
             people.append(1)",
        );
        assert!(
            error
                .message
                .contains("expected a value implementing trait 'Named', got int"),
            "{error}"
        );
        assert_eq!(error.line, 5);
    }

    #[test]
    fn functions_support_return_type_annotations() {
        let env = env_after(
            "def min(input: List[Int]) -> Int {\n\
             \x20   lowest = input[0]\n\
             \x20   for i in input[1:] {\n\
             \x20       if i < lowest { lowest = i }\n\
             \x20   }\n\
             \x20   lowest\n\
             }\n\
             def max(input: List[Int]) -> Int {\n\
             \x20   highest = input[0]\n\
             \x20   for i in input[1:] {\n\
             \x20       if i > highest { highest = i }\n\
             \x20   }\n\
             \x20   highest\n\
             }\n\
             def identity<T>(value: T) -> T { value }\n\
             def doubles() -> List[Int] { [1, 2, 3] }\n\
             lowest = min([4, 1, 9, -2])\n\
             highest = max([4, 1, 9, -2])\n\
             same = identity(\"text\")\n\
             values = doubles()\n\
             values.append(4)",
        );

        assert_eq!(env.get_value("lowest").unwrap(), Value::Int(-2));
        assert_eq!(env.get_value("highest").unwrap(), Value::Int(9));
        assert_eq!(
            env.get_value("same").unwrap(),
            Value::Text("text".into())
        );
        assert_eq!(env.get_value("values").unwrap().to_string(), "[1, 2, 3, 4]");
    }

    #[test]
    fn return_type_errors_are_reported() {
        let error = error_of("def bad() -> Int { \"text\" }\nbad()");
        assert!(
            error
                .message
                .contains("function 'bad' must return Int: expected Int, got text"),
            "{error}"
        );

        let error = error_of("def bad() -> Int { return \"text\" }\nbad()");
        assert!(error.message.contains("must return Int"), "{error}");

        let error = error_of("def bad() -> Int { }\nbad()");
        assert!(error.message.contains("expected Int, got None"), "{error}");

        let error = error_of("def bad() -> Missing { 1 }\nbad()");
        assert!(
            error.message.contains("unknown type annotation 'Missing'"),
            "{error}"
        );

        let error = error_of(
            "def bad() -> List[Int] { [1, \"two\"] }\n\
             bad()",
        );
        assert!(
            error
                .message
                .contains("must return List[Int]: list element at index 1"),
            "{error}"
        );
    }

    #[test]
    fn methods_support_return_type_annotations() {
        let env = env_after(
            "struct Box { value }\n\
             impl Box {\n\
             \x20   def get(self) -> Int { self.value }\n\
             \x20   def label(self) -> Text { return \"box\" }\n\
             }\n\
             trait Sized {\n\
             \x20   def size(self) -> Int\n\
             }\n\
             impl Sized for Box {\n\
             \x20   def size(self) -> Int { self.value }\n\
             }\n\
             trait Default {\n\
             \x20   def zero(self) -> Int { 0 }\n\
             }\n\
             impl Default for Box {}\n\
             b = Box(7)\n\
             number = b.get()\n\
             text = b.label()\n\
             size = b.size()\n\
             zero = b.zero()",
        );

        assert_eq!(env.get_value("number").unwrap(), Value::Int(7));
        assert_eq!(env.get_value("text").unwrap(), Value::Text("box".into()));
        assert_eq!(env.get_value("size").unwrap(), Value::Int(7));
        assert_eq!(env.get_value("zero").unwrap(), Value::Int(0));

        let error = error_of(
            "struct Box { value }\n\
             trait Sized { def size(self) -> Int }\n\
             impl Sized for Box {\n\
             \x20   def size(self) -> Text { \"big\" }\n\
             }",
        );
        assert!(
            error
                .message
                .contains("must return Int, but returns Text"),
            "{error}"
        );
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
    fn hashmap_loops_can_bind_key_and_value() {
        let env = env_after(
            "total = 0\n\
             for key, value in {1: 10, 2: 20} {\n\
             \x20   total = total + key + value\n\
             }\n\
             def min(input: Hashmap) {\n\
             \x20   lowest = input[0]\n\
             \x20   for n, i in input {\n\
             \x20       if i < lowest {\n\
             \x20           lowest = i\n\
             \x20       }\n\
             \x20   }\n\
             \x20   lowest\n\
             }\n\
             smallest = min({0: 5, 1: 3, 2: 8})",
        );

        assert_eq!(env.get_value("total").unwrap(), Value::Int(33));
        assert_eq!(env.get_value("smallest").unwrap(), Value::Int(3));

        let error = error_of("for a, b in hashSet(1) { }\nfor c, d, e in {} { }");
        assert!(
            error.message.contains("at most two loop variables"),
            "{error}"
        );
    }

    #[test]
    fn final_expression_returns_implicitly() {
        let env = env_after(
            "def double(x: Int) { x * 2 }\n\
             def do_nothing() { }\n\
             a = double(4)\n\
             b = do_nothing()",
        );

        assert_eq!(env.get_value("a").unwrap(), Value::Int(8));
        assert_eq!(env.get_value("b").unwrap(), Value::None);

        let error = error_of("for a, b in 1 { }");
        assert!(
            error
                .message
                .contains("two-variable 'for' loops need a hashmap"),
            "{error}"
        );
    }

    #[test]
    fn functions_accept_parameter_type_annotations() {
        let env = env_after(
            "def minimum(x: Int, fallback: Int = 10) {\n\
             \x20   return x\n\
             }\n\
             a = minimum(7)\n\
             b = minimum(3, 9)\n\
             def label(value: Text) { return value }\n\
             c = label(\"ok\")\n\
             def any_value(value: Any) { return value }\n\
             d = any_value(True)\n\
             def number(value: Number) { return value }\n\
             e = number(1.5)",
        );

        assert_eq!(env.get_value("a").unwrap(), Value::Int(7));
        assert_eq!(env.get_value("b").unwrap(), Value::Int(3));
        assert_eq!(env.get_value("c").unwrap(), Value::Text("ok".into()));
        assert_eq!(env.get_value("d").unwrap(), Value::Bool(true));
        assert_eq!(env.get_value("e").unwrap(), Value::Float(1.5));
    }

    #[test]
    fn parameter_type_errors_report_the_call_line() {
        let error = error_of(
            "def minimum(x: Int) {\n\
             \x20   return x\n\
             }\n\
             minimum(\"nope\")",
        );
        assert!(
            error
                .message
                .contains("argument 'x' to function 'minimum' must be Int, got text"),
            "{error}"
        );
        assert_eq!(error.line, 4);

        let error = error_of("def f(value: Missing) { return value }\nf(1)");
        assert!(
            error
                .message
                .contains("unknown type annotation 'Missing'"),
            "{error}"
        );
        assert_eq!(error.line, 2);
    }

    #[test]
    fn parameter_annotations_support_structs_traits_and_type_parameters() {
        let env = env_after(
            "trait Named {\n\
             \x20   def name(self)\n\
             }\n\
             struct Person { name = \"\" }\n\
             impl Named for Person {\n\
             \x20   def name(self) { return self.name }\n\
             }\n\
             def person_name(value: Person) { return value.name }\n\
             def named_name<T: Named>(value: T) { return value.name() }\n\
             def identity<T>(value: T) { return value }\n\
             a = person_name(Person(\"Ada\"))\n\
             b = named_name(Person(\"Nirab\"))\n\
             c = identity(5)",
        );

        assert_eq!(env.get_value("a").unwrap(), Value::Text("Ada".into()));
        assert_eq!(env.get_value("b").unwrap(), Value::Text("Nirab".into()));
        assert_eq!(env.get_value("c").unwrap(), Value::Int(5));

        let error = error_of(
            "struct Plain { x = 1 }\n\
             def f(value: Plain) { return value }\n\
             f(1)",
        );
        assert!(
            error
                .message
                .contains("must be Plain, got int"),
            "{error}"
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
    fn traits_supply_default_methods_and_dispatch() {
        let env = env_after(
            "trait Sized {\n\
             def size(self)\n\
             def double_size(self) {\n\
             return self.size() * 2\n\
             }\n\
             }\n\
             struct Box { width }\n\
             impl Sized for Box {\n\
             def size(self) {\n\
             return self.width\n\
             }\n\
             }\n\
             box = Box(4)\n\
             doubled = box.double_size()",
        );

        assert_eq!(env.get_value("doubled").unwrap(), Value::Int(8));
    }

    #[test]
    fn trait_methods_can_be_overridden_and_inherent_methods_win() {
        let env = env_after(
            "trait Label {\n\
             def label(self) {\n\
             return \"trait\"\n\
             }\n\
             }\n\
             struct Defaulted { x = 1 }\n\
             impl Label for Defaulted { }\n\
             d = Defaulted()\n\
             default_label = d.label()\n\
             struct Overridden { x = 1 }\n\
             impl Label for Overridden {\n\
             def label(self) {\n\
             return \"impl\"\n\
             }\n\
             }\n\
             o = Overridden()\n\
             impl_label = o.label()\n\
             struct Inherent { x = 1 }\n\
             impl Label for Inherent { }\n\
             impl Inherent {\n\
             def label(self) {\n\
             return \"inherent\"\n\
             }\n\
             }\n\
             i = Inherent()\n\
             inherent_label = i.label()",
        );

        assert_eq!(
            env.get_value("default_label").unwrap(),
            Value::Text("trait".into())
        );
        assert_eq!(
            env.get_value("impl_label").unwrap(),
            Value::Text("impl".into())
        );
        assert_eq!(
            env.get_value("inherent_label").unwrap(),
            Value::Text("inherent".into())
        );
    }

    #[test]
    fn trait_errors_report_the_line() {
        let error = error_of(
            "trait T { def m(self) }\n\
             struct S { x = 1 }\n\
             impl T for S { }",
        );
        assert!(
            error
                .message
                .contains("missing required method(s) for trait 'T': m"),
            "{error}"
        );
        assert_eq!(error.line, 3);

        let error = error_of(
            "trait T { def m(self) }\n\
             struct S { x = 1 }\n\
             impl T for S { def n(self) { return 1 } }",
        );
        assert!(
            error.message.contains("trait 'T' has no method 'n'"),
            "{error}"
        );
        assert_eq!(error.line, 3);

        let error = error_of(
            "trait T { def m(self, a) }\n\
             struct S { x = 1 }\n\
             impl T for S { def m(self) { return 1 } }",
        );
        assert!(
            error
                .message
                .contains("takes 2 parameter(s), but 1 given"),
            "{error}"
        );
        assert_eq!(error.line, 3);

        let error = error_of("struct S { x = 1 }\nimpl Missing for S { }");
        assert!(error.message.contains("unknown trait 'Missing'"), "{error}");
        assert_eq!(error.line, 2);
    }

    #[test]
    fn a_trait_can_only_be_implemented_once_per_struct() {
        let error = error_of(
            "trait T { def m(self) }\n\
             struct S { x = 1 }\n\
             impl T for S { def m(self) { return 1 } }\n\
             impl T for S { def m(self) { return 2 } }",
        );
        assert!(
            error
                .message
                .contains("struct 'S' already implements trait 'T'"),
            "{error}"
        );
        assert_eq!(error.line, 4);
    }

    #[test]
    fn two_traits_cannot_provide_the_same_method() {
        let error = error_of(
            "trait A { def m(self) }\n\
             trait B { def m(self) }\n\
             struct S { x = 1 }\n\
             impl A for S { def m(self) { return 1 } }\n\
             impl B for S { def m(self) { return 2 } }",
        );
        assert!(
            error
                .message
                .contains("already comes from trait 'A'"),
            "{error}"
        );
        assert_eq!(error.line, 5);
    }

    #[test]
    fn functions_can_require_trait_bounds() {
        let env = env_after(
            "trait Shape {\n\
             def area(self)\n\
             }\n\
             struct Circle { radius }\n\
             impl Shape for Circle {\n\
             def area(self) { return self.radius }\n\
             }\n\
             def total<T: Shape, U: Shape>(first, second = Circle(1)) {\n\
             return first.area() * 10 + second.area()\n\
             }\n\
             circle_total = total(Circle(2), Circle(3))\n\
             default_total = total(Circle(4))\n\
             def identity<T>(value) { return value }\n\
             plain = identity(5)",
        );

        assert_eq!(env.get_value("circle_total").unwrap(), Value::Int(23));
        assert_eq!(env.get_value("default_total").unwrap(), Value::Int(41));
        assert_eq!(env.get_value("plain").unwrap(), Value::Int(5));
    }

    #[test]
    fn function_trait_bound_errors_report_the_line() {
        let error = error_of(
            "trait Shape { def area(self) }\n\
             struct Plain { x = 1 }\n\
             def show<T: Shape>(value) { return value }\n\
             show(Plain())",
        );
        assert!(
            error
                .message
                .contains("argument 'value' to function 'show' must implement trait 'Shape'"),
            "{error}"
        );
        assert!(error.message.contains("struct 'Plain' does not"), "{error}");
        assert_eq!(error.line, 4);

        let error = error_of(
            "trait Shape { def area(self) }\n\
             def show<T: Shape>(value) { return value }\n\
             show(42)",
        );
        assert!(
            error.message.contains("must implement trait 'Shape', got int"),
            "{error}"
        );
        assert_eq!(error.line, 3);

        let error = error_of(
            "def show<T: Missing>(value) { return value }\n\
             show(1)",
        );
        assert!(
            error.message.contains("unknown trait 'Missing'"),
            "{error}"
        );
        assert_eq!(error.line, 2);
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
             unless$(False, { result = 1 })\n\
             unless$(True, { result = 2 })",
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
             twice$({ bump() })",
        );

        assert_eq!(env.get_value("counter").unwrap(), Value::Int(2));
    }

    #[test]
    fn format_macros_render_placeholders_and_escapes() {
        let render = |format: &str, args: &[Value]| {
            render_format(format, args, "println", 1).unwrap()
        };

        assert_eq!(
            render("Hello, {}!", &[Value::Text("Nirab".into())]),
            "Hello, Nirab!"
        );
        assert_eq!(
            render(
                "{} + {} = {}",
                &[Value::Int(1), Value::Int(2), Value::Int(3)]
            ),
            "1 + 2 = 3"
        );
        assert_eq!(
            render(
                "{1} then {0}",
                &[Value::Text("a".into()), Value::Text("b".into())]
            ),
            "b then a"
        );
        assert_eq!(render("{{}} and {}", &[Value::Int(7)]), "{} and 7");
        assert_eq!(
            render(
                "debug {:?}",
                &[Value::list(vec![
                    Value::Int(1),
                    Value::Text("two".into())
                ])]
            ),
            "debug [1, \"two\"]"
        );
        assert_eq!(render("no placeholders", &[]), "no placeholders");
    }

    #[test]
    fn format_macros_report_format_string_errors() {
        let error = error_of("println!(\"{}\")");
        assert!(error.message.contains("references argument 0"), "{error}");

        let error = error_of("println!(\"{}\", 1, 2)");
        assert!(
            error.message.contains("argument 2 is never used"),
            "{error}"
        );

        let error = error_of("println!(\"{x}\", 1)");
        assert!(error.message.contains("invalid argument index 'x'"), "{error}");

        let error = error_of("println!(\"{\", 1)");
        assert!(error.message.contains("unclosed '{'"), "{error}");

        let error = error_of("println!(\"}\", 1)");
        assert!(error.message.contains("single '}'"), "{error}");

        let error = error_of("println!(\"{:>5}\", 1)");
        assert!(
            error.message.contains("unsupported format specifier"),
            "{error}"
        );

        let error = error_of("println!(1)");
        assert!(error.message.contains("format string must be text"), "{error}");

        let error = error_of("foo!(1)");
        assert!(
            error.message.contains("unknown format macro 'foo!'"),
            "{error}"
        );
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

    #[test]
    fn imports_functions_structs_and_traits_from_another_file() {
        let (directory, main) = temp_project("definitions");
        fs::write(
            directory.join("lib.dsq"),
            r#"
def add(a, b) {
    return a + b
}

struct Point { x y }

trait Labeled {
    def label(self)
}

impl Labeled for Point {
    def label(self) {
        return "point"
    }
}
"#,
        )
        .unwrap();
        fs::write(
            &main,
            r#"
import "lib.dsq"

total = add(2, 3)
point = Point(4, 5)
label = point.label()
"#,
        )
        .unwrap();

        let mut env = Env::new();
        run_file_with_env(&mut env, &main).unwrap();

        assert_eq!(env.get_value("total").unwrap(), Value::Int(5));
        assert_eq!(env.get_value("label").unwrap(), Value::Text("point".into()));
    }

    #[test]
    fn extensionless_imports_share_macros_both_ways() {
        let (directory, main) = temp_project("macros");
        fs::write(
            directory.join("lib.dsq"),
            r#"
def bump() {
    inc$(value)
}

macro twice($body) {
$body
$body
}
"#,
        )
        .unwrap();
        fs::write(
            &main,
            r#"
macro inc($name) {
$name = $name + 1
}

import lib

value = 0
bump()
twice$({ bump() })
"#,
        )
        .unwrap();

        let mut env = Env::new();
        run_file_with_env(&mut env, &main).unwrap();

        // `bump()` runs once, then `twice$(...)` runs it twice more.
        assert_eq!(env.get_value("value").unwrap(), Value::Int(3));
    }

    #[test]
    fn duplicate_imports_execute_only_once() {
        let (directory, main) = temp_project("duplicate");
        fs::write(directory.join("lib.dsq"), "counter = counter + 1").unwrap();
        fs::write(
            &main,
            r#"
counter = 0
import "lib.dsq"
import "lib.dsq"
"#,
        )
        .unwrap();

        let mut env = Env::new();
        run_file_with_env(&mut env, &main).unwrap();

        assert_eq!(env.get_value("counter").unwrap(), Value::Int(1));
    }

    #[test]
    fn nested_imports_resolve_from_the_importing_files_directory() {
        let (directory, main) = temp_project("nested");
        fs::create_dir_all(directory.join("lib")).unwrap();
        fs::write(
            directory.join("lib").join("one.dsq"),
            "import \"two.dsq\"\ndef read_value() { return two_value }",
        )
        .unwrap();
        fs::write(directory.join("lib").join("two.dsq"), "two_value = 99").unwrap();
        fs::write(
            &main,
            r#"
import lib/one
result = read_value()
"#,
        )
        .unwrap();

        let mut env = Env::new();
        run_file_with_env(&mut env, &main).unwrap();

        assert_eq!(env.get_value("result").unwrap(), Value::Int(99));
    }

    #[test]
    fn cyclic_imports_report_the_importing_file() {
        let (directory, main) = temp_project("cycle");
        fs::write(directory.join("a.dsq"), "import \"b.dsq\"\nvalue = 1").unwrap();
        fs::write(directory.join("b.dsq"), "import \"a.dsq\"\nvalue = 2").unwrap();
        fs::write(&main, "import \"a.dsq\"").unwrap();

        let mut env = Env::new();
        let error = run_file_with_env(&mut env, &main).unwrap_err();

        assert!(error.message.contains("cyclic import"), "{error}");
        assert!(
            error
                .file
                .as_deref()
                .is_some_and(|file| file.ends_with("b.dsq")),
            "{error}"
        );
    }

    #[test]
    fn errors_inside_imported_functions_name_the_imported_file() {
        let (directory, main) = temp_project("function_error");
        fs::write(
            directory.join("lib.dsq"),
            "def divide(a, b) {\n    return a / b\n}",
        )
        .unwrap();
        fs::write(
            &main,
            "import \"lib.dsq\"\nresult = divide(1, 0)",
        )
        .unwrap();

        let mut env = Env::new();
        let error = run_file_with_env(&mut env, &main).unwrap_err();

        assert!(error.message.contains("division by zero"), "{error}");
        assert_eq!(error.line, 2);
        assert!(
            error
                .file
                .as_deref()
                .is_some_and(|file| file.ends_with("lib.dsq")),
            "{error}"
        );
    }

    #[test]
    fn errors_in_imported_top_level_code_name_the_file() {
        let (directory, main) = temp_project("top_level_error");
        fs::write(
            directory.join("lib.dsq"),
            "value = 1\nbroken = value / 0",
        )
        .unwrap();
        fs::write(&main, "import \"lib.dsq\"").unwrap();

        let mut env = Env::new();
        let error = run_file_with_env(&mut env, &main).unwrap_err();

        assert!(error.message.contains("division by zero"), "{error}");
        assert_eq!(error.line, 2);
        assert!(
            error
                .file
                .as_deref()
                .is_some_and(|file| file.ends_with("lib.dsq")),
            "{error}"
        );
    }

    #[test]
    fn missing_imports_report_the_import_line() {
        let (_directory, main) = temp_project("missing");
        fs::write(&main, "x = 1\nimport \"missing.dsq\"").unwrap();

        let mut env = Env::new();
        let error = run_file_with_env(&mut env, &main).unwrap_err();

        assert!(error.message.contains("cannot import"), "{error}");
        assert_eq!(error.line, 2);
        assert!(
            error
                .file
                .as_deref()
                .is_some_and(|file| file.ends_with("main.dsq")),
            "{error}"
        );
    }

    #[test]
    fn runtime_errors_in_the_main_file_name_it() {
        let (_directory, main) = temp_project("main_error");
        fs::write(&main, "x = 1\ny = x / 0").unwrap();

        let mut env = Env::new();
        let error = run_file_with_env(&mut env, &main).unwrap_err();

        assert!(error.message.contains("division by zero"), "{error}");
        assert_eq!(error.line, 2);
        assert!(
            error
                .file
                .as_deref()
                .is_some_and(|file| file.ends_with("main.dsq")),
            "{error}"
        );
    }
}
