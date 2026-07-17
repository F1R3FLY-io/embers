use firefly_client::models::{Uri, WalletAddress};
use firefly_client::rendering::Render;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};

use crate::blockchain::oslfs::models;
use crate::domain::common::record_trace;
use crate::domain::oslfs::OslfsService;
use crate::domain::oslfs::models::Oslfs;

#[derive(Debug, Clone, Render)]
#[template(path = "oslfs/list_versions.rho")]
struct ListVersions {
    env_uri: Uri,
    address: WalletAddress,
    id: String,
}

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> OslfsService<R, W, N> {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(address, id),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn list_versions(
        &self,
        address: WalletAddress,
        id: String,
    ) -> anyhow::Result<Option<Oslfs>> {
        record_trace!(address, id);

        let code = ListVersions {
            env_uri: self.uri.clone(),
            address,
            id,
        }
        .render()?;

        let oslfs: Option<Vec<models::OslfHeader>> =
            self.read_client.get_data_or_none(code).await?.flatten();
        Ok(oslfs.map(|mut oslfs| {
            oslfs.sort_by(|l, r| l.version.cmp(&r.version));
            Oslfs {
                oslfs: oslfs.into_iter().map(Into::into).collect(),
            }
        }))
    }
}
