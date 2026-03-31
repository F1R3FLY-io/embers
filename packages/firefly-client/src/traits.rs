use std::future::Future;
use std::time::Duration;

use futures::Stream;
use secp256k1::SecretKey;

use crate::errors::ReadNodeError;
use crate::models::{DeployData, DeployId, SignedCode, WalletAddress};
use crate::node_events::DeployEvent;

/// Trait abstracting read operations against a blockchain node.
///
/// The concrete implementation is [`ReadNodeClient`](crate::ReadNodeClient),
/// which uses HTTP / explore-deploy under the hood. Extracting a trait
/// enables unit-testing domain logic with lightweight test doubles.
pub trait ReadNode: Clone + Send + Sync + 'static {
    fn get_data<T: serde::de::DeserializeOwned + Send>(
        &self,
        rholang_code: String,
    ) -> impl Future<Output = Result<T, ReadNodeError>> + Send;

    fn get_data_or_none<T: serde::de::DeserializeOwned + Send>(
        &self,
        rholang_code: String,
    ) -> impl Future<Output = Result<Option<T>, ReadNodeError>> + Send;

    fn get_data_with_retry<T: serde::de::DeserializeOwned + Send>(
        &self,
        rholang_code: String,
        max_retries: u32,
        delay: Duration,
    ) -> impl Future<Output = Result<T, ReadNodeError>> + Send;

    fn get_data_or_none_with_retry<T: serde::de::DeserializeOwned + Send>(
        &self,
        rholang_code: String,
        max_retries: u32,
        delay: Duration,
    ) -> impl Future<Output = Result<Option<T>, ReadNodeError>> + Send;
}

/// Trait abstracting write / deploy operations against a blockchain node.
///
/// The concrete implementation is [`WriteNodeClient`](crate::WriteNodeClient),
/// which uses gRPC under the hood.
pub trait WriteNode: Clone + Send + Sync + 'static {
    fn deploy(
        &mut self,
        key: &SecretKey,
        deploy_data: DeployData,
    ) -> impl Future<Output = anyhow::Result<DeployId>> + Send;

    fn deploy_signed_contract(
        &mut self,
        contract: SignedCode,
    ) -> impl Future<Output = anyhow::Result<DeployId>> + Send;

    fn full_deploy(
        &mut self,
        key: &SecretKey,
        deploy_data: DeployData,
    ) -> impl Future<Output = anyhow::Result<DeployId>> + Send;

    fn get_head_block_index(
        &mut self,
    ) -> impl Future<Output = anyhow::Result<u64>> + Send;
}

/// Trait abstracting event subscriptions from a blockchain node.
///
/// The concrete implementation is [`NodeEvents`](crate::NodeEvents),
/// which uses WebSocket under the hood.
pub trait NodeEventSource: Clone + Send + Sync + 'static {
    fn wait_for_deploy(
        &self,
        deploy_id: &DeployId,
        max_wait: Duration,
    ) -> impl Future<Output = Option<bool>> + Send;

    fn subscribe_for_deploys(
        &self,
        wallet_address: WalletAddress,
    ) -> impl Stream<Item = DeployEvent> + Send;
}
