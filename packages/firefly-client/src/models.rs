use std::collections::HashMap;
use std::marker::PhantomData;

use derive_more::{AsRef, Display, From, Into};
use serde::{Deserialize, Deserializer, Serialize, de};

pub mod servicemodelapi {
    #![allow(warnings)]
    #![allow(clippy::all)]
    #![allow(clippy::pedantic)]
    #![allow(clippy::nursery)]
    tonic::include_proto!("servicemodelapi");
}

pub mod rhoapi {
    #![allow(warnings)]
    #![allow(clippy::all)]
    #![allow(clippy::pedantic)]
    #![allow(clippy::nursery)]
    tonic::include_proto!("rhoapi");
}

pub mod casper {
    #![allow(warnings)]
    #![allow(clippy::all)]
    #![allow(clippy::pedantic)]
    #![allow(clippy::nursery)]
    tonic::include_proto!("casper");

    pub mod v1 {
        tonic::include_proto!("casper.v1");
    }
}

#[derive(
    Debug, Clone, Display, PartialEq, Eq, PartialOrd, Ord, AsRef, Into, From, Serialize, Deserialize,
)]
pub struct BlockId(String);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockInfo {
    pub block_hash: BlockId,
    pub parents_hash_list: Vec<BlockId>,
}

#[derive(
    Debug, Clone, Display, PartialEq, Eq, PartialOrd, Ord, AsRef, Into, From, Serialize, Deserialize,
)]
pub struct DeployId(String);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deploy {
    pub timestamp: i64,
    pub cost: u64,
    pub term: String,
    pub sig: String,
    pub deployer: String,
    pub errored: bool,
    pub system_deploy_error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub block_info: BlockInfo,
    pub deploys: Vec<Deploy>,
}

#[derive(derive_more::Debug, Clone)]
pub struct SignedCode {
    #[debug("\"{}...\"", hex::encode(&contract[..32]))]
    pub contract: Vec<u8>,
    #[debug("{:?}", hex::encode(sig))]
    pub sig: Vec<u8>,
    pub sig_algorithm: String,
    #[debug("{:?}", hex::encode(deployer))]
    pub deployer: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize)]
pub enum ReadNodeExprUnforg {
    UnforgPrivate { data: String },
    UnforgDeploy { data: String },
    UnforgDeployer { data: String },
}

impl From<ReadNodeExprUnforg> for serde_json::Value {
    fn from(value: ReadNodeExprUnforg) -> Self {
        match value {
            ReadNodeExprUnforg::UnforgPrivate { data } => Self::String(data),
            ReadNodeExprUnforg::UnforgDeploy { data } => Self::String(data),
            ReadNodeExprUnforg::UnforgDeployer { data } => Self::String(data),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub enum ReadNodeExpr {
    ExprTuple { data: Vec<ReadNodeExpr> },
    ExprList { data: Vec<ReadNodeExpr> },
    ExprSet { data: Vec<ReadNodeExpr> },
    ExprMap { data: HashMap<String, ReadNodeExpr> },

    ExprNil {},
    ExprBool { data: bool },
    ExprInt { data: serde_json::Number },
    ExprString { data: String },
    ExprUri { data: String },
    ExprUnforg { data: ReadNodeExprUnforg },
}

impl From<ReadNodeExpr> for serde_json::Value {
    fn from(value: ReadNodeExpr) -> Self {
        match value {
            ReadNodeExpr::ExprTuple { data } => {
                Self::Array(data.into_iter().map(Into::into).collect())
            }
            ReadNodeExpr::ExprList { data } => {
                Self::Array(data.into_iter().map(Into::into).collect())
            }
            ReadNodeExpr::ExprSet { data } => {
                Self::Array(data.into_iter().map(Into::into).collect())
            }
            ReadNodeExpr::ExprMap { data } => {
                Self::Object(data.into_iter().map(|(k, v)| (k, v.into())).collect())
            }
            ReadNodeExpr::ExprNil {} => Self::Null,
            ReadNodeExpr::ExprBool { data } => Self::Bool(data),
            ReadNodeExpr::ExprInt { data } => Self::Number(data),
            ReadNodeExpr::ExprString { data } => Self::String(data),
            ReadNodeExpr::ExprUri { data } => Self::String(data),
            ReadNodeExpr::ExprUnforg { data } => data.into(),
        }
    }
}

pub enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L, R> Either<L, R> {
    pub fn to_result(self) -> Result<R, L> {
        self.into()
    }
}

impl<L, R> From<Either<L, R>> for Result<R, L> {
    fn from(value: Either<L, R>) -> Self {
        match value {
            Either::Left(err) => Err(err),
            Either::Right(v) => Ok(v),
        }
    }
}

impl<'de, L, R> Deserialize<'de> for Either<L, R>
where
    L: Deserialize<'de>,
    R: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_tuple(
            2,
            EitherVisitor {
                phantom: PhantomData,
            },
        )
    }
}

struct EitherVisitor<L, R> {
    phantom: PhantomData<(L, R)>,
}

impl<'de, L, R> de::Visitor<'de> for EitherVisitor<L, R>
where
    L: Deserialize<'de>,
    R: Deserialize<'de>,
{
    type Value = Either<L, R>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a sequence of (bool, value)")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let valid: bool = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;

        if valid {
            seq.next_element()?.map(Either::Right)
        } else {
            seq.next_element()?.map(Either::Left)
        }
        .ok_or_else(|| de::Error::invalid_length(1, &self))
    }
}
