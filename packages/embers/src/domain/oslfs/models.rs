use chrono::{DateTime, Utc};

use crate::domain::common::PreparedContract;

#[derive(Debug, Clone)]
pub struct Oslfs {
    pub oslfs: Vec<Oslf>,
}

#[derive(Debug, Clone)]
pub struct Oslf {
    pub id: String,
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub name: String,
    pub description: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreateReq {
    pub name: String,
    pub description: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreateResp {
    pub id: String,
    pub version: String,
    pub contract: PreparedContract,
}

pub type SaveReq = CreateReq;

#[derive(Debug, Clone)]
pub struct SaveResp {
    pub version: String,
    pub contract: PreparedContract,
}

#[derive(Debug, Clone)]
pub struct DeleteResp {
    pub contract: PreparedContract,
}
