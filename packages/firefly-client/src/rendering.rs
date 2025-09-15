use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub use firefly_client_macros::{IntoRhoValue, Render};
use uuid::Uuid;

use crate::models::{DeployData, DeployDataBuilder};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Value {
    Tuple(Vec<Value>),
    List(Vec<Value>),
    Set(BTreeSet<Value>),
    Map(BTreeMap<String, Value>),

    Nil,
    Bool(bool),
    Int(i64),
    String(String),
    Bytes(Vec<u8>),
    Uri(String),
    Inline(String),
}

pub trait IntoRhoValue {
    fn into_rho_value(self) -> Value;
}

impl IntoRhoValue for Value {
    fn into_rho_value(self) -> Value {
        self
    }
}

impl IntoRhoValue for bool {
    fn into_rho_value(self) -> Value {
        Value::Bool(self)
    }
}

impl IntoRhoValue for i8 {
    fn into_rho_value(self) -> Value {
        Value::Int(self as _)
    }
}

impl IntoRhoValue for i16 {
    fn into_rho_value(self) -> Value {
        Value::Int(self as _)
    }
}

impl IntoRhoValue for i32 {
    fn into_rho_value(self) -> Value {
        Value::Int(self as _)
    }
}

impl IntoRhoValue for i64 {
    fn into_rho_value(self) -> Value {
        Value::Int(self)
    }
}

impl IntoRhoValue for String {
    fn into_rho_value(self) -> Value {
        Value::String(self)
    }
}

impl IntoRhoValue for &str {
    fn into_rho_value(self) -> Value {
        self.to_string().into_rho_value()
    }
}

impl IntoRhoValue for Vec<u8> {
    fn into_rho_value(self) -> Value {
        Value::Bytes(self)
    }
}

impl IntoRhoValue for &[u8] {
    fn into_rho_value(self) -> Value {
        self.to_vec().into_rho_value()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Uri(pub String);

impl IntoRhoValue for Uri {
    fn into_rho_value(self) -> Value {
        Value::Uri(self.0)
    }
}

impl<T: IntoRhoValue> IntoRhoValue for Vec<T> {
    fn into_rho_value(self) -> Value {
        Value::List(self.into_iter().map(|item| item.into_rho_value()).collect())
    }
}

impl<T: IntoRhoValue> IntoRhoValue for BTreeSet<T> {
    fn into_rho_value(self) -> Value {
        Value::Set(self.into_iter().map(|item| item.into_rho_value()).collect())
    }
}

impl<T: IntoRhoValue> IntoRhoValue for BTreeMap<String, T> {
    fn into_rho_value(self) -> Value {
        Value::Map(
            self.into_iter()
                .map(|(k, v)| (k, v.into_rho_value()))
                .collect(),
        )
    }
}

impl<T: IntoRhoValue> IntoRhoValue for Option<T> {
    fn into_rho_value(self) -> Value {
        match self {
            Some(item) => item.into_rho_value(),
            None => Value::Nil,
        }
    }
}

impl IntoRhoValue for Uuid {
    fn into_rho_value(self) -> Value {
        self.to_string().into_rho_value()
    }
}

impl<Tz: chrono::TimeZone> IntoRhoValue for chrono::DateTime<Tz> {
    fn into_rho_value(self) -> Value {
        self.to_rfc3339().into_rho_value()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Inline(pub String);

impl IntoRhoValue for Inline {
    fn into_rho_value(self) -> Value {
        Value::Inline(self.0)
    }
}

fn escape_rho_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn display_iterable<T, F>(values: T, f: &mut fmt::Formatter<'_>, mut format: F) -> fmt::Result
where
    T: IntoIterator,
    F: FnMut(&mut fmt::Formatter<'_>, T::Item) -> fmt::Result,
{
    values
        .into_iter()
        .enumerate()
        .try_fold((), |_, (i, entry)| {
            if i > 0 {
                f.write_str(", ")?;
            }
            format(f, entry)
        })
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => f.write_str("Nil"),
            Self::Bool(b) => b.fmt(f),
            Self::Int(number) => number.fmt(f),
            Self::String(string) => write!(f, "\"{}\"", escape_rho_string(string)),
            Self::Bytes(bytes) => write!(f, "\"{}\".hexToBytes()", hex::encode(bytes)),
            Self::Uri(string) => write!(f, "`{string}`"),
            Self::Inline(string) => f.write_str(string),
            Self::Tuple(values) => {
                f.write_str("(")?;
                display_iterable(values, f, |f, entry| entry.fmt(f))?;
                f.write_str(")")
            }
            Self::List(values) => {
                f.write_str("[")?;
                display_iterable(values, f, |f, entry| entry.fmt(f))?;
                f.write_str("]")
            }
            Self::Set(values) => {
                f.write_str("Set(")?;
                display_iterable(values, f, |f, entry| entry.fmt(f))?;
                f.write_str(")")
            }
            Self::Map(map) => {
                f.write_str("{")?;
                display_iterable(map, f, |f, (k, v)| {
                    write!(f, "\"{}\"", escape_rho_string(k))?;
                    f.write_str(": ")?;
                    v.fmt(f)
                })?;
                f.write_str("}")
            }
        }
    }
}

pub trait Render: Sized {
    fn render(self) -> Result<String, _dependencies::askama::Error>;

    fn builder(self) -> Result<DeployDataBuilder, _dependencies::askama::Error> {
        self.render().map(DeployData::builder)
    }
}

pub mod _dependencies {
    pub use {askama, serde};
}
