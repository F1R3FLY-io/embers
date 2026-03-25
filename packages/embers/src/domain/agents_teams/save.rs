use anyhow::bail;
use chrono::{DateTime, Utc};
use firefly_client::models::{DeployId, SignedCode, Uri, WalletAddress};
use firefly_client::rendering::Render;
use uuid::Uuid;

use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::models::{Graph, SaveReq, SaveResp};
use crate::domain::common::{prepare_for_signing, record_trace};

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/save.rho")]
struct Save {
    env_uri: Uri,
    id: String,
    version: Uuid,
    created_at: DateTime<Utc>,
    name: String,
    description: Option<String>,
    shard: Option<String>,
    logo: Option<String>,
    graph: Option<String>,
}

impl AgentsTeamsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(id, request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_save_contract(
        &self,
        address: WalletAddress,
        id: String,
        request: SaveReq,
    ) -> anyhow::Result<SaveResp> {
        record_trace!(id, request);

        // Verify team exists on observer before generating contract.
        // Without this, the Rholang would abort!() if the prior create
        // hasn't propagated yet, wasting gas and returning a confusing error.
        let teams = self.list(address).await?;
        if !teams.agents_teams.iter().any(|t| t.id == id) {
            bail!(
                "agents team {id} not found (create may still be finalizing)"
            );
        }

        let version = Uuid::now_v7();

        let contract = Save {
            env_uri: self.uri.clone(),
            id,
            version,
            created_at: Utc::now(),
            name: request.name,
            description: request.description,
            shard: request.shard,
            logo: request.logo,
            graph: request.graph.map(Graph::graphl),
        }
        .render()?;

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(SaveResp {
            version: version.into(),
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
    pub async fn deploy_signed_save(&self, contract: SignedCode) -> anyhow::Result<DeployId> {
        record_trace!(contract);

        let mut write_client = self.write_client.clone();

        let deploy_id = write_client.deploy_signed_contract(contract).await?;
        Ok(deploy_id)
    }
}
