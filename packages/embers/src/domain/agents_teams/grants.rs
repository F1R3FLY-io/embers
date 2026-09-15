//! Channel-grant lifecycle: the standing, revocable, on-chain delegations that
//! let a channel service (chat gateway, mail bridge) initiate runs OWNED by
//! the granting user while the channel wallet signs and pays the deploys.
//!
//! Grants are created and revoked through the platform's signed two-phase
//! protocol, always signed by the grantor in the environment where their key
//! lives (the browser wallet). The grantor identity is derived on chain from
//! the creating deploy's signer, never accepted as input, so a grant can only
//! ever bind its author. Records are public authorization facts; the one
//! secret involved -- the optional subject bearer token -- is minted here,
//! returned exactly once in the prepare response, and stored on chain only as
//! a blake2b-256 hex digest.
//!
//! Adjudication is the chain's job: `beginDurableRunUnderGrant`,
//! `claimOutputReveal`, and `claimCommand` re-check the grant at claim time.
//! Everything this module reads is therefore advisory routing state; the
//! deploy-time checks are what actually protect the grantor.

use anyhow::anyhow;
use blake2::{Blake2b, Digest};
use blake2::digest::consts::U32;
use firefly_client::models::{DeployId, Uri};
use firefly_client::rendering::{IntoValue, Render};
use firefly_client::{NodeEventSource, ReadNode, WriteNode};
use secp256k1::rand::RngCore;

use super::AgentsTeamsService;
use super::models::{
    CreateGrantReq,
    CreateGrantResp,
    DeploySignedCreateGrantReq,
    DeploySignedRevokeGrantReq,
    Grant,
    RevokeGrantReq,
    RevokeGrantResp,
};
use crate::blockchain;
use crate::domain::common::{prepare_for_signing, record_trace, utc_now_protocol};

/// Command classes a grantor may delegate to a channel. `timerFired` belongs
/// to the service scheduler and `migrate` is administrative, so neither is
/// delegable to a channel identity.
pub const DELEGABLE_COMMAND_CLASSES: [&str; 6] =
    ["approve", "reject", "cancel", "signal", "event", "retry"];

/// Channels a grant may name. The gateway supplies opaque author-hash
/// principals under "chat"; the mail bridge supplies sender addresses under
/// "mail". Platform disambiguation for chat lives inside the principal hash,
/// which is already platform- and workspace-scoped.
pub const GRANT_CHANNELS: [&str; 2] = ["chat", "mail"];

/// Sender-authentication floors a mail grant may require. Enforcement is the
/// channel's duty (embers cannot see SMTP-time verdicts); the schema exists
/// now so grants never need migration when the mail bridge starts enforcing.
pub const MIN_AUTH_LEVELS: [&str; 4] = ["none", "spf", "dkim", "dmarc"];

#[derive(Debug, Clone, IntoValue)]
struct GrantCapsValue {
    max_runs_per_day: Option<i64>,
}

#[derive(Debug, Clone, IntoValue)]
struct GrantAuthorityValue {
    reveal: bool,
    commands: Vec<String>,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/create_grant.rho")]
struct CreateGrant {
    env_uri: Uri,
    grant_id: String,
    grantee: String,
    channel: String,
    channel_principal: String,
    agents_team: Uri,
    caps: GrantCapsValue,
    authority: GrantAuthorityValue,
    require_token_hash: Option<String>,
    min_auth: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/revoke_grant.rho")]
struct RevokeGrant {
    env_uri: Uri,
    grant_id: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/get_grant.rho")]
struct GetGrant {
    env_uri: Uri,
    grant_id: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/get_grants_by_grantor.rho")]
struct GetGrantsByGrantor {
    env_uri: Uri,
    grantor: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/get_grants_by_grantee.rho")]
struct GetGrantsByGrantee {
    env_uri: Uri,
    grantee: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/get_grants_for_team.rho")]
struct GetGrantsForTeam {
    env_uri: Uri,
    agents_team: Uri,
}

#[derive(Debug)]
pub enum GrantError {
    /// No grant record exists under the id.
    NotFound,
    /// The mutation deploy was rejected on chain and the surviving record
    /// proves the remaining abort cause: the signer is not the grantor.
    Unauthorized,
    /// The request named a channel, authority class, or auth level outside
    /// the legal vocabulary.
    InvalidRequest(String),
    Internal(anyhow::Error),
}

impl std::fmt::Display for GrantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "grant not found"),
            Self::Unauthorized => write!(f, "the signer does not hold grant authority"),
            Self::InvalidRequest(reason) => write!(f, "{reason}"),
            Self::Internal(err) => write!(f, "{err:?}"),
        }
    }
}

impl std::error::Error for GrantError {}

impl From<anyhow::Error> for GrantError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal(err)
    }
}

fn blake2b256_hex(bytes: &[u8]) -> String {
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Mint a subject bearer token: 32 bytes from the process CSPRNG, hex-encoded.
/// The plaintext leaves this function exactly once, inside the prepare
/// response; only its digest is ever stored.
fn mint_subject_token() -> String {
    let mut bytes = [0u8; 32];
    secp256k1::rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn validate_create_grant(request: &CreateGrantReq) -> Result<(), GrantError> {
    if !GRANT_CHANNELS.contains(&request.channel.as_str()) {
        return Err(GrantError::InvalidRequest(format!(
            "unknown grant channel {:?}; legal channels: {GRANT_CHANNELS:?}",
            request.channel
        )));
    }
    if !MIN_AUTH_LEVELS.contains(&request.min_auth.as_str()) {
        return Err(GrantError::InvalidRequest(format!(
            "unknown min_auth level {:?}; legal levels: {MIN_AUTH_LEVELS:?}",
            request.min_auth
        )));
    }
    for class in &request.authority.commands {
        if !DELEGABLE_COMMAND_CLASSES.contains(&class.as_str()) {
            return Err(GrantError::InvalidRequest(format!(
                "command class {class:?} is not delegable; legal classes: {DELEGABLE_COMMAND_CLASSES:?}"
            )));
        }
    }
    if request.channel_principal.is_empty() {
        return Err(GrantError::InvalidRequest(
            "channel principal must not be empty".to_owned(),
        ));
    }
    if request.grantee.is_empty() {
        return Err(GrantError::InvalidRequest(
            "grantee address must not be empty".to_owned(),
        ));
    }
    if let Some(cap) = request.caps.max_runs_per_day
        && cap <= 0
    {
        return Err(GrantError::InvalidRequest(
            "max_runs_per_day must be positive when present".to_owned(),
        ));
    }
    Ok(())
}

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_create_grant(
        &self,
        request: CreateGrantReq,
    ) -> Result<CreateGrantResp, GrantError> {
        record_trace!(request);
        validate_create_grant(&request)?;

        let grant_id = uuid::Uuid::now_v7().to_string();
        let issued_at = utc_now_protocol();
        let require_token = request.require_token.then(mint_subject_token);
        let require_token_hash = require_token
            .as_deref()
            .map(|token| blake2b256_hex(token.as_bytes()));

        let code = CreateGrant {
            env_uri: self.uri.clone(),
            grant_id: grant_id.clone(),
            grantee: request.grantee.clone(),
            channel: request.channel.clone(),
            channel_principal: request.channel_principal.clone(),
            agents_team: request.agents_team.clone(),
            caps: GrantCapsValue {
                max_runs_per_day: request.caps.max_runs_per_day,
            },
            authority: GrantAuthorityValue {
                reveal: request.authority.reveal,
                commands: request.authority.commands.clone(),
            },
            require_token_hash,
            min_auth: request.min_auth.clone(),
        }
        .render()
        .map_err(anyhow::Error::from)?;

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(CreateGrantResp {
            grant_id,
            contract: prepare_for_signing()
                .code(code)
                .valid_after_block_number(valid_after)
                .call(),
            issued_at,
            require_token,
        })
    }

    #[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
    pub async fn deploy_signed_create_grant(
        &self,
        request: DeploySignedCreateGrantReq,
    ) -> Result<DeployId, GrantError> {
        let grant_id = request.prepare_response.grant_id.clone();
        let deploy_id = self
            .write_client
            .clone()
            .deploy_signed_contract(request.contract)
            .await
            .map_err(anyhow::Error::from)?;
        match self
            .wait_for_deploy_finalization(&deploy_id, self.deploy_finalization_timeout)
            .await
        {
            Some(false) => {}
            Some(true) => {
                // The clause aborted. A duplicate grant id is the only abort
                // this clause can raise for a well-formed prepare; surfacing
                // it as a conflict keeps re-prepared attempts working.
                return Err(GrantError::Internal(anyhow!(
                    "grant creation deploy {deploy_id} was rejected on chain (grant identity conflict)"
                )));
            }
            None => {
                return Err(GrantError::Internal(anyhow!(
                    "grant creation deploy {deploy_id} was not finalized"
                )));
            }
        }

        let acknowledged: bool = self
            .read_deploy_result(deploy_id.clone())
            .await
            .map_err(|err| GrantError::Internal(err.context("grant creation acknowledgment")))?;
        if !acknowledged {
            return Err(GrantError::Internal(anyhow!(
                "grant creation was not acknowledged on chain"
            )));
        }

        // Registration readback: the grant must be observable before the
        // caller is told it exists, mirroring the run-registration discipline.
        match self.get_grant(grant_id.clone()).await? {
            Some(_) => Ok(deploy_id),
            None => Err(GrantError::Internal(anyhow!(
                "grant {grant_id} was acknowledged but is not observable"
            ))),
        }
    }

    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_revoke_grant(
        &self,
        request: RevokeGrantReq,
    ) -> Result<RevokeGrantResp, GrantError> {
        record_trace!(request);

        // Fail fast on unknown grants; the chain re-checks at claim time.
        self.get_grant(request.grant_id.clone())
            .await?
            .ok_or(GrantError::NotFound)?;

        let code = RevokeGrant {
            env_uri: self.uri.clone(),
            grant_id: request.grant_id.clone(),
        }
        .render()
        .map_err(anyhow::Error::from)?;
        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(RevokeGrantResp {
            contract: prepare_for_signing()
                .code(code)
                .valid_after_block_number(valid_after)
                .call(),
        })
    }

    #[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
    pub async fn deploy_signed_revoke_grant(
        &self,
        request: DeploySignedRevokeGrantReq,
    ) -> Result<DeployId, GrantError> {
        let grant_id = request.prepare_request.grant_id.clone();
        let deploy_id = self
            .write_client
            .clone()
            .deploy_signed_contract(request.contract)
            .await
            .map_err(anyhow::Error::from)?;
        match self
            .wait_for_deploy_finalization(&deploy_id, self.deploy_finalization_timeout)
            .await
        {
            Some(false) => {}
            Some(true) => {
                // The clause aborted. Re-read the record to name the cause
                // honestly: absence is knowable without authorization, and a
                // surviving unrevoked record proves the signer was not the
                // grantor (revoking an already-revoked grant acknowledges
                // instead of aborting).
                return Err(match self.get_grant(grant_id.clone()).await? {
                    None => GrantError::NotFound,
                    Some(_) => GrantError::Unauthorized,
                });
            }
            None => {
                return Err(GrantError::Internal(anyhow!(
                    "grant revocation deploy {deploy_id} was not finalized"
                )));
            }
        }

        let acknowledged: bool = self
            .read_deploy_result(deploy_id.clone())
            .await
            .map_err(|err| GrantError::Internal(err.context("grant revocation acknowledgment")))?;
        if !acknowledged {
            return Err(GrantError::Internal(anyhow!(
                "grant revocation was not acknowledged on chain"
            )));
        }

        // Revocation readback: disclosure authority must be observably cut
        // before the grantor is told it is.
        match self.get_grant(grant_id.clone()).await? {
            Some(grant) if grant.revoked => Ok(deploy_id),
            Some(_) => Err(GrantError::Internal(anyhow!(
                "grant {grant_id} revocation was acknowledged but is not observable"
            ))),
            None => Err(GrantError::Internal(anyhow!(
                "grant {grant_id} disappeared after revocation"
            ))),
        }
    }

    pub async fn get_grant(&self, grant_id: String) -> anyhow::Result<Option<Grant>> {
        let code = GetGrant {
            env_uri: self.uri.clone(),
            grant_id,
        }
        .render()?;
        let grant: Option<blockchain::agents_teams::models::Grant> = self
            .read_client
            .get_data_or_none(code)
            .await?
            .flatten();
        Ok(grant.map(Into::into))
    }

    pub async fn list_grants_by_grantor(&self, grantor: String) -> anyhow::Result<Vec<Grant>> {
        let code = GetGrantsByGrantor {
            env_uri: self.uri.clone(),
            grantor,
        }
        .render()?;
        let grants: Vec<blockchain::agents_teams::models::Grant> =
            self.read_client.get_data(code).await?;
        Ok(grants.into_iter().map(Into::into).collect())
    }

    pub async fn list_grants_by_grantee(&self, grantee: String) -> anyhow::Result<Vec<Grant>> {
        let code = GetGrantsByGrantee {
            env_uri: self.uri.clone(),
            grantee,
        }
        .render()?;
        let grants: Vec<blockchain::agents_teams::models::Grant> =
            self.read_client.get_data(code).await?;
        Ok(grants.into_iter().map(Into::into).collect())
    }

    pub async fn list_grants_for_team(&self, agents_team: Uri) -> anyhow::Result<Vec<Grant>> {
        let code = GetGrantsForTeam {
            env_uri: self.uri.clone(),
            agents_team,
        }
        .render()?;
        let grants: Vec<blockchain::agents_teams::models::Grant> =
            self.read_client.get_data(code).await?;
        Ok(grants.into_iter().map(Into::into).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::agents_teams::models::{GrantAuthority, GrantCaps};

    fn create_request() -> CreateGrantReq {
        CreateGrantReq {
            grantee: "1111AbCdEf".to_owned(),
            channel: "chat".to_owned(),
            channel_principal: "a3f9deadbeef".to_owned(),
            agents_team: crate::domain::test_helpers::test_uri(),
            caps: GrantCaps {
                max_runs_per_day: Some(5),
            },
            authority: GrantAuthority {
                reveal: true,
                commands: vec!["approve".to_owned(), "cancel".to_owned()],
            },
            require_token: false,
            min_auth: "none".to_owned(),
        }
    }

    #[test]
    fn create_grant_template_renders_caps_authority_and_ack_forward() {
        let code = CreateGrant {
            env_uri: crate::domain::test_helpers::test_uri(),
            grant_id: "grant-1".to_owned(),
            grantee: "1111AbCdEf".to_owned(),
            channel: "chat".to_owned(),
            channel_principal: "a3f9".to_owned(),
            agents_team: crate::domain::test_helpers::test_uri(),
            caps: GrantCapsValue {
                max_runs_per_day: Some(5),
            },
            authority: GrantAuthorityValue {
                reveal: true,
                commands: vec!["approve".to_owned()],
            },
            require_token_hash: None,
            min_auth: "none".to_owned(),
        }
        .render()
        .expect("create grant template should render");

        assert!(code.contains("createGrant"));
        assert!(code.contains("{\"max_runs_per_day\": 5}"));
        assert!(code.contains("\"reveal\": true"));
        assert!(code.contains("[\"approve\"]"));
        assert!(code.contains("Nil"));
        assert!(code.contains("forward!(ack, (*deployId).toString())"));
    }

    #[test]
    fn uncapped_grant_renders_nil_cap() {
        let code = CreateGrant {
            env_uri: crate::domain::test_helpers::test_uri(),
            grant_id: "grant-1".to_owned(),
            grantee: "1111AbCdEf".to_owned(),
            channel: "mail".to_owned(),
            channel_principal: "sender@example.test".to_owned(),
            agents_team: crate::domain::test_helpers::test_uri(),
            caps: GrantCapsValue {
                max_runs_per_day: None,
            },
            authority: GrantAuthorityValue {
                reveal: true,
                commands: Vec::new(),
            },
            require_token_hash: Some("aa".repeat(32)),
            min_auth: "dkim".to_owned(),
        }
        .render()
        .expect("create grant template should render");

        assert!(code.contains("{\"max_runs_per_day\": Nil}"));
        assert!(code.contains("\"commands\": []"));
        assert!(code.contains("\"dkim\""));
    }

    #[test]
    fn unknown_channel_is_rejected_before_any_chain_traffic() {
        let mut request = create_request();
        request.channel = "carrier-pigeon".to_owned();
        let error = validate_create_grant(&request).expect_err("channel must be validated");
        assert!(matches!(error, GrantError::InvalidRequest(_)));
    }

    #[test]
    fn non_delegable_command_class_is_rejected() {
        let mut request = create_request();
        request.authority.commands = vec!["migrate".to_owned()];
        let error = validate_create_grant(&request).expect_err("migrate must not be delegable");
        assert!(matches!(error, GrantError::InvalidRequest(_)));
    }

    #[test]
    fn non_positive_cap_is_rejected() {
        let mut request = create_request();
        request.caps.max_runs_per_day = Some(0);
        let error = validate_create_grant(&request).expect_err("zero cap must be rejected");
        assert!(matches!(error, GrantError::InvalidRequest(_)));
    }

    #[test]
    fn minted_tokens_are_distinct_and_hash_stably() {
        let token_a = mint_subject_token();
        let token_b = mint_subject_token();
        assert_eq!(token_a.len(), 64);
        assert_ne!(token_a, token_b, "tokens must be unpredictable");
        assert_eq!(
            blake2b256_hex(token_a.as_bytes()),
            blake2b256_hex(token_a.as_bytes()),
            "digest must be deterministic"
        );
        assert_ne!(
            blake2b256_hex(token_a.as_bytes()),
            blake2b256_hex(token_b.as_bytes())
        );
    }
}
