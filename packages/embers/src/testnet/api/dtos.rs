use poem_openapi::{Enum, Object, Union};
use structural_convert::StructuralConvert;

use crate::common::api::dtos::{PreparedContract, SignedContract};
use crate::testnet::models;

#[derive(Debug, Clone, Object)]
pub struct CreateTestwalletResp {
    pub key: String,
}

impl From<models::CreateTestwalletResp> for CreateTestwalletResp {
    fn from(value: models::CreateTestwalletResp) -> Self {
        Self {
            key: value.key.display_secret().to_string(),
        }
    }
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::DeployTestResp))]
pub struct DeployTestResp {
    pub env_contract: Option<PreparedContract>,
    pub test_contract: PreparedContract,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::DeploySignedTestReq))]
pub struct DeploySignedTestReq {
    pub env: Option<SignedContract>,
    pub test: SignedContract,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::DeployTestReq))]
pub struct DeployTestReq {
    pub env: Option<String>,
    pub test: String,
}

#[derive(Debug, Clone, Object)]
pub struct EnvDeployFailed {
    pub error: String,
}

#[derive(Debug, Clone, Object)]
pub struct TestDeployFailed {
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
pub struct SignedTestDeployLogs {
    pub logs: Vec<Log>,
}

#[derive(Debug, Clone, Union)]
#[oai(one_of = true, discriminator_name = "type")]
pub enum DeploySignedTestResp {
    EnvDeployFailed(EnvDeployFailed),
    TestDeployFailed(TestDeployFailed),
    Ok(SignedTestDeployLogs),
}

impl From<models::DeploySignedTestResp> for DeploySignedTestResp {
    fn from(value: models::DeploySignedTestResp) -> Self {
        match value {
            models::DeploySignedTestResp::EnvDeployFailed { error } => {
                Self::EnvDeployFailed(EnvDeployFailed { error })
            }
            models::DeploySignedTestResp::TestDeployFailed { error } => {
                Self::TestDeployFailed(TestDeployFailed { error })
            }
            models::DeploySignedTestResp::Ok { logs } => Self::Ok(SignedTestDeployLogs {
                logs: logs.into_iter().map(Into::into).collect(),
            }),
        }
    }
}
