// SPDX-License-Identifier: GPL-3.0-or-later

//! Run-time values and the Papyrus conversion rules.
//!
//! Strings compare case-insensitively, as the language defines `==` on
//! strings. Arrays are reference types shared between variables, so an array
//! is a counted handle to a shared vector. Objects are handles into the VM's
//! object table, and `None` is both the missing object and the missing value.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

/// A game object the VM knows about. The host decides what it stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId(pub u64);

/// The element type of an array, from the declared variable type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    Object,
    Str,
    Int,
    Float,
    Bool,
}

impl ElementKind {
    /// From a type name such as `Int[]`, `Actor[]`. Anything not a primitive
    /// is an object array.
    pub fn from_array_type(type_name: &str) -> Option<ElementKind> {
        let base = type_name.strip_suffix("[]")?;
        Some(ElementKind::from_type(base))
    }

    pub fn from_type(base: &str) -> ElementKind {
        match base.to_ascii_lowercase().as_str() {
            "int" => ElementKind::Int,
            "float" => ElementKind::Float,
            "bool" => ElementKind::Bool,
            "string" => ElementKind::Str,
            _ => ElementKind::Object,
        }
    }

    pub fn default_value(self) -> Value {
        match self {
            ElementKind::Object => Value::None,
            ElementKind::Str => Value::Str(Rc::from("")),
            ElementKind::Int => Value::Int(0),
            ElementKind::Float => Value::Float(0.0),
            ElementKind::Bool => Value::Bool(false),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ArrayRef {
    pub kind: ElementKind,
    pub items: Rc<RefCell<Vec<Value>>>,
}

impl ArrayRef {
    pub fn new(kind: ElementKind, len: usize) -> ArrayRef {
        ArrayRef {
            kind,
            items: Rc::new(RefCell::new(vec![kind.default_value(); len])),
        }
    }
    pub fn len(&self) -> usize {
        self.items.borrow().len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, i: usize) -> Option<Value> {
        self.items.borrow().get(i).cloned()
    }
    pub fn same(&self, other: &ArrayRef) -> bool {
        Rc::ptr_eq(&self.items, &other.items)
    }
}

impl PartialEq for ArrayRef {
    fn eq(&self, other: &ArrayRef) -> bool {
        self.same(other)
    }
}

#[derive(Debug, Clone, Default)]
pub enum Value {
    #[default]
    None,
    Object(ObjectId),
    Str(Rc<str>),
    Int(i32),
    Float(f32),
    Bool(bool),
    Array(ArrayRef),
}

impl Value {
    pub fn string(s: &str) -> Value {
        Value::Str(Rc::from(s))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::None => "None",
            Value::Object(_) => "Object",
            Value::Str(_) => "String",
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::Bool(_) => "Bool",
            Value::Array(_) => "Array",
        }
    }

    /// The truth rule: numbers are true when not zero, strings when not
    /// empty, objects when not None, arrays when they have elements.
    pub fn as_bool(&self) -> bool {
        match self {
            Value::None => false,
            Value::Object(_) => true,
            Value::Str(s) => !s.is_empty(),
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Bool(b) => *b,
            Value::Array(a) => !a.is_empty(),
        }
    }

    pub fn as_int(&self) -> i32 {
        match self {
            Value::Int(i) => *i,
            Value::Float(f) => *f as i32,
            Value::Bool(b) => *b as i32,
            Value::Str(s) => parse_leading_int(s),
            _ => 0,
        }
    }

    pub fn as_float(&self) -> f32 {
        match self {
            Value::Int(i) => *i as f32,
            Value::Float(f) => *f,
            Value::Bool(b) => *b as i32 as f32,
            Value::Str(s) => parse_leading_float(s),
            _ => 0.0,
        }
    }

    pub fn as_object(&self) -> Option<ObjectId> {
        match self {
            Value::Object(o) => Some(*o),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&ArrayRef> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Value::None)
    }

    /// `==` in the language. Numbers compare across int and float, strings
    /// ignore case, objects compare by handle, arrays by identity.
    pub fn equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::None, Value::None) => true,
            (Value::Object(a), Value::Object(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a.eq_ignore_ascii_case(b),
            (Value::None, Value::Object(_))
            | (Value::Object(_), Value::None)
            | (Value::Str(_), _)
            | (_, Value::Str(_)) => false,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Bool(_), _) | (_, Value::Bool(_)) => self.as_bool() == other.as_bool(),
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => a.same(b),
            (Value::Array(_) | Value::None, _) | (_, Value::Array(_) | Value::None) => false,
            _ => self.as_float() == other.as_float(),
        }
    }

    pub fn less_than(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a < b,
            (Value::Str(a), Value::Str(b)) => a.to_ascii_lowercase() < b.to_ascii_lowercase(),
            _ => self.as_float() < other.as_float(),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        self.equals(other)
    }
}

/// `as String`. Floats print with six decimals, the way the game's log shows
/// them. Objects render through the VM, which knows their names, so here they
/// print as a bare handle.
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::None => write!(f, "None"),
            Value::Object(o) => write!(f, "[Object <{}>]", o.0),
            Value::Str(s) => write!(f, "{s}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => write!(f, "{x:.6}"),
            Value::Bool(b) => write!(f, "{}", if *b { "True" } else { "False" }),
            Value::Array(a) => {
                let items = a.items.borrow();
                write!(f, "[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
        }
    }
}

fn parse_leading_int(s: &str) -> i32 {
    let t = s.trim_start();
    let end = t
        .char_indices()
        .take_while(|(i, c)| c.is_ascii_digit() || (*i == 0 && (*c == '-' || *c == '+')))
        .map(|(i, c)| i + c.len_utf8())
        .last()
        .unwrap_or(0);
    t[..end].parse().unwrap_or(0)
}

fn parse_leading_float(s: &str) -> f32 {
    let t = s.trim_start();
    let mut seen_dot = false;
    let end = t
        .char_indices()
        .take_while(|(i, c)| {
            if c.is_ascii_digit() {
                true
            } else if *c == '.' && !seen_dot {
                seen_dot = true;
                true
            } else {
                *i == 0 && (*c == '-' || *c == '+')
            }
        })
        .map(|(i, c)| i + c.len_utf8())
        .last()
        .unwrap_or(0);
    t[..end].parse().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truth_follows_the_language() {
        assert!(!Value::None.as_bool());
        assert!(Value::Object(ObjectId(1)).as_bool());
        assert!(!Value::string("").as_bool());
        assert!(Value::string("x").as_bool());
        assert!(!Value::Int(0).as_bool());
        assert!(Value::Float(0.5).as_bool());
        assert!(!Value::Array(ArrayRef::new(ElementKind::Int, 0)).as_bool());
        assert!(Value::Array(ArrayRef::new(ElementKind::Int, 2)).as_bool());
    }

    #[test]
    fn strings_compare_without_case_and_numbers_cross_types() {
        assert!(Value::string("Hello").equals(&Value::string("hELLO")));
        assert!(Value::Int(3).equals(&Value::Float(3.0)));
        assert!(!Value::string("3").equals(&Value::Int(3)));
        assert!(!Value::None.equals(&Value::Object(ObjectId(4))));
        let a = ArrayRef::new(ElementKind::Int, 1);
        let b = a.clone();
        assert!(Value::Array(a).equals(&Value::Array(b)));
    }

    #[test]
    fn casts_from_strings_take_the_leading_number() {
        assert_eq!(Value::string("42abc").as_int(), 42);
        assert_eq!(Value::string("-7").as_int(), -7);
        assert_eq!(Value::string("abc").as_int(), 0);
        assert_eq!(Value::string("1.5x").as_float(), 1.5);
        assert_eq!(Value::Float(2.9).as_int(), 2);
        assert_eq!(Value::Float(-2.9).as_int(), -2);
    }

    #[test]
    fn display_matches_the_game_log() {
        assert_eq!(Value::Float(1.5).to_string(), "1.500000");
        assert_eq!(Value::Bool(true).to_string(), "True");
        assert_eq!(Value::None.to_string(), "None");
        let a = ArrayRef::new(ElementKind::Int, 2);
        assert_eq!(Value::Array(a).to_string(), "[0, 0]");
    }
}
