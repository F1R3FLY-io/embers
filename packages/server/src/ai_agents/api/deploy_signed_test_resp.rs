use poem_openapi::{Enum, Object, Union};
use structural_convert::StructuralConvert;

use crate::ai_agents::models;

#[derive(Debug, Clone, Object)]
pub struct SignedTestDeplotError {
    pub error: String,
}

#[derive(Debug, Clone, StructuralConvert, Enum)]
#[convert(from(models::LogLevel))]
#[oai(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Error,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::Log))]
pub struct Log {
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, Object)]
pub struct SignedTestDeplotLogs {
    pub logs: Vec<Log>,
}

#[derive(Debug, Clone, Union)]
pub enum DeploySignedTestResp {
    EnvDeployFailed(SignedTestDeplotError),
    TestDeployFailed(SignedTestDeplotError),
    Ok(SignedTestDeplotLogs),
}

impl From<models::DeploySignedTestResp> for DeploySignedTestResp {
    fn from(value: models::DeploySignedTestResp) -> Self {
        match value {
            models::DeploySignedTestResp::EnvDeployFailed { error } => {
                Self::EnvDeployFailed(SignedTestDeplotError { error })
            }
            models::DeploySignedTestResp::TestDeployFailed { error } => {
                Self::TestDeployFailed(SignedTestDeplotError { error })
            }
            models::DeploySignedTestResp::Ok { logs } => Self::Ok(SignedTestDeplotLogs {
                logs: logs.into_iter().map(Into::into).collect(),
            }),
        }
    }
}
