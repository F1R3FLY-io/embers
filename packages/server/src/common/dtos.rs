use std::borrow::Cow;
use std::num::NonZero;

use chrono::{DateTime, Utc};
use derive_more::From;
use poem_openapi::payload::Json;
use poem_openapi::registry::{MetaSchema, MetaSchemaRef, Registry};
use poem_openapi::types::{
    ParseError, ParseFromJSON, ParseFromParameter, ParseResult, ToJSON, Type,
};
use poem_openapi::{ApiResponse, Object, Tags};
use structural_convert::StructuralConvert;

use super::models::PreparedContract;

/// Transforms T to [`String`] before serialization/deserialization
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

/// Parses [`String`] path parameter into T using [`TryFrom::try_from`].
#[derive(Debug, Clone, From)]
pub struct ParseFromString<T>(pub T);

impl<T> Type for ParseFromString<T>
where
    T: Send + Sync,
{
    const IS_REQUIRED: bool = String::IS_REQUIRED;

    type RawValueType = Self;
    type RawElementValueType = Self;

    fn name() -> Cow<'static, str> {
        String::name()
    }

    fn schema_ref() -> MetaSchemaRef {
        String::schema_ref()
    }

    fn register(registry: &mut Registry) {
        String::register(registry);
    }

    fn as_raw_value(&self) -> Option<&Self::RawValueType> {
        Some(self)
    }

    fn raw_element_iter<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = &'a Self::RawElementValueType> + 'a> {
        Box::new(self.as_raw_value().into_iter())
    }
}

impl<T> ParseFromParameter for ParseFromString<T>
where
    T: TryFrom<String> + Send + Sync,
    T::Error: std::fmt::Display,
{
    fn parse_from_parameter(value: &str) -> ParseResult<Self> {
        value.to_owned().try_into().map(Self).map_err(Into::into)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Tags)]
pub enum ApiTags {
    Wallets,
    AIAgents,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
pub struct InternalError {
    description: String,
}

impl From<anyhow::Error> for InternalError {
    fn from(err: anyhow::Error) -> Self {
        Self {
            description: format!("{err:?}"),
        }
    }
}

#[derive(Debug, Clone, ApiResponse)]
pub enum MaybeNotFound<T>
where
    T: Type + ToJSON + Send + Sync,
{
    #[oai(status = 200)]
    Ok(Json<T>),
    #[oai(status = 404)]
    NotFound,
    #[oai(status = 500)]
    InternalError(Json<InternalError>),
}

impl<T, K> From<Option<T>> for MaybeNotFound<K>
where
    K: Type + ToJSON + Send + Sync + From<T>,
{
    fn from(value: Option<T>) -> Self {
        value.map_or_else(|| Self::NotFound, |value| Self::Ok(Json(value.into())))
    }
}

impl<T, K> From<anyhow::Result<Option<T>>> for MaybeNotFound<K>
where
    K: Type + ToJSON + Send + Sync + From<T>,
{
    fn from(value: anyhow::Result<Option<T>>) -> Self {
        match value {
            Ok(opt) => opt.into(),
            Err(err) => Self::InternalError(Json(err.into())),
        }
    }
}

#[derive(derive_more::Debug, Clone, Object, StructuralConvert)]
#[convert(from(PreparedContract))]
pub struct PreparedContractDto {
    #[debug("\"{}...\"", hex::encode(&contract[..32]))]
    pub contract: Vec<u8>,
}

#[derive(derive_more::Debug, Clone, Object, StructuralConvert)]
#[convert(into(firefly_client::models::SignedCode))]
pub struct SignedContractDto {
    #[debug("\"{}...\"", hex::encode(&contract[..32]))]
    pub contract: Vec<u8>,
    #[debug("{:?}", hex::encode(sig))]
    pub sig: Vec<u8>,
    pub sig_algorithm: String,
    #[debug("{:?}", hex::encode(deployer))]
    pub deployer: Vec<u8>,
}
