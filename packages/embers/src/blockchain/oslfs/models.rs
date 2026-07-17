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
    pub description: Option<String>,
}

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::Oslf))]
pub struct Oslf {
    pub id: String,
    pub version: String,
    pub created_at: DateTime,
    pub name: String,
    pub description: Option<String>,
    // Reverse the Rholang string escaping the tuplespace preserves (see
    // `blockchain::common::unescape_rho_string`).
    #[serde(
        default,
        deserialize_with = "crate::blockchain::common::deserialize_unescaped_opt"
    )]
    pub query: Option<String>,
}
