use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use crc::Crc;
pub use firefly_client_macros::{IntoValue, Render};
use secp256k1::PublicKey;
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

pub trait IntoValue {
    fn into_value(self) -> Value;
}

impl IntoValue for Value {
    fn into_value(self) -> Value {
        self
    }
}

impl IntoValue for () {
    fn into_value(self) -> Value {
        Value::Tuple(Default::default())
    }
}

macro_rules! impl_into_rho_value {
    ($($ty:ident),+) => {
        #[allow(non_snake_case)]
        impl<$($ty),+> IntoValue for ($($ty,)+)
        where
            $($ty: IntoValue,)+
        {
            fn into_value(self) -> Value {
                let ($($ty,)+) = self;
                Value::Tuple(vec![
                    $(
                        $ty.into_value()
                    ),+
                ])
            }
        }
    };
}

impl_into_rho_value!(R1);
impl_into_rho_value!(R1, R2);
impl_into_rho_value!(R1, R2, R3);
impl_into_rho_value!(R1, R2, R3, R4);
impl_into_rho_value!(R1, R2, R3, R4, R5);
impl_into_rho_value!(R1, R2, R3, R4, R5, R6);
impl_into_rho_value!(R1, R2, R3, R4, R5, R6, R7);
impl_into_rho_value!(R1, R2, R3, R4, R5, R6, R7, R8);
impl_into_rho_value!(R1, R2, R3, R4, R5, R6, R7, R8, R9);

impl IntoValue for bool {
    fn into_value(self) -> Value {
        Value::Bool(self)
    }
}

impl IntoValue for i8 {
    fn into_value(self) -> Value {
        Value::Int(self.into())
    }
}

impl IntoValue for i16 {
    fn into_value(self) -> Value {
        Value::Int(self.into())
    }
}

impl IntoValue for i32 {
    fn into_value(self) -> Value {
        Value::Int(self.into())
    }
}

impl IntoValue for i64 {
    fn into_value(self) -> Value {
        Value::Int(self)
    }
}

impl IntoValue for String {
    fn into_value(self) -> Value {
        Value::String(self)
    }
}

impl IntoValue for &str {
    fn into_value(self) -> Value {
        self.to_string().into_value()
    }
}

impl IntoValue for Vec<u8> {
    fn into_value(self) -> Value {
        Value::Bytes(self)
    }
}

impl IntoValue for &[u8] {
    fn into_value(self) -> Value {
        self.to_vec().into_value()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Uri(pub String);

impl From<PublicKey> for Uri {
    fn from(value: PublicKey) -> Self {
        const CRC14: crc::Algorithm<u16> = crc::Algorithm {
            width: 14,
            poly: 0x4805,
            init: 0x0000,
            refin: false,
            refout: false,
            xorout: 0x0000,
            check: 0,
            residue: 0x0000,
        };

        let hash = Blake2b::<U32>::new()
            .chain_update(value.serialize_uncompressed())
            .finalize();

        let crc = Crc::<u16>::new(&CRC14);
        let mut digest = crc.digest();
        digest.update(&hash);
        let crc = digest.finalize().to_le_bytes();

        let full_key = [hash.as_ref(), [crc[0]].as_ref(), [crc[1] << 2].as_ref()].concat();
        let encoded = zbase32::encode(&full_key, 270);
        Self(format!("rho:id:{encoded}"))
    }
}

impl IntoValue for Uri {
    fn into_value(self) -> Value {
        Value::Uri(self.0)
    }
}

impl<T: IntoValue> IntoValue for Vec<T> {
    fn into_value(self) -> Value {
        Value::List(self.into_iter().map(IntoValue::into_value).collect())
    }
}

impl<T: IntoValue> IntoValue for BTreeSet<T> {
    fn into_value(self) -> Value {
        Value::Set(self.into_iter().map(IntoValue::into_value).collect())
    }
}

impl<T: IntoValue> IntoValue for BTreeMap<String, T> {
    fn into_value(self) -> Value {
        Value::Map(self.into_iter().map(|(k, v)| (k, v.into_value())).collect())
    }
}

impl<T: IntoValue> IntoValue for BTreeMap<&str, T> {
    fn into_value(self) -> Value {
        Value::Map(
            self.into_iter()
                .map(|(k, v)| (k.to_owned(), v.into_value()))
                .collect(),
        )
    }
}

impl<T: IntoValue> IntoValue for Option<T> {
    fn into_value(self) -> Value {
        self.map_or(Value::Nil, IntoValue::into_value)
    }
}

impl IntoValue for Uuid {
    fn into_value(self) -> Value {
        self.to_string().into_value()
    }
}

impl<Tz: chrono::TimeZone> IntoValue for chrono::DateTime<Tz> {
    fn into_value(self) -> Value {
        self.to_rfc3339().into_value()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Inline(pub String);

impl IntoValue for Inline {
    fn into_value(self) -> Value {
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
    fn render(self) -> Result<String, askama::Error>;

    fn builder(self) -> Result<DeployDataBuilder, askama::Error> {
        self.render().map(DeployData::builder)
    }
}

pub mod _dependencies {
    pub use askama;
}
