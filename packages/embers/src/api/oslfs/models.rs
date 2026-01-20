use chrono::{DateTime, Utc};
use poem_openapi::Object;
use structural_convert::StructuralConvert;

use crate::api::common::{PreparedContract, Stringified};
use crate::domain::oslfs::models;

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::Oslfs))]
pub struct Oslfs {
    pub oslfs: Vec<Oslf>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::Oslf))]
pub struct Oslf {
    pub id: String,
    pub version: String,
    pub created_at: Stringified<DateTime<Utc>>,
    pub name: String,
    pub description: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(into(models::CreateReq))]
pub struct CreateReq {
    pub name: String,
    pub description: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::CreateResp))]
pub struct CreateResp {
    pub id: String,
    pub version: String,
    pub contract: PreparedContract,
}

pub type SaveReq = CreateReq;

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::SaveResp))]
pub struct SaveResp {
    pub version: String,
    pub contract: PreparedContract,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::DeleteResp))]
pub struct DeleteResp {
    pub contract: PreparedContract,
}
