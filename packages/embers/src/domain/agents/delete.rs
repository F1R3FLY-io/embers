use firefly_client::models::{DeployId, SignedCode, Uri};
use firefly_client::rendering::Render;

use crate::domain::agents::AgentsService;
use crate::domain::agents::models::DeleteResp;
use crate::domain::common::{prepare_for_signing, record_trace};

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents/delete_agent.rho")]
struct Delete {
    env_uri: Uri,
    id: String,
}

impl AgentsService {
    #[tracing::instrument(level = "info", skip(self), err(Debug), ret(Debug, level = "trace"))]
    pub async fn prepare_delete_contract(&self, id: String) -> anyhow::Result<DeleteResp> {
        let contract = Delete {
            env_uri: self.uri.clone(),
            id,
        }
        .render()?;

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(DeleteResp {
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
    pub async fn deploy_signed_delete(&self, contract: SignedCode) -> anyhow::Result<DeployId> {
        record_trace!(contract);

        let mut write_client = self.write_client.clone();

        let deploy_id = write_client.deploy_signed_contract(contract).await?;
        write_client.propose().await?;

        Ok(deploy_id)
    }
}
