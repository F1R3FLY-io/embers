use poem_openapi::{Enum, Object, Union};
use structural_convert::StructuralConvert;

use crate::api::common::{PreparedContract, SignedContract};
use crate::domain::testnet::models;

#[derive(Debug, Clone, Object)]
pub struct CreateTestwalletResp {
    /// The new wallet's secp256k1 private key, hex-encoded.
    pub key: String,
}

impl From<models::CreateTestwalletResp> for CreateTestwalletResp {
    fn from(value: models::CreateTestwalletResp) -> Self {
        Self {
            key: value.key.display_secret().to_string(),
        }
    }
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::DeployTestResp))]
pub struct DeployTestResp {
    /// Unsigned environment contract bytes; present only when `env` was
    /// supplied.
    pub env_contract: Option<PreparedContract>,
    /// Unsigned test contract bytes to sign.
    pub test_contract: PreparedContract,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::DeploySignedTestReq))]
pub struct DeploySignedTestReq {
    /// The signed environment contract; present exactly when prepare
    /// returned `env_contract`.
    pub env: Option<SignedContract>,
    /// The signed test contract.
    pub test: SignedContract,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(into(models::DeployTestReq))]
pub struct DeployTestReq {
    /// Rholang source of the environment (code under test).
    pub env: Option<String>,
    /// Rholang source of the test contract.
    pub test: String,
}

/// The environment deploy was rejected; the test was never deployed.
#[derive(Debug, Clone, Object)]
pub struct EnvDeployFailed {
    /// The rejection reported for the environment deploy.
    pub error: String,
}

/// The environment (if any) deployed, but the test deploy was rejected.
#[derive(Debug, Clone, Object)]
pub struct TestDeployFailed {
    /// The rejection reported for the test deploy.
    pub error: String,
}

/// Severity a test records on the on-chain log contract.
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
    /// Severity recorded by the test.
    pub level: LogLevel,
    /// The log line.
    pub message: String,
}

/// Both deploys landed, the test finalized, and its logs were read back.
#[derive(Debug, Clone, Object)]
pub struct SignedTestDeployLogs {
    /// The messages the test sent to the on-chain log contract, in order.
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
