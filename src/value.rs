use std::{
    collections::{HashMap, HashSet},
    fmt,
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
            Value::None => Err("hashset items cannot be None".to_string()),
        }
    }
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
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Text(String),
    HashSet(HashSet<SetItem>),
    Matrix(HashMap<String, HashMap<String, Value>>),
    /// Python's `None`: the result of a call that only has side effects, and
    /// what a function returns when it falls off the end.
    None,
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Bool(_) => "bool",
            Value::Text(_) => "text",
            Value::HashSet(_) => "hashset",
            Value::Matrix(_) => "matrix",
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
            Value::HashSet(items) => {
                let mut parts: Vec<String> = items.iter().map(ToString::to_string).collect();
                parts.sort();
                if parts.is_empty() {
                    write!(f, "hashset")
                } else {
                    write!(f, "hashset {}", parts.join(" "))
                }
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
                            .map(|(key, value)| format!("{}: {value}", display_key(key)))
                            .collect();
                        cells.sort();
                        format!("{}: {{{}}}", display_key(outer), cells.join(", "))
                    })
                    .collect();
                parts.sort();
                write!(f, "mat{{{}}}", parts.join(", "))
            }
            Value::None => write!(f, "None"),
        }
    }
}
