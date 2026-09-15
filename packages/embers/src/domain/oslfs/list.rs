use firefly_client::models::{Uri, WalletAddress};
use firefly_client::rendering::Render;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};

use crate::blockchain::oslfs::models;
use crate::domain::common::record_trace;
use crate::domain::oslfs::OslfsService;
use crate::domain::oslfs::models::Oslfs;

#[derive(Debug, Clone, Render)]
#[template(path = "oslfs/list.rho")]
struct List {
    env_uri: Uri,
    address: WalletAddress,
}

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> OslfsService<R, W, N> {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(address),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn list(&self, address: WalletAddress) -> anyhow::Result<Oslfs> {
        record_trace!(address);

        let code = List {
            env_uri: self.uri.clone(),
            address,
        }
        .render()?;
        let oslfs: Vec<models::OslfHeader> = self
            .read_client
            .get_data_or_none(code)
            .await?
            .unwrap_or_default();
        Ok(Oslfs {
            oslfs: oslfs.into_iter().map(Into::into).collect(),
        })
    }
}
