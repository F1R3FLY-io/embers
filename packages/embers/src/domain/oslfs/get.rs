use firefly_client::models::{Uri, WalletAddress};
use firefly_client::rendering::Render;

use crate::blockchain::oslfs::models;
use crate::domain::common::record_trace;
use crate::domain::oslfs::OslfsService;
use crate::domain::oslfs::models::Oslf;

#[derive(Debug, Clone, Render)]
#[template(path = "oslfs/get.rho")]
struct Get {
    env_uri: Uri,
    address: WalletAddress,
    id: String,
    version: String,
}

impl OslfsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(address, id, version),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn get(
        &self,
        address: WalletAddress,
        id: String,
        version: String,
    ) -> anyhow::Result<Option<Oslf>> {
        record_trace!(address, id, version);

        let code = Get {
            env_uri: self.uri.clone(),
            address,
            id,
            version,
        }
        .render()?;

        let oslf: Option<models::Oslf> = self.read_client.get_data(code).await?;
        Ok(oslf.map(Into::into))
    }
}
