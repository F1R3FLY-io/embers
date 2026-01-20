use serde::Deserialize;
use structural_convert::StructuralConvert;

use crate::blockchain::common::DateTime;
use crate::domain::oslfs::models;

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::Oslf))]
pub struct Oslf {
    pub id: String,
    pub version: String,
    pub created_at: DateTime,
    pub name: String,
    pub description: Option<String>,
    pub query: Option<String>,
}
