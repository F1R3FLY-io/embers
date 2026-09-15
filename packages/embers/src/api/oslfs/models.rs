use chrono::{DateTime, Utc};
use poem_openapi::Object;
use structural_convert::StructuralConvert;

use crate::api::common::{PreparedContract, Stringified};
use crate::domain::oslfs::models;

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::Oslfs))]
pub struct Oslfs {
    /// The wallet's OSLF documents, latest version each.
    pub oslfs: Vec<OslfHeader>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::OslfHeader))]
pub struct OslfHeader {
    /// Document identity (`UUIDv4`), stable across versions.
    pub id: String,
    /// Latest version identifier (`UUIDv7`); creation-ordered by construction.
    pub version: String,
    /// When this version was prepared.
    pub created_at: Stringified<DateTime<Utc>>,
    /// Document name.
    pub name: String,
    /// Document description.
    pub description: Option<String>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::Oslf))]
pub struct Oslf {
    /// Document identity (`UUIDv4`), stable across versions.
    pub id: String,
    /// This version's identifier (`UUIDv7`).
    pub version: String,
    /// When this version was prepared.
    pub created_at: Stringified<DateTime<Utc>>,
    /// Document name.
    pub name: String,
    /// Document description.
    pub description: Option<String>,
    /// The OSLF expression body; absent when the version was written without
    /// one.
    pub query: Option<String>,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(into(models::CreateReq))]
pub struct CreateOslfReq {
    /// Document name.
    pub name: String,
    /// Document description.
    pub description: Option<String>,
    /// The OSLF expression body (from the OSLF editor).
    pub query: Option<String>,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::CreateResp))]
pub struct CreateOslfResp {
    /// The minted document identity (`UUIDv4`).
    pub id: String,
    /// The first version's identifier (`UUIDv7`).
    pub version: String,
    /// The unsigned creation contract bytes to sign.
    pub contract: PreparedContract,
}

pub type SaveOslfReq = CreateOslfReq;

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::SaveResp))]
pub struct SaveOslfResp {
    /// The appended version's identifier (`UUIDv7`).
    pub version: String,
    /// The unsigned save contract bytes to sign.
    pub contract: PreparedContract,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::DeleteResp))]
pub struct DeleteOslfResp {
    /// The unsigned deletion contract bytes to sign.
    pub contract: PreparedContract,
}
