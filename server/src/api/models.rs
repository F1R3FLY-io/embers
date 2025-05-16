use std::str::FromStr;

use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use poem_openapi::types::{ParseError, ParseFromJSON, ParseResult, ToJSON, Type};
use poem_openapi::{Enum, NewType, Object};
use structural_convert::StructuralConvert;

use crate::domain::models;

#[derive(Debug, Clone, NewType)]
#[oai(
    from_json = false,
    from_parameter = false,
    from_multipart = false,
    to_json = false,
    to_header = false
)]
pub struct Stringified<T: Type>(pub T);

impl<T> ToJSON for Stringified<T>
where
    T: Type + ToString,
{
    fn to_json(&self) -> Option<serde_json::Value> {
        self.0.to_string().to_json()
    }
}

impl<T> ParseFromJSON for Stringified<T>
where
    T: Type + FromStr,
    <T as FromStr>::Err: std::fmt::Display,
{
    fn parse_from_json(value: Option<serde_json::Value>) -> ParseResult<Self> {
        let s = String::parse_from_json(value).map_err(ParseError::propagate)?;
        s.parse::<T>().map(Self).map_err(ParseError::custom)
    }
}

impl<T: Type> From<T> for Stringified<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, NewType)]
#[oai(
    from_json = false,
    from_parameter = false,
    from_multipart = false,
    to_header = false
)]
pub struct RevAddress(pub String);

impl ParseFromJSON for RevAddress {
    fn parse_from_json(value: Option<serde_json::Value>) -> ParseResult<Self> {
        let rev_addr = String::parse_from_json(value).map_err(ParseError::propagate)?;

        let rev_bytes = bs58::decode(&rev_addr)
            .into_vec()
            .map_err(ParseError::custom)?;

        let (payload, checksum) = rev_bytes
            .split_at_checked(rev_bytes.len() - 4)
            .ok_or_else(|| ParseError::custom("invalid rev address size"))?;

        let checksum = hex::encode(checksum);

        let hash = Blake2b::<U32>::new().chain_update(payload).finalize();
        let checksum_calc = hex::encode(&hash[..4]);

        if checksum != checksum_calc {
            return Err(ParseError::custom("invalid rev address"));
        }

        Ok(Self(rev_addr))
    }
}

impl From<String> for RevAddress {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Enum, StructuralConvert)]
#[oai(rename_all = "lowercase")]
#[convert(from(models::RequestStatus))]
pub enum RequestStatus {
    Done,
    Ongoing,
    Cancelled,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::Request))]
pub struct Request {
    pub id: String,
    pub date: Stringified<u64>,
    pub amount: Stringified<u64>,
    pub status: RequestStatus,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::Exchange))]
pub struct Exchange {}

#[derive(Debug, Clone, Enum, StructuralConvert)]
#[oai(rename_all = "lowercase")]
#[convert(from(models::Direction))]
pub enum Direction {
    Incoming,
    Outgoing,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::Boost))]
pub struct Boost {
    pub id: String,
    pub username: String,
    pub direction: Direction,
    pub date: Stringified<u64>,
    pub amount: Stringified<u64>,
    pub post: String,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::Transfer))]
pub struct Transfer {
    pub id: String,
    pub direction: Direction,
    pub date: Stringified<u64>,
    pub amount: Stringified<u64>,
    pub to_address: String,
    pub cost: Stringified<u64>,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::WalletStateAndHistory))]
pub struct WalletStateAndHistory {
    pub address: String,
    pub balance: Stringified<u64>,
    pub requests: Vec<Request>,
    pub exchanges: Vec<Exchange>,
    pub boosts: Vec<Boost>,
    pub transfers: Vec<Transfer>,
}
