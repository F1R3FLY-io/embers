use std::collections::BTreeMap;
use std::fmt::{self, Display};
use std::marker::PhantomData;

use firefly_client::models::casper::DeployDataProto;
use itertools::intersperse_with;
use prost::Message as _;
use serde::Serialize;
use serde_json::json;

use crate::common::models::PreparedContract;

pub enum RhoValue<T> {
    Nil(PhantomData<T>),
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<RhoValue<T>>),
    Object(BTreeMap<String, RhoValue<T>>),
}

impl<T> RhoValue<T> {
    pub fn from_json(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Self::Nil(PhantomData),
            serde_json::Value::Bool(b) => Self::Bool(b),
            serde_json::Value::Number(number) => Self::Number(number),
            serde_json::Value::String(string) => Self::String(string),
            serde_json::Value::Array(values) => {
                Self::Array(values.into_iter().map(Self::from_json).collect())
            }
            serde_json::Value::Object(map) => Self::Object(
                map.into_iter()
                    .map(|(k, v)| (k, Self::from_json(v)))
                    .collect(),
            ),
        }
    }
}

impl<T> From<T> for RhoValue<T>
where
    T: Serialize,
{
    fn from(value: T) -> Self {
        Self::from_json(json!(value))
    }
}

impl<T> Display for RhoValue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil(PhantomData) => "Nil".fmt(f),
            Self::Bool(b) => b.fmt(f),
            Self::Number(number) => number.fmt(f),
            Self::String(string) => fmt::Debug::fmt(string, f),
            Self::Array(values) => {
                let sep = Self::String(" ,".to_owned());

                "[".fmt(f)?;
                for entry in intersperse_with(values.iter(), || &sep) {
                    entry.fmt(f)?;
                }
                "]".fmt(f)
            }
            Self::Object(map) => {
                "{".fmt(f)?;
                for (k, v) in map {
                    k.fmt(f)?;
                    ":".fmt(f)?;
                    v.fmt(f)?;
                    ",".fmt(f)?;
                }
                "}".fmt(f)
            }
        }
    }
}

pub trait PrepareForSigning {
    fn prepare_for_signing(self) -> PreparedContract;
}

impl<T> PrepareForSigning for T
where
    T: askama::Template,
{
    fn prepare_for_signing(self) -> PreparedContract {
        let timestamp = chrono::Utc::now().timestamp_millis();
        let contract = DeployDataProto {
            term: self.render().unwrap(),
            timestamp,
            phlo_price: 1,
            phlo_limit: 500_000,
            valid_after_block_number: 0,
            shard_id: "root".into(),
            ..Default::default()
        }
        .encode_to_vec();

        PreparedContract { contract }
    }
}
