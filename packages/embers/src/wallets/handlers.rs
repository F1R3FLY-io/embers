use firefly_client::rendering::Uri;
use firefly_client::{NodeEvents, ReadNodeClient, WriteNodeClient};

mod get_wallet_state_and_history;
mod subscribe_to_deploys;
mod transfer;

#[derive(Clone)]
pub struct WalletsService {
    pub uri: Uri,
    pub write_client: WriteNodeClient,
    pub read_client: ReadNodeClient,
    pub validator_node_events: NodeEvents,
    pub observer_node_events: NodeEvents,
}
