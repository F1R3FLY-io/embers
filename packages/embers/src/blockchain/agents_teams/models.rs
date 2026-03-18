use derive_more::Into;
use serde::{Deserialize, Serialize, de};
use structural_convert::StructuralConvert;

use crate::blockchain::common::{DateTime, Hex, Uri};
use crate::domain::agents_teams::models;

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::AgentsTeams))]
pub struct AgentsTeams {
    pub agents_teams: Vec<AgentsTeamHeader>,
}

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::AgentsTeamHeader))]
pub struct AgentsTeamHeader {
    pub id: String,
    #[serde(default)]
    pub version: String,
    pub created_at: DateTime,
    #[serde(default)]
    pub last_deploy: Option<DateTime>,
    #[serde(default)]
    pub uri: Option<Uri>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub shard: Option<String>,
    #[serde(default)]
    pub logo: Option<String>,
}

#[derive(Debug, Clone, Into)]
pub struct Graph(models::Graph);

impl<'de> Deserialize<'de> for Graph {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let graphl = String::deserialize(deserializer)?;
        tracing::debug!("graphl raw (first 200): {}", &graphl[..graphl.len().min(200)]);
        // Rholang string storage adds escape sequences for quotes.
        // Unescape: \" -> " and \\ -> \
        let graphl = graphl.replace("\\\"", "\"").replace("\\\\", "\\");
        tracing::debug!("graphl unescaped (first 200): {}", &graphl[..graphl.len().min(200)]);
        models::Graph::new(graphl)
            .map(Self)
            .map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::AgentsTeam))]
pub struct AgentsTeam {
    pub id: String,
    #[serde(default)]
    pub version: String,
    pub created_at: DateTime,
    #[serde(default)]
    pub last_deploy: Option<DateTime>,
    #[serde(default)]
    pub uri: Option<Uri>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub shard: Option<String>,
    #[serde(default)]
    pub logo: Option<String>,
    #[serde(default)]
    pub graph: Option<Graph>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FireskyCredentials {
    pub uri: String,
    pub pds_url: String,
    pub email: String,
    pub token: String,
}

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::EncryptedMsg))]
pub struct EncryptedMsg {
    pub ciphertext: Hex,
    pub nonce: Hex,
}
