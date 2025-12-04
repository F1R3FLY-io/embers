use atrium_api::agent::Agent;
use atrium_api::agent::atp_agent::CredentialSession;
use atrium_api::agent::atp_agent::store::MemorySessionStore;
use atrium_api::client::AtpServiceClient;
use atrium_api::com::atproto::server;
use atrium_api::types::string::Handle;
use atrium_xrpc_client::reqwest::ReqwestClient;
use firefly_client::models::{DeployId, SignedCode, Uri, WalletAddress};
use firefly_client::rendering::Render;

use crate::ai_agents_teams::blockchain;
use crate::ai_agents_teams::common::{EncryptedMsg, serialize_encrypted};
use crate::ai_agents_teams::handlers::AgentsTeamsService;
use crate::ai_agents_teams::models::{
    FireskyCredentials,
    PublishAgentsTeamToFireskyReq,
    PublishAgentsTeamToFireskyResp,
};
use crate::common::prepare_for_signing;
use crate::common::tracing::record_trace;

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents_teams/save_firesky_token.rho")]
struct SaveFireskyToken {
    env_uri: Uri,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

impl AgentsTeamsService {
    #[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
    pub async fn prepare_publish_agents_team_to_firesky_contract(
        &self,
        address: WalletAddress,
        id: String,
        request: PublishAgentsTeamToFireskyReq,
    ) -> anyhow::Result<PublishAgentsTeamToFireskyResp> {
        let agent_team = self
            .get_agents_team(address, id, "latest".into())
            .await?
            .ok_or_else(|| anyhow::anyhow!("agents team not found"))?;

        let uri = agent_team
            .uri
            .ok_or_else(|| anyhow::anyhow!("agents team not deployed"))?;

        let http_client = ReqwestClient::new(request.pds_url.clone());
        let client = AtpServiceClient::new(http_client.clone());

        client
            .service
            .com
            .atproto
            .server
            .create_account(
                server::create_account::InputData {
                    did: None,
                    email: request.email.clone().into(),
                    handle: Handle::new(request.handle).map_err(|err| anyhow::anyhow!(err))?,
                    invite_code: request.invite_code.unwrap_or_default().into(),
                    password: request.password.clone().into(),
                    plc_op: None,
                    recovery_key: None,
                    verification_code: None,
                    verification_phone: None,
                }
                .into(),
            )
            .await?;

        let session = CredentialSession::new(http_client, MemorySessionStore::default());
        session.login(&request.email, request.password).await?;
        let agent = Agent::new(session);

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

        let payload = blockchain::dtos::FireskyCredentials {
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
        Ok(PublishAgentsTeamToFireskyResp {
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
    pub async fn deploy_signed_publish_agents_team_to_firesky(
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
