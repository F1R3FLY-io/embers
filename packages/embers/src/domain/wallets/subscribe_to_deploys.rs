use std::io;

use firefly_client::models::WalletAddress;
use firefly_client::node_events;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};
use futures::{Sink, SinkExt, StreamExt, stream};

use crate::domain::wallets::WalletsService;
use crate::domain::wallets::models::{DeployDescription, DeployEvent, NodeType};

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> WalletsService<R, W, N> {
    #[tracing::instrument(level = "info", skip_all)]
    pub async fn subscribe_to_deploys(
        self,
        wallet_address: WalletAddress,
        sink: impl Sink<DeployEvent, Error = io::Error> + Send + 'static,
    ) {
        let observer_deploys = self
            .observer_node_events
            .subscribe_for_deploys(wallet_address.clone())
            .map(|deploy_event| match deploy_event {
                node_events::DeployEvent::Finalized { id, cost, errored } => {
                    DeployEvent::Finalized(DeployDescription {
                        deploy_id: id,
                        cost,
                        errored,
                        node_type: NodeType::Observer,
                    })
                }
            })
            .map(Ok);

        let validator_deploys = self
            .validator_node_events
            .subscribe_for_deploys(wallet_address)
            .map(|deploy_event| match deploy_event {
                node_events::DeployEvent::Finalized { id, cost, errored } => {
                    DeployEvent::Finalized(DeployDescription {
                        deploy_id: id,
                        cost,
                        errored,
                        node_type: NodeType::Validator,
                    })
                }
            })
            .map(Ok);

        let sum_stream = stream::select(observer_deploys, validator_deploys);

        tokio::pin!(sum_stream);
        tokio::pin!(sink);

        let _ = sink
            .send_all(&mut sum_stream)
            .await
            .inspect_err(|err| tracing::debug!("error in wallet deploy sink: {err:?}"));
    }
}
