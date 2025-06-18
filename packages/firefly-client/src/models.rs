use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockInfo {
    pub block_hash: String,
    pub parents_hash_list: Vec<String>,
}

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
        }
    }
}
