use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use serde::{Deserialize, de};
use structural_convert::StructuralConvert;

use crate::ai_agents::models;

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::Agents))]
pub struct Agents {
    pub agents: Vec<AgentHeader>,
}

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::AgentHeader))]
pub struct AgentHeader {
    pub id: String,
    pub version: String,
    pub name: String,
    pub shard: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Code(String);

impl<'de> Deserialize<'de> for Code {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let bytes = BASE64_STANDARD.decode(value).map_err(de::Error::custom)?;
        String::from_utf8(bytes)
            .map(Self)
            .map_err(de::Error::custom)
    }
}

impl From<Code> for String {
    fn from(value: Code) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::Agent))]
pub struct Agent {
    pub id: String,
    pub version: String,
    pub name: String,
    pub shard: Option<String>,
    pub code: Option<Code>,
}
