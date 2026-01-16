use chrono::{DateTime, Utc};
use poem_openapi::Object;
use structural_convert::StructuralConvert;

use crate::api::common::{PreparedContract, Stringified};

#[derive(Debug, Clone, StructuralConvert, Object)]
pub struct Oslfs {
    pub oslfs: Vec<Oslf>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
pub struct Oslf {
    pub id: String,
    pub version: String,
    pub created_at: Stringified<DateTime<Utc>>,
    pub name: String,
    pub description: Option<String>,
    pub query: String,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
pub struct CreateOslfReq {
    pub name: String,
    pub description: Option<String>,
    pub query: String,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
pub struct CreateOslfResp {
    pub id: String,
    pub version: String,
    pub contract: PreparedContract,
}

pub type SaveOslfReq = CreateOslfReq;

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
pub struct SaveOslfResp {
    pub version: String,
    pub contract: PreparedContract,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
pub struct DeleteOslfResp {
    pub contract: PreparedContract,
}
