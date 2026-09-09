//! Publishing a team to the mail channel: an owner-signed, on-chain binding
//! between a mail address and the team's deployed registry URI.
//!
//! The prepare phase is deliberately side-effect light -- a pure render plus
//! an address-uniqueness read -- because the f1r3sky publish taught the cost
//! of prepare-time irreversibility (accounts and app passwords minted before
//! the user ever signed). Mail has no external account: the binding record IS
//! the publication, it carries no key material, and unpublishing merely flips
//! `active` off, reversibly. Mailbox content keys are minted and custodied by
//! the mail bridge, never by embers: a mailbox key would let embers read
//! third-party mail sent to the team, which the posting-only f1r3sky
//! app-password precedent never allowed.

use anyhow::anyhow;
use firefly_client::models::{DeployId, Uri, WalletAddress};
use firefly_client::rendering::Render;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};

use super::AgentsTeamsService;
use super::models::{
    ChannelBinding,
    DeploySignedPublishToMailReq,
    DeploySignedUnpublishMailReq,
    PublishToMailReq,
    PublishToMailResp,
    UnpublishMailReq,
    UnpublishMailResp,
};
use crate::blockchain;
use crate::domain::common::{prepare_for_signing, record_trace};

pub const MAIL_CHANNEL: &str = "mail";

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/save_channel_binding.rho")]
struct SaveChannelBinding {
    env_uri: Uri,
    channel: String,
    channel_address: String,
    agents_team: Uri,
    team_id: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/set_channel_binding_active.rho")]
struct SetChannelBindingActive {
    env_uri: Uri,
    channel: String,
    channel_address: String,
    active: bool,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/get_channel_bindings.rho")]
struct GetChannelBindings {
    env_uri: Uri,
    channel: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/get_team_channel_bindings.rho")]
struct GetTeamChannelBindings {
    env_uri: Uri,
    team_id: String,
}

#[derive(Debug)]
pub enum PublishToMailError {
    /// The team has no deployed version, so there is no registry URI to bind.
    NotDeployed,
    /// The requested address is already actively bound to a different team.
    BindingDuplicate,
    /// The local part or domain fails the conservative address grammar.
    InvalidAddress(String),
    /// No active mail binding exists for the team (unpublish), or the team
    /// itself is unknown.
    NotFound,
    /// The mutation deploy was rejected on chain and the surviving records
    /// prove the remaining abort cause: the signer does not own the team.
    Unauthorized,
    Internal(anyhow::Error),
}

impl std::fmt::Display for PublishToMailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotDeployed => write!(f, "the team has no deployment to bind"),
            Self::BindingDuplicate => {
                write!(f, "the mail address is already bound to another team")
            }
            Self::InvalidAddress(reason) => write!(f, "{reason}"),
            Self::NotFound => write!(f, "no active mail binding exists for the team"),
            Self::Unauthorized => write!(f, "the signer does not own the team"),
            Self::Internal(err) => write!(f, "{err:?}"),
        }
    }
}

impl std::error::Error for PublishToMailError {}

impl From<anyhow::Error> for PublishToMailError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal(err)
    }
}

/// Conservative RFC 5321-compatible address grammar: enough for every address
/// the platform mints, strict enough that nothing pathological reaches the
/// chain or the mail bridge. Case is normalized to lowercase before
/// validation; the canonical bound address is the lowercase form.
fn validate_mail_address(local_part: &str, domain: &str) -> Result<String, PublishToMailError> {
    let local = local_part.to_lowercase();
    let domain = domain.to_lowercase();

    if local.is_empty() || local.len() > 64 {
        return Err(PublishToMailError::InvalidAddress(
            "mail local part must be 1-64 characters".to_owned(),
        ));
    }
    if !local
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '+' | '-'))
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
    {
        return Err(PublishToMailError::InvalidAddress(
            "mail local part accepts lowercase letters, digits, '.', '_', '+', '-', \
             with no leading, trailing, or doubled dots"
                .to_owned(),
        ));
    }

    if domain.is_empty() || domain.len() > 253 || !domain.contains('.') {
        return Err(PublishToMailError::InvalidAddress(
            "mail domain must be a dotted name of at most 253 characters".to_owned(),
        ));
    }
    let labels_valid = domain.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    });
    if !labels_valid {
        return Err(PublishToMailError::InvalidAddress(
            "mail domain labels accept lowercase letters, digits, and interior hyphens"
                .to_owned(),
        ));
    }

    Ok(format!("{local}@{domain}"))
}

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(address, request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_publish_to_mail(
        &self,
        address: WalletAddress,
        request: PublishToMailReq,
    ) -> Result<PublishToMailResp, PublishToMailError> {
        record_trace!(address, request);

        let mail_address = validate_mail_address(&request.local_part, &request.mail_domain)?;

        let team = self
            .get(address, request.team_id.clone(), "latest".to_owned())
            .await?
            .ok_or(PublishToMailError::NotFound)?;
        let team_uri = team.uri.ok_or(PublishToMailError::NotDeployed)?;

        // Address uniqueness against every existing mail binding. The clause
        // re-checks this atomically at deploy time; the read here keeps the
        // failure before anything is signed.
        let bindings = self.list_channel_bindings(MAIL_CHANNEL.to_owned()).await?;
        let conflict = bindings.iter().any(|binding| {
            binding.address == mail_address
                && binding.active
                && binding.team_id != request.team_id
        });
        if conflict {
            return Err(PublishToMailError::BindingDuplicate);
        }

        let code = SaveChannelBinding {
            env_uri: self.uri.clone(),
            channel: MAIL_CHANNEL.to_owned(),
            channel_address: mail_address.clone(),
            agents_team: team_uri,
            team_id: request.team_id.clone(),
        }
        .render()
        .map_err(anyhow::Error::from)?;
        let valid_after = self
            .write_client
            .clone()
            .get_head_block_index()
            .await
            .map_err(anyhow::Error::from)?;
        Ok(PublishToMailResp {
            address: mail_address,
            contract: prepare_for_signing()
                .code(code)
                .valid_after_block_number(valid_after)
                .call(),
        })
    }

    #[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
    pub async fn deploy_signed_publish_to_mail(
        &self,
        request: DeploySignedPublishToMailReq,
    ) -> Result<DeployId, PublishToMailError> {
        let mail_address = request.prepare_response.address.clone();
        let team_id = request.prepare_request.team_id.clone();
        let deploy_id = self
            .write_client
            .clone()
            .deploy_signed_contract(request.contract)
            .await
            .map_err(anyhow::Error::from)?;
        match self
            .observer_node_events
            .wait_for_deploy(&deploy_id, self.deploy_finalization_timeout)
            .await
        {
            Some(false) => {}
            Some(true) => {
                // The clause aborted. Re-read the bindings to name the cause
                // honestly: an address actively bound to another team is
                // knowable from the public record; what remains is a signer
                // who does not own the team.
                let bindings = self.list_channel_bindings(MAIL_CHANNEL.to_owned()).await?;
                let taken = bindings.iter().any(|binding| {
                    binding.address == mail_address && binding.team_id != team_id
                });
                return Err(if taken {
                    PublishToMailError::BindingDuplicate
                } else {
                    PublishToMailError::Unauthorized
                });
            }
            None => {
                return Err(PublishToMailError::Internal(anyhow!(
                    "mail binding deploy {deploy_id} was not finalized"
                )));
            }
        }

        let acknowledged: bool = self
            .read_deploy_result(deploy_id.clone())
            .await
            .map_err(|err| PublishToMailError::Internal(err.context("mail binding acknowledgment")))?;
        if !acknowledged {
            return Err(PublishToMailError::Internal(anyhow!(
                "mail binding was not acknowledged on chain"
            )));
        }

        // Registration readback: the binding must be observably active before
        // the owner is told the team is reachable by mail.
        let bindings = self.get_team_channel_bindings(team_id.clone()).await?;
        let observable = bindings.iter().any(|binding| {
            binding.channel == MAIL_CHANNEL && binding.address == mail_address && binding.active
        });
        if observable {
            Ok(deploy_id)
        } else {
            Err(PublishToMailError::Internal(anyhow!(
                "mail binding for team {team_id} was acknowledged but is not observable"
            )))
        }
    }

    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_unpublish_mail(
        &self,
        request: UnpublishMailReq,
    ) -> Result<UnpublishMailResp, PublishToMailError> {
        record_trace!(request);

        let bindings = self.get_team_channel_bindings(request.team_id.clone()).await?;
        let binding = bindings
            .into_iter()
            .find(|binding| binding.channel == MAIL_CHANNEL && binding.active)
            .ok_or(PublishToMailError::NotFound)?;

        let code = SetChannelBindingActive {
            env_uri: self.uri.clone(),
            channel: MAIL_CHANNEL.to_owned(),
            channel_address: binding.address.clone(),
            active: false,
        }
        .render()
        .map_err(anyhow::Error::from)?;
        let valid_after = self
            .write_client
            .clone()
            .get_head_block_index()
            .await
            .map_err(anyhow::Error::from)?;
        Ok(UnpublishMailResp {
            address: binding.address,
            contract: prepare_for_signing()
                .code(code)
                .valid_after_block_number(valid_after)
                .call(),
        })
    }

    #[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
    pub async fn deploy_signed_unpublish_mail(
        &self,
        request: DeploySignedUnpublishMailReq,
    ) -> Result<DeployId, PublishToMailError> {
        let mail_address = request.prepare_response.address.clone();
        let deploy_id = self
            .write_client
            .clone()
            .deploy_signed_contract(request.contract)
            .await
            .map_err(anyhow::Error::from)?;
        match self
            .observer_node_events
            .wait_for_deploy(&deploy_id, self.deploy_finalization_timeout)
            .await
        {
            Some(false) => {}
            Some(true) => {
                // Absence and ownership are the only abort causes; absence is
                // knowable from the public record.
                let bindings = self.list_channel_bindings(MAIL_CHANNEL.to_owned()).await?;
                let exists = bindings.iter().any(|binding| binding.address == mail_address);
                return Err(if exists {
                    PublishToMailError::Unauthorized
                } else {
                    PublishToMailError::NotFound
                });
            }
            None => {
                return Err(PublishToMailError::Internal(anyhow!(
                    "mail unbinding deploy {deploy_id} was not finalized"
                )));
            }
        }

        let acknowledged: bool = self
            .read_deploy_result(deploy_id.clone())
            .await
            .map_err(|err| {
                PublishToMailError::Internal(err.context("mail unbinding acknowledgment"))
            })?;
        if !acknowledged {
            return Err(PublishToMailError::Internal(anyhow!(
                "mail unbinding was not acknowledged on chain"
            )));
        }

        let bindings = self.list_channel_bindings(MAIL_CHANNEL.to_owned()).await?;
        let still_active = bindings
            .iter()
            .any(|binding| binding.address == mail_address && binding.active);
        if still_active {
            Err(PublishToMailError::Internal(anyhow!(
                "mail unbinding was acknowledged but the binding is still active"
            )))
        } else {
            Ok(deploy_id)
        }
    }

    pub async fn list_channel_bindings(
        &self,
        channel: String,
    ) -> anyhow::Result<Vec<ChannelBinding>> {
        let code = GetChannelBindings {
            env_uri: self.uri.clone(),
            channel,
        }
        .render()?;
        let bindings: Vec<blockchain::agents_teams::models::ChannelBinding> =
            self.read_client.get_data(code).await?;
        Ok(bindings.into_iter().map(Into::into).collect())
    }

    pub async fn get_team_channel_bindings(
        &self,
        team_id: String,
    ) -> anyhow::Result<Vec<ChannelBinding>> {
        let code = GetTeamChannelBindings {
            env_uri: self.uri.clone(),
            team_id,
        }
        .render()?;
        let bindings: Vec<blockchain::agents_teams::models::ChannelBinding> =
            self.read_client.get_data(code).await?;
        Ok(bindings.into_iter().map(Into::into).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_address_is_lowercased_and_joined() {
        let address =
            validate_mail_address("Research-Crew", "Agents.Example").expect("valid address");
        assert_eq!(address, "research-crew@agents.example");
    }

    #[test]
    fn local_part_grammar_is_enforced() {
        assert!(matches!(
            validate_mail_address("", "agents.example"),
            Err(PublishToMailError::InvalidAddress(_))
        ));
        assert!(matches!(
            validate_mail_address(".leading", "agents.example"),
            Err(PublishToMailError::InvalidAddress(_))
        ));
        assert!(matches!(
            validate_mail_address("dou..bled", "agents.example"),
            Err(PublishToMailError::InvalidAddress(_))
        ));
        assert!(matches!(
            validate_mail_address("spa ce", "agents.example"),
            Err(PublishToMailError::InvalidAddress(_))
        ));
        assert!(matches!(
            validate_mail_address(&"a".repeat(65), "agents.example"),
            Err(PublishToMailError::InvalidAddress(_))
        ));
    }

    #[test]
    fn domain_grammar_is_enforced() {
        assert!(matches!(
            validate_mail_address("team", "nodots"),
            Err(PublishToMailError::InvalidAddress(_))
        ));
        assert!(matches!(
            validate_mail_address("team", "-bad.example"),
            Err(PublishToMailError::InvalidAddress(_))
        ));
        assert!(matches!(
            validate_mail_address("team", "double..dot"),
            Err(PublishToMailError::InvalidAddress(_))
        ));
        validate_mail_address("team", "agents.shard-1.example").expect("hyphenated labels are legal");
    }

    #[test]
    fn binding_templates_render_the_clause_calls() {
        let save = SaveChannelBinding {
            env_uri: crate::domain::test_helpers::test_uri(),
            channel: MAIL_CHANNEL.to_owned(),
            channel_address: "team@agents.example".to_owned(),
            agents_team: crate::domain::test_helpers::test_uri(),
            team_id: "team-1".to_owned(),
        }
        .render()
        .expect("save binding template should render");
        assert!(save.contains("saveChannelBinding"));
        assert!(save.contains("\"team@agents.example\""));
        assert!(save.contains("forward!(ack, (*deployId).toString())"));

        let deactivate = SetChannelBindingActive {
            env_uri: crate::domain::test_helpers::test_uri(),
            channel: MAIL_CHANNEL.to_owned(),
            channel_address: "team@agents.example".to_owned(),
            active: false,
        }
        .render()
        .expect("set binding active template should render");
        assert!(deactivate.contains("setChannelBindingActive"));
        assert!(deactivate.contains("false"));
    }
}
