use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, StructuralConvert, Serialize, Deserialize)]
#[convert(from(models::Filesystem), into(models::Filesystem))]
#[serde(tag = "type")]
pub enum Filesystem {
    Directory(Directory),
    File(File),
}

#[derive(Debug, Clone, StructuralConvert, Serialize, Deserialize)]
#[convert(from(models::Directory), into(models::Directory))]
pub struct Directory {
    pub name: String,
    pub members: Vec<Filesystem>,
}

#[derive(Debug, Clone, StructuralConvert, Serialize, Deserialize)]
#[convert(from(models::File), into(models::File))]
pub struct File {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::Agent))]
pub struct Agent {
    pub id: String,
    pub version: String,
    pub name: String,
    pub shard: Option<String>,
    pub filesystem: Option<Directory>,
}
