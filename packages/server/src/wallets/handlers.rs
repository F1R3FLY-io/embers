use firefly_client::rendering::Uri;
use firefly_client::{ReadNodeClient, WriteNodeClient};

mod get_wallet_state_and_history;
mod transfer;

#[derive(Clone)]
pub struct WalletsService {
    pub uri: Uri,
    pub write_client: WriteNodeClient,
    pub read_client: ReadNodeClient,
}
