use serde::Deserialize;
use structural_convert::StructuralConvert;

use crate::ai_agents::models;

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::LogLevel))]
pub enum LogLevel {
    Debug,
    Info,
    Error,
}

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::Log))]
pub struct Log {
    pub level: LogLevel,
    pub message: String,
}
