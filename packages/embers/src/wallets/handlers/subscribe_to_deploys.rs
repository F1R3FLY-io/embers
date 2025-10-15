use std::io;

use futures::{Sink, SinkExt, StreamExt, future, stream};
use tokio_stream::wrappers::{BroadcastStream, errors};
use tracing::Instrument;

use crate::wallets::handlers::WalletsService;
use crate::wallets::models::{NodeType, WalletEvent};

impl WalletsService {
    #[tracing::instrument(level = "info", skip_all)]
    pub fn subscribe_to_deploys(
        &self,
        sink: impl Sink<WalletEvent, Error = io::Error> + Send + 'static,
    ) {
        let observer_deploys = BroadcastStream::new(
            self.observer_node_events.subscribe_for_deploys(),
        )
        .filter_map(|res| {
            future::ready(match res {
                Ok(deploy_id) => Some(Ok(WalletEvent::DeploySeen {
                    deploy_id,
                    node_type: NodeType::Observer,
                })),
                Err(errors::BroadcastStreamRecvError::Lagged(_)) => None,
            })
        });

        let validator_deploys = BroadcastStream::new(
            self.validator_node_events.subscribe_for_deploys(),
        )
        .filter_map(|res| {
            future::ready(match res {
                Ok(deploy_id) => Some(Ok(WalletEvent::DeploySeen {
                    deploy_id,
                    node_type: NodeType::Validator,
                })),
                Err(errors::BroadcastStreamRecvError::Lagged(_)) => None,
            })
        });

        tokio::spawn(
            async move {
                let mut sum_stream = stream::select(observer_deploys, validator_deploys);

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
