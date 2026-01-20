use std::sync::LazyLock;

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

static SELF_RKEY: LazyLock<RecordKey> = LazyLock::new(|| RecordKey::new("self".into()).unwrap());

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/save_firesky_token.rho")]
struct SaveFireskyToken {
    env_uri: Uri,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

impl AgentsTeamsService {
    #[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
    pub async fn prepare_publish_to_firesky_contract(
        &self,
        address: WalletAddress,
        id: String,
        request: PublishToFireskyReq,
    ) -> anyhow::Result<PublishToFireskyResp> {
        let handle = Handle::new(request.handle).map_err(|err| anyhow::anyhow!(err))?;

        let agent_team = self
            .get(address, id, "latest".into())
            .await?
            .ok_or_else(|| anyhow::anyhow!("agents team not found"))?;

        let uri = agent_team
            .uri
            .ok_or_else(|| anyhow::anyhow!("agents team not deployed"))?;

        let http_client = ReqwestClient::new(request.pds_url.clone());
        let client = AtpServiceClient::new(http_client.clone());

        let did = client
            .service
            .com
            .atproto
            .server
            .create_account(
                server::create_account::InputData {
                    did: None,
                    email: request.email.clone().into(),
                    handle,
                    invite_code: request.invite_code,
                    password: request.password.clone().into(),
                    plc_op: None,
                    recovery_key: None,
                    verification_code: None,
                    verification_phone: None,
                }
                .into(),
            )
            .await?
            .data
            .did;

        let session = CredentialSession::new(http_client, MemorySessionStore::default());
        session.login(&request.email, request.password).await?;
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

        agent
            .api
            .com
            .atproto
            .repo
            .create_record(
                repo::create_record::InputData {
                    collection: AgentsTeamConfig::nsid(),
                    record: AgentsTeamConfig {
                        uri: uri.clone().into(),
                    }
                    .try_into_unknown()?,
                    repo: agent_repo,
                    rkey: Some(SELF_RKEY.clone()),
                    swap_commit: None,
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
                    name: "Embers".into(),
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
        };
        let EncryptedMsg { ciphertext, nonce } =
            serialize_encrypted(payload, &self.aes_encryption_key)?;

        self.firesky_accounts.insert(
            uri,
            FireskyCredentials {
                pds_url: request.pds_url,
                email: request.email,
                token: result.data.password,
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
        write_client.propose().await?;
        Ok(deploy_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentsTeamConfig {
    uri: String,
}

impl Collection for AgentsTeamConfig {
    const NSID: &'static str = "com.f1r3sky.agentsteam.config";
    type Record = Self;
}
