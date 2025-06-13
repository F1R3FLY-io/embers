use std::borrow::Cow;
use std::num::NonZero;

use chrono::{DateTime, Utc};
use derive_more::From;
use poem_openapi::registry::{MetaSchema, MetaSchemaRef, Registry};
use poem_openapi::types::{ParseError, ParseFromJSON, ParseResult, ToJSON, Type};
use poem_openapi::{Object, Tags};
use structural_convert::StructuralConvert;

use super::models::PreparedContract;

/// This type transforms values into [`String`] for serialization/deserialization
/// and keeps original format in `OpenApi` model.
#[derive(Debug, Clone, From)]
pub struct Stringified<T>(pub T);

impl<T> Type for Stringified<T>
where
    T: Type + Format,
{
    const IS_REQUIRED: bool = T::IS_REQUIRED;

    type RawValueType = T::RawValueType;
    type RawElementValueType = T::RawElementValueType;

    fn name() -> Cow<'static, str> {
        format!("Stringified_{}", T::name()).into()
    }

    fn schema_ref() -> MetaSchemaRef {
        T::schema_ref().merge(MetaSchema {
            format: T::format().into(),
            ..MetaSchema::ANY
        })
    }

    fn register(registry: &mut Registry) {
        T::register(registry);
    }

    fn as_raw_value(&self) -> Option<&Self::RawValueType> {
        T::as_raw_value(&self.0)
    }

    fn raw_element_iter<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = &'a Self::RawElementValueType> + 'a> {
        T::raw_element_iter(&self.0)
    }
}

trait Format {
    fn format() -> &'static str;
}

impl Format for DateTime<Utc> {
    fn format() -> &'static str {
        "datetime"
    }
}

impl ParseFromJSON for Stringified<DateTime<Utc>> {
    fn parse_from_json(value: Option<serde_json::Value>) -> ParseResult<Self> {
        let value = String::parse_from_json(value).map_err(ParseError::propagate)?;
        let seconds = value.parse::<i64>().map_err(ParseError::custom)?;
        let datetime = DateTime::<Utc>::from_timestamp(seconds, 0)
            .ok_or_else(|| ParseError::custom("invalid timestamp"))?;

        Ok(Self(datetime))
    }
}

impl ToJSON for Stringified<DateTime<Utc>> {
    fn to_json(&self) -> Option<serde_json::Value> {
        self.0.timestamp().to_string().to_json()
    }
}

impl Format for u64 {
    fn format() -> &'static str {
        "uint64"
    }
}

impl ParseFromJSON for Stringified<u64> {
    fn parse_from_json(value: Option<serde_json::Value>) -> ParseResult<Self> {
        let value = String::parse_from_json(value).map_err(ParseError::propagate)?;
        value.parse::<u64>().map(Self).map_err(ParseError::custom)
    }
}

impl ToJSON for Stringified<u64> {
    fn to_json(&self) -> Option<serde_json::Value> {
        self.0.to_string().to_json()
    }
}

impl From<NonZero<u64>> for Stringified<u64> {
    fn from(value: NonZero<u64>) -> Self {
        Self(value.get())
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Tags)]
pub enum ApiTags {
    Wallets,
    AIAgents,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(PreparedContract))]
pub struct PreparedContractDto {
    pub contract: Vec<u8>,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(into(firefly_client::models::SignedCode))]
pub struct SignedContractDto {
    pub contract: Vec<u8>,
    pub sig: Vec<u8>,
    pub sig_algorithm: String,
    pub deployer: Vec<u8>,
}
