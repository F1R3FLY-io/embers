use std::io;

use firefly_client::models::WalletAddress;
use futures::{Sink, SinkExt, StreamExt, stream};
use tracing::Instrument;

use crate::wallets::handlers::WalletsService;
use crate::wallets::models::{NodeType, WalletEvent};

impl WalletsService {
    #[tracing::instrument(level = "info", skip_all)]
    pub fn subscribe_to_deploys(
        &self,
        wallet_address: WalletAddress,
        sink: impl Sink<WalletEvent, Error = io::Error> + Send + 'static,
    ) {
        let observer_deploys = self
            .observer_node_events
            .subscribe_for_deploys(wallet_address.clone())
            .map(|deploy_event| {
                Ok(WalletEvent::DeploySeen {
                    deploy_id: deploy_event.id,
                    cost: deploy_event.cost,
                    errored: deploy_event.errored,
                    node_type: NodeType::Observer,
                })
            });

        let validator_deploys = self
            .validator_node_events
            .subscribe_for_deploys(wallet_address)
            .map(|deploy_event| {
                Ok(WalletEvent::DeploySeen {
                    deploy_id: deploy_event.id,
                    cost: deploy_event.cost,
                    errored: deploy_event.errored,
                    node_type: NodeType::Validator,
                })
            });

        tokio::spawn(
            async move {
                let sum_stream = stream::select(observer_deploys, validator_deploys);

                tokio::pin!(sum_stream);
                tokio::pin!(sink);

                let _ = sink
                    .send_all(&mut sum_stream)
                    .await
                    .inspect_err(|err| tracing::debug!("error in sink: {err:?}"));
            }
            .in_current_span(),
        );
    }
}
