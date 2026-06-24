//! The runtime value model used during evaluation.
//!
//! Everything a `const` produces, every loop variable, every interpolated
//! expression resolves to a `Value`. Values are deliberately small: this is a
//! build-time templating evaluator, not a general-purpose language runtime.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone)]
pub enum Value {
    Str(String),
    Number(f64),
    Bool(bool),
    Nil,
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
    /// A tagged value, e.g. a post whose `kind` is `Essay`. Used by `match`.
    /// The tag drives pattern matching; the payload carries the data.
    Tagged(String, Box<Value>),
}

impl Value {
    pub fn truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Nil => false,
            Value::Str(s) => !s.is_empty(),
            Value::Number(n) => *n != 0.0,
            Value::List(l) => !l.is_empty(),
            Value::Map(m) => !m.is_empty(),
            Value::Tagged(..) => true,
        }
    }

    /// The tag used for `match`. A bare string is its own tag, so
    /// `match post.kind { Essay => ... }` compares the pattern against the
    /// string directly. Maps fall back to their `kind` field if present, so
    /// `match post { Essay => ... }` works too. Tagged values carry an explicit
    /// tag.
    pub fn tag(&self) -> Option<String> {
        match self {
            Value::Tagged(t, _) => Some(t.clone()),
            Value::Str(s) => Some(s.clone()),
            Value::Map(m) => match m.get("kind") {
                Some(Value::Str(s)) => Some(s.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn get_field(&self, field: &str) -> Option<Value> {
        match self {
            Value::Map(m) => m.get(field).cloned(),
            Value::Tagged(_, inner) => inner.get_field(field),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Str(s) => write!(f, "{}", s),
            Value::Number(n) => {
                // Render integers without a trailing .0
                if n.fract() == 0.0 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{}", n)
                }
            }
            Value::Bool(b) => write!(f, "{}", b),
            Value::Nil => write!(f, ""),
            Value::List(_) => write!(f, "[list]"),
            Value::Map(_) => write!(f, "[map]"),
            Value::Tagged(t, _) => write!(f, "{}", t),
        }
    }
}
