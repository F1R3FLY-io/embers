use firefly_client::models::SignedCode;
use secp256k1::SecretKey;

use crate::domain::common::PreparedContract;

#[derive(Debug, Clone)]
pub struct CreateTestwalletResp {
    pub key: SecretKey,
}

#[derive(Debug, Clone)]
pub struct DeployTestReq {
    pub env: Option<String>,
    pub test: String,
}

#[derive(Debug, Clone)]
pub struct DeployTestResp {
    pub env_contract: Option<PreparedContract>,
    pub test_contract: PreparedContract,
}

#[derive(Debug, Clone)]
pub struct DeploySignedTestReq {
    pub env: Option<SignedCode>,
    pub test: SignedCode,
}

#[derive(Debug, Clone)]
pub enum LogLevel {
    Debug,
    Info,
    Error,
}

#[derive(Debug, Clone)]
pub struct Log {
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum DeploySignedTestResp {
    EnvDeployFailed { error: String },
    TestDeployFailed { error: String },
    Ok { logs: Vec<Log> },
}
