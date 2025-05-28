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

#[derive(Debug, Clone)]
pub struct SignedCode {
    pub contract: Vec<u8>,
    pub sig: Vec<u8>,
    pub sig_algorithm: String,
    pub deployer: Vec<u8>,
}
