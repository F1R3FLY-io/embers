use serde::Deserialize;
use structural_convert::StructuralConvert;

use crate::blockchain::common::DateTime;
use crate::domain::oslfs::models;

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::OslfHeader))]
pub struct OslfHeader {
    pub id: String,
    pub version: String,
    pub created_at: DateTime,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::Oslf))]
pub struct Oslf {
    pub id: String,
    pub version: String,
    pub created_at: DateTime,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
}
