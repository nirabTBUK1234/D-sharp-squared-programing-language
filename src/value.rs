use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fmt,
    rc::Rc,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SetItem {
    Int(i64),
    Float(u64),
    Bool(bool),
    Text(String),
}

impl fmt::Display for SetItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SetItem::Int(n) => write!(f, "{n}"),
            SetItem::Float(bits) => write!(f, "{}", f64::from_bits(*bits)),
            SetItem::Bool(b) => write!(f, "{}", bool_text(*b)),
            SetItem::Text(s) => write!(f, "\"{s}\""),
        }
    }
}

impl SetItem {
    pub fn from_value(value: Value) -> Result<SetItem, String> {
        match value {
            Value::Int(n) => Ok(SetItem::Int(n)),
            Value::Float(n) => Ok(SetItem::Float(n.to_bits())),
            Value::Bool(b) => Ok(SetItem::Bool(b)),
            Value::Text(s) => Ok(SetItem::Text(s)),
            Value::HashSet(_) => Err("hashset items cannot be another hashset".to_string()),
            Value::Matrix(_) => Err("hashset items cannot be a matrix".to_string()),
            Value::Map(_) => Err("hashset items cannot be a hashmap".to_string()),
            Value::List(_) => Err("hashset items cannot be a list".to_string()),
            Value::Struct(_) => Err("hashset items cannot be a struct".to_string()),
            Value::None => Err("hashset items cannot be None".to_string()),
        }
    }
}

/// Inside collections, text is quoted so the output looks like the literal.
fn repr_value(value: &Value) -> String {
    match value {
        Value::Text(text) => format!("\"{}\"", escape_text(text)),
        other => other.to_string(),
    }
}

fn escape_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
}

fn bool_text(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

/// Matrix keys that are not plain identifiers are printed as text literals so
/// the output can be parsed back in.
fn display_key(key: &str) -> String {
    let mut chars = key.chars();
    let starts_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || c == '_');

    if starts_ok && rest_ok {
        key.to_string()
    } else {
        format!("\"{key}\"")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructValue {
    pub type_name: String,
    pub fields: HashMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Text(String),
    /// `[1, 2, 3]`: a Python-style mutable list. The `Rc<RefCell<...>>`
    /// makes assignment and method calls share the same list, like Python.
    List(Rc<RefCell<Vec<Value>>>),
    HashSet(HashSet<SetItem>),
    Matrix(HashMap<String, HashMap<String, Value>>),
    /// `{"a": 1}`: Python-style dictionary keyed by hashable scalars.
    Map(HashMap<SetItem, Value>),
    /// `Point(1, 2)`: a fixed set of named fields, created by `struct`.
    Struct(StructValue),
    /// Python's `None`: the result of a call that only has side effects, and
    /// what a function returns when it falls off the end.
    None,
}

impl Value {
    pub fn list(items: Vec<Value>) -> Value {
        Value::List(Rc::new(RefCell::new(items)))
    }

    pub fn type_name(&self) -> &str {
        match self {
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Bool(_) => "bool",
            Value::Text(_) => "text",
            Value::List(_) => "list",
            Value::HashSet(_) => "hashset",
            Value::Matrix(_) => "matrix",
            Value::Map(_) => "hashmap",
            Value::Struct(value) => &value.type_name,
            Value::None => "None",
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Float(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{}", bool_text(*b)),
            Value::Text(s) => write!(f, "{s}"),
            Value::List(items) => {
                let parts: Vec<String> = items.borrow().iter().map(repr_value).collect();
                write!(f, "[{}]", parts.join(", "))
            }
            Value::HashSet(items) => {
                let mut parts: Vec<String> = items.iter().map(ToString::to_string).collect();
                parts.sort();
                write!(f, "hashSet({})", parts.join(", "))
            }
            Value::Matrix(rows) => {
                if rows.is_empty() {
                    return write!(f, "mat{{}}");
                }
                let mut parts: Vec<String> = rows
                    .iter()
                    .map(|(outer, inner)| {
                        let mut cells: Vec<String> = inner
                            .iter()
                            .map(|(key, value)| {
                                format!("{}: {}", display_key(key), repr_value(value))
                            })
                            .collect();
                        cells.sort();
                        format!("{}: {{{}}}", display_key(outer), cells.join(", "))
                    })
                    .collect();
                parts.sort();
                write!(f, "mat{{{}}}", parts.join(", "))
            }
            Value::Map(entries) => {
                let mut parts: Vec<String> = entries
                    .iter()
                    .map(|(key, value)| format!("{key}: {}", repr_value(value)))
                    .collect();
                parts.sort();
                write!(f, "{{{}}}", parts.join(", "))
            }
            Value::Struct(value) => {
                let mut fields: Vec<String> = value
                    .fields
                    .iter()
                    .map(|(name, field)| format!("{name}={}", repr_value(field)))
                    .collect();
                fields.sort();
                write!(f, "{}({})", value.type_name, fields.join(", "))
            }
            Value::None => write!(f, "None"),
        }
    }
}
