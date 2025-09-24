use std::borrow::Cow;

use chrono::{DateTime, Utc};
use derive_more::From;
use firefly_client::helpers::ShortHex;
use poem_openapi::payload::Json;
use poem_openapi::registry::{MetaSchema, MetaSchemaRef, Registry};
use poem_openapi::types::{
    Base64,
    ParseError,
    ParseFromJSON,
    ParseFromParameter,
    ParseResult,
    ToJSON,
    Type,
};
use poem_openapi::{ApiResponse, NewType, Object, Tags};

use crate::ai_agents_teams::models::Graph;
use crate::common::models::{self, PositiveNonZero, WalletAddress};

impl<T> Type for PositiveNonZero<T>
where
    T: Type + Format,
{
    const IS_REQUIRED: bool = T::IS_REQUIRED;

    type RawValueType = T::RawValueType;
    type RawElementValueType = T::RawElementValueType;

    fn name() -> Cow<'static, str> {
        format!("PositiveNonZero_{}", T::name()).into()
    }

    fn schema_ref() -> MetaSchemaRef {
        T::schema_ref()
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
        MetaSchemaRef::Inline(Box::new(MetaSchema::new_with_format("string", T::format())))
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
        "unix-timestamp"
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

impl Format for i64 {
    fn format() -> &'static str {
        "int64"
    }
}

impl<T> Format for PositiveNonZero<T>
where
    T: Format,
{
    fn format() -> &'static str {
        T::format()
    }
}

impl ParseFromJSON for Stringified<PositiveNonZero<i64>> {
    fn parse_from_json(value: Option<serde_json::Value>) -> ParseResult<Self> {
        let value = String::parse_from_json(value).map_err(ParseError::propagate)?;
        let number = value.parse::<i64>().map_err(ParseError::custom)?;
        number.try_into().map(Self).map_err(ParseError::custom)
    }
}

impl ToJSON for Stringified<PositiveNonZero<i64>> {
    fn to_json(&self) -> Option<serde_json::Value> {
        self.0.0.to_string().to_json()
    }
}

impl Format for WalletAddress {
    fn format() -> &'static str {
        "blockchain-address"
    }
}

impl From<Stringified<WalletAddress>> for WalletAddress {
    fn from(value: Stringified<WalletAddress>) -> Self {
        value.0
    }
}

impl Type for WalletAddress {
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

impl ParseFromParameter for Stringified<WalletAddress> {
    fn parse_from_parameter(value: &str) -> ParseResult<Self> {
        value.to_owned().try_into().map(Self).map_err(Into::into)
    }
}

impl ParseFromJSON for Stringified<WalletAddress> {
    fn parse_from_json(value: Option<serde_json::Value>) -> ParseResult<Self> {
        let value = String::parse_from_json(value).map_err(ParseError::propagate)?;
        value.try_into().map(Self).map_err(Into::into)
    }
}

impl ToJSON for Stringified<WalletAddress> {
    fn to_json(&self) -> Option<serde_json::Value> {
        self.0.as_ref().to_json()
    }
}

impl Format for Graph {
    fn format() -> &'static str {
        "graphl"
    }
}

impl From<Stringified<Graph>> for Graph {
    fn from(value: Stringified<Graph>) -> Self {
        value.0
    }
}

impl Type for Graph {
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

impl ParseFromParameter for Stringified<Graph> {
    fn parse_from_parameter(value: &str) -> ParseResult<Self> {
        Graph::new(value.to_owned()).map(Self).map_err(Into::into)
    }
}

impl ParseFromJSON for Stringified<Graph> {
    fn parse_from_json(value: Option<serde_json::Value>) -> ParseResult<Self> {
        let value = String::parse_from_json(value).map_err(ParseError::propagate)?;
        Graph::new(value).map(Self).map_err(Into::into)
    }
}

impl ToJSON for Stringified<Graph> {
    fn to_json(&self) -> Option<serde_json::Value> {
        self.0.clone().to_graphl().to_json()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Tags)]
pub enum ApiTags {
    Testnet,
    Wallets,
    AIAgents,
    AIAgentsTeams,
    Service,
}

#[derive(Debug, Clone, Object)]
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

#[derive(derive_more::Debug, Clone, NewType)]
#[oai(to_header = false, from_multipart = false)]
#[debug("{:?}", _0.0.short_hex(32))]
pub struct PreparedContract(pub Base64<Vec<u8>>);

impl From<models::PreparedContract> for PreparedContract {
    fn from(value: models::PreparedContract) -> Self {
        Self(Base64(value.0))
    }
}

#[derive(derive_more::Debug, Clone, Object)]
pub struct SignedContract {
    #[debug("{:?}", contract.0.short_hex(32))]
    pub contract: Base64<Vec<u8>>,
    #[debug("{:?}", hex::encode(&sig.0))]
    pub sig: Base64<Vec<u8>>,
    pub sig_algorithm: String,
    #[debug("{:?}", hex::encode(&deployer.0))]
    pub deployer: Base64<Vec<u8>>,
}

impl From<SignedContract> for firefly_client::models::SignedCode {
    fn from(value: SignedContract) -> Self {
        Self {
            contract: value.contract.0,
            sig: value.sig.0,
            sig_algorithm: value.sig_algorithm,
            deployer: value.deployer.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestNet<T>(pub T);
