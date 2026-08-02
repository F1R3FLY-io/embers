use std::sync::LazyLock;
use std::time::Duration;

use atrium_api::agent::Agent;
use atrium_api::agent::atp_agent::CredentialSession;
use atrium_api::agent::atp_agent::store::MemorySessionStore;
use atrium_api::app::bsky::actor::{Profile, profile};
use atrium_api::client::AtpServiceClient;
use atrium_api::com::atproto::{repo, server};
use atrium_api::record::KnownRecord;
use atrium_api::types::string::{AtIdentifier, Datetime, Handle, RecordKey};
use atrium_api::types::{Collection, TryIntoUnknown};
use atrium_xrpc_client::reqwest::ReqwestClient;
use firefly_client::models::{DeployId, SignedCode, Uri, WalletAddress};
use firefly_client::rendering::Render;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};
use futures::FutureExt;
use futures::future::OptionFuture;
use serde::{Deserialize, Serialize};

use crate::blockchain::agents_teams::models;
use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::models::{
    EncryptedMsg,
    FireskyCredentials,
    PublishToFireskyReq,
    PublishToFireskyResp,
};
use crate::domain::common::{
    prepare_for_signing,
    record_trace,
    serialize_encrypted,
    upload_blob_from_url,
};

/// An app-password name this account is not already using.
///
/// Names are unique per account, so a second `Embers` password is refused -- which
/// made publishing to an account that had been published to before fail at the last
/// step, after the config record had already been written. Numbering from the names
/// actually present keeps every publication holding its own credential, so revoking
/// one never silently disarms another.
///
/// Bounded rather than open-ended: an account carrying a hundred Embers passwords is
/// a symptom, not a case to accommodate, and atproto caps app passwords anyway.
async fn free_app_password_name<S>(agent: &Agent<S>) -> anyhow::Result<String>
where
    S: atrium_api::agent::SessionManager + Send + Sync,
{
    const BASE: &str = "Embers";
    const LIMIT: usize = 100;

    let taken: std::collections::HashSet<String> = agent
        .api
        .com
        .atproto
        .server
        .list_app_passwords()
        .await?
        .data
        .passwords
        .into_iter()
        .map(|password| password.data.name)
        .collect();

    if !taken.contains(BASE) {
        return Ok(BASE.to_owned());
    }

    (2..=LIMIT)
        .map(|n| format!("{BASE} {n}"))
        .find(|name| !taken.contains(name))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "this account already holds {LIMIT} Embers app passwords; revoke the \
                 ones no longer in use before publishing again"
            )
        })
}

pub(super) static SELF_RKEY: LazyLock<RecordKey> =
    LazyLock::new(|| RecordKey::new("self".into()).unwrap());

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/save_firesky_token.rho")]
struct SaveFireskyToken {
    env_uri: Uri,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    #[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
    pub async fn prepare_publish_to_firesky_contract(
        &self,
        address: WalletAddress,
        id: String,
        request: PublishToFireskyReq,
    ) -> anyhow::Result<PublishToFireskyResp> {
        let handle = Handle::new(request.handle).map_err(|err| anyhow::anyhow!(err))?;
        // Captured before `handle` and `did` are consumed below: these are what let
        // a later read say *which* account a team was published under, and so
        // whether that account still exists.
        let handle_str = handle.as_str().to_owned();

        let agent_team = self
            .get_deployed_with_retry(address, id, "latest".into(), 10, Duration::from_millis(500))
            .await?
            .ok_or_else(|| anyhow::anyhow!("agents team not found"))?;

        let uri = agent_team
            .uri
            .ok_or_else(|| anyhow::anyhow!("agents team not deployed"))?;

        let http_client = ReqwestClient::new(request.pds_url.clone());
        let client = AtpServiceClient::new(http_client.clone());

        let created = client
            .service
            .com
            .atproto
            .server
            .create_account(
                server::create_account::InputData {
                    did: None,
                    email: request.email.clone().into(),
                    handle: handle.clone(),
                    invite_code: request.invite_code,
                    password: request.password.clone().into(),
                    plc_op: None,
                    recovery_key: None,
                    verification_code: None,
                    verification_phone: None,
                }
                .into(),
            )
            .await;

        let session = CredentialSession::new(http_client, MemorySessionStore::default());

        // Publishing to a handle that already exists is the ordinary way to publish
        // again after unpublishing. Unpublishing deliberately keeps the account and
        // the handle, so that it is reversible rather than a slower deletion -- but
        // creating unconditionally made that promise false: the second publish died
        // on the handle the first one had taken, and the only way back was to pick a
        // different handle, which is precisely what was being preserved.
        //
        // Signing in is what tells the two cases apart. Credentials that work belong
        // to the caller; credentials that do not mean the handle is somebody else's,
        // which is a real conflict and is reported as one.
        let did = match created {
            Ok(account) => {
                session.login(&request.email, &request.password).await?;
                account.data.did
            }
            Err(create_error) => {
                let existing = session
                    .login(&request.email, &request.password)
                    .await
                    .map_err(|login_error| {
                        anyhow::anyhow!(
                            "could not create an account for this handle ({create_error}), \
                             and could not sign in to an existing one either \
                             ({login_error}) — the handle may belong to someone else, or \
                             the password may differ from the one it was created with"
                        )
                    })?;

                // The identifier signed in with is the email, so the account reached
                // is not necessarily the one that carries the requested handle.
                // Publishing to it anyway would put the team on an account the user
                // did not name, under a handle they did not ask for.
                if existing.data.handle != handle {
                    return Err(anyhow::anyhow!(
                        "these credentials belong to @{}, not @{}; publish under that \
                         handle, or use the credentials of the account you meant",
                        existing.data.handle.as_str(),
                        handle.as_str(),
                    ));
                }

                existing.data.did.clone()
            }
        };

        let did_str = did.as_str().to_owned();
        let agent = Agent::new(session);
        let agent_repo = AtIdentifier::Did(did);

        if agent_team.description.is_some() || agent_team.logo.is_some() {
            let avatar = OptionFuture::from(
                agent_team
                    .logo
                    .as_ref()
                    .map(|logo| upload_blob_from_url(&agent, logo).map(Result::ok)),
            )
            .await
            .flatten();

            agent
                .api
                .com
                .atproto
                .repo
                .put_record(
                    repo::put_record::InputData {
                        collection: Profile::nsid(),
                        record: KnownRecord::AppBskyActorProfile(Box::new(
                            profile::RecordData {
                                avatar,
                                banner: None,
                                created_at: Some(Datetime::now()),
                                description: agent_team.description,
                                display_name: None,
                                joined_via_starter_pack: None,
                                labels: None,
                                pinned_post: None,
                                pronouns: None,
                                website: None,
                            }
                            .into(),
                        ))
                        .try_into_unknown()?,
                        repo: agent_repo.clone(),
                        rkey: SELF_RKEY.clone(),
                        swap_record: None,
                        swap_commit: None,
                        validate: None,
                    }
                    .into(),
                )
                .await?;
        }

        // `put` rather than `create`: the record is a singleton at rkey `self`, and
        // creating it fails outright when one is already there. That made publishing
        // twice impossible -- both after unpublishing and after a re-deploy, where
        // the point of publishing again is precisely to replace the uri this record
        // holds with the newer one.
        agent
            .api
            .com
            .atproto
            .repo
            .put_record(
                repo::put_record::InputData {
                    collection: AgentsTeamConfig::nsid(),
                    record: AgentsTeamConfig {
                        uri: uri.clone().into(),
                    }
                    .try_into_unknown()?,
                    repo: agent_repo,
                    rkey: SELF_RKEY.clone(),
                    swap_commit: None,
                    swap_record: None,
                    validate: None,
                }
                .into(),
            )
            .await?;

        let result = agent
            .api
            .com
            .atproto
            .server
            .create_app_password(
                server::create_app_password::InputData {
                    name: free_app_password_name(&agent).await?,
                    privileged: Some(false),
                }
                .into(),
            )
            .await?;

        let payload = models::FireskyCredentials {
            uri: uri.clone().into(),
            pds_url: request.pds_url.clone(),
            email: request.email.clone(),
            token: result.data.password.clone(),
            handle: Some(handle_str.clone()),
            did: Some(did_str.clone()),
        };
        let EncryptedMsg { ciphertext, nonce } =
            serialize_encrypted(payload, &self.aes_encryption_key)?;

        self.firesky_accounts.insert(
            uri,
            FireskyCredentials {
                pds_url: request.pds_url,
                email: request.email,
                token: result.data.password,
                handle: Some(handle_str),
                did: Some(did_str),
            },
        );

        let contract = SaveFireskyToken {
            env_uri: self.uri.clone(),
            nonce,
            ciphertext,
        }
        .render()?;

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(PublishToFireskyResp {
            contract: prepare_for_signing()
                .code(contract)
                .valid_after_block_number(valid_after)
                .call(),
        })
    }

    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(contract),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn deploy_signed_publish_to_firesky(
        &self,
        contract: SignedCode,
    ) -> anyhow::Result<DeployId> {
        record_trace!(contract);

        let mut write_client = self.write_client.clone();

        let deploy_id = write_client.deploy_signed_contract(contract).await?;
        Ok(deploy_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AgentsTeamConfig {
    uri: String,
}

impl Collection for AgentsTeamConfig {
    const NSID: &'static str = "com.f1r3sky.agentsteam.config";
    type Record = Self;
}
