use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use firefly_client::errors::ReadNodeError;
use firefly_client::models::{DeployData, DeployId, SignedCode, Uri, WalletAddress};
use firefly_client::node_events::DeployEvent;
use firefly_client::traits::{NodeEventSource, ReadNode, WriteNode};
use futures::stream;
use secp256k1::{PublicKey, Secp256k1, SecretKey};

// -------------------------------------------------------
// MockReadNode
// -------------------------------------------------------

/// A mock read node client that returns pre-configured responses.
///
/// Responses are matched by checking whether the Rholang code
/// passed to each method contains a specified substring.
#[derive(Clone)]
pub struct MockReadNode {
    responses: Arc<Vec<MockReadResponse>>,
    calls: Arc<Mutex<Vec<String>>>,
}

struct MockReadResponse {
    contains: String,
    value: serde_json::Value,
    error: Option<ReadNodeError>,
    /// Number of leading matching reads that should return `ReturnValueMissing`
    /// before `value` is served — simulates the read-node visibility lag a
    /// retrying caller must tolerate.
    empty_before: usize,
    /// How many times this response has matched so far (interior mutability so
    /// `find_response` can sequence empty-then-data through `&self`).
    hits: AtomicUsize,
}

impl MockReadNode {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Vec::new()),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Add a response that triggers when the Rholang code contains `substr`.
    pub fn on_code_containing(mut self, substr: &str, value: serde_json::Value) -> Self {
        Arc::get_mut(&mut self.responses)
            .expect("no other clones should exist during setup")
            .push(MockReadResponse {
                contains: substr.to_owned(),
                value,
                error: None,
                empty_before: 0,
                hits: AtomicUsize::new(0),
            });
        self
    }

    /// Add a response that returns `ReturnValueMissing` for the first
    /// `empty_before` matching reads and then serves `value`. Models the
    /// read-after-write visibility lag that a retrying caller must tolerate.
    pub fn on_code_containing_empty_then(
        mut self,
        substr: &str,
        empty_before: usize,
        value: serde_json::Value,
    ) -> Self {
        Arc::get_mut(&mut self.responses)
            .expect("no other clones should exist during setup")
            .push(MockReadResponse {
                contains: substr.to_owned(),
                value,
                error: None,
                empty_before,
                hits: AtomicUsize::new(0),
            });
        self
    }

    /// Add an error response that triggers when the Rholang code contains `substr`.
    pub fn on_code_containing_error(mut self, substr: &str, error: ReadNodeError) -> Self {
        Arc::get_mut(&mut self.responses)
            .expect("no other clones should exist during setup")
            .push(MockReadResponse {
                contains: substr.to_owned(),
                value: serde_json::Value::Null,
                error: Some(error),
                empty_before: 0,
                hits: AtomicUsize::new(0),
            });
        self
    }

    /// Return the list of Rholang codes that were passed to get_data/get_data_or_none.
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn find_response(&self, code: &str) -> Result<serde_json::Value, ReadNodeError> {
        self.calls.lock().unwrap().push(code.to_owned());
        for resp in self.responses.iter() {
            if code.contains(&resp.contains) {
                // Sequenced empty-then-data: the first `empty_before` matching
                // reads return `ReturnValueMissing`, so a retrying caller only
                // succeeds once the simulated visibility lag clears.
                let hit = resp.hits.fetch_add(1, Ordering::SeqCst);
                if hit < resp.empty_before {
                    return Err(ReadNodeError::ReturnValueMissing);
                }
                if let Some(ref err) = resp.error {
                    return Err(match err {
                        ReadNodeError::ReturnValueMissing => ReadNodeError::ReturnValueMissing,
                        ReadNodeError::Api(status, body) => {
                            ReadNodeError::Api(*status, body.clone())
                        }
                        ReadNodeError::Timeout(d) => ReadNodeError::Timeout(*d),
                        _ => ReadNodeError::ReturnValueMissing,
                    });
                }
                return Ok(resp.value.clone());
            }
        }
        // Default: return missing
        Err(ReadNodeError::ReturnValueMissing)
    }
}

impl ReadNode for MockReadNode {
    async fn get_data<T: serde::de::DeserializeOwned + Send>(
        &self,
        rholang_code: String,
    ) -> Result<T, ReadNodeError> {
        let value = self.find_response(&rholang_code)?;
        serde_json::from_value(value).map_err(|e| ReadNodeError::Deserialization(e.into()))
    }

    async fn get_data_or_none<T: serde::de::DeserializeOwned + Send>(
        &self,
        rholang_code: String,
    ) -> Result<Option<T>, ReadNodeError> {
        match self.get_data(rholang_code).await {
            Ok(data) => Ok(Some(data)),
            Err(ReadNodeError::ReturnValueMissing) => Ok(None),
            Err(err) => Err(err),
        }
    }

    async fn get_data_with_retry<T: serde::de::DeserializeOwned + Send>(
        &self,
        rholang_code: String,
        max_retries: u32,
        _delay: Duration,
    ) -> Result<T, ReadNodeError> {
        // Faithfully model the real client: retry only on an empty result
        // (`ReturnValueMissing`), up to `max_retries` times. `delay` is ignored
        // so tests stay fast.
        let mut attempts = 0u32;
        loop {
            match self.get_data(rholang_code.clone()).await {
                Err(ReadNodeError::ReturnValueMissing) if attempts < max_retries => {
                    attempts += 1;
                }
                result => return result,
            }
        }
    }

    async fn get_data_or_none_with_retry<T: serde::de::DeserializeOwned + Send>(
        &self,
        rholang_code: String,
        max_retries: u32,
        delay: Duration,
    ) -> Result<Option<T>, ReadNodeError> {
        match self.get_data_with_retry(rholang_code, max_retries, delay).await {
            Ok(data) => Ok(Some(data)),
            Err(ReadNodeError::ReturnValueMissing) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

// -------------------------------------------------------
// MockWriteNode
// -------------------------------------------------------

/// A mock write node client that returns pre-configured deploy IDs.
#[derive(Clone)]
pub struct MockWriteNode {
    head_block_index: u64,
    deploy_responses: Arc<Mutex<VecDeque<anyhow::Result<DeployId>>>>,
    deployed_contracts: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl MockWriteNode {
    pub fn new() -> Self {
        Self {
            head_block_index: 0,
            deploy_responses: Arc::new(Mutex::new(VecDeque::new())),
            deployed_contracts: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_head_block_index(mut self, index: u64) -> Self {
        self.head_block_index = index;
        self
    }

    pub fn with_deploy_response(self, response: anyhow::Result<DeployId>) -> Self {
        self.deploy_responses.lock().unwrap().push_back(response);
        self
    }

    pub fn deployed_contracts(&self) -> Vec<Vec<u8>> {
        self.deployed_contracts.lock().unwrap().clone()
    }

    fn next_deploy_response(&self) -> anyhow::Result<DeployId> {
        self.deploy_responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Ok(test_deploy_id()))
    }
}

impl WriteNode for MockWriteNode {
    async fn deploy(
        &mut self,
        _key: &SecretKey,
        _deploy_data: DeployData,
    ) -> anyhow::Result<DeployId> {
        self.next_deploy_response()
    }

    async fn deploy_signed_contract(&mut self, contract: SignedCode) -> anyhow::Result<DeployId> {
        self.deployed_contracts
            .lock()
            .unwrap()
            .push(contract.contract.clone());
        self.next_deploy_response()
    }

    async fn full_deploy(
        &mut self,
        key: &SecretKey,
        deploy_data: DeployData,
    ) -> anyhow::Result<DeployId> {
        self.deploy(key, deploy_data).await
    }

    async fn get_head_block_index(&mut self) -> anyhow::Result<u64> {
        Ok(self.head_block_index)
    }
}

// -------------------------------------------------------
// MockNodeEventSource
// -------------------------------------------------------

/// A mock node event source that returns pre-configured results.
#[derive(Clone)]
pub struct MockNodeEventSource {
    wait_results: Arc<Mutex<std::collections::HashMap<String, Option<bool>>>>,
}

impl MockNodeEventSource {
    pub fn new() -> Self {
        Self {
            wait_results: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Configure wait_for_deploy to return the given result for the given deploy ID.
    pub fn with_wait_result(self, deploy_id: &str, result: Option<bool>) -> Self {
        self.wait_results
            .lock()
            .unwrap()
            .insert(deploy_id.to_owned(), result);
        self
    }
}

impl NodeEventSource for MockNodeEventSource {
    fn wait_for_deploy(
        &self,
        deploy_id: &DeployId,
        _max_wait: Duration,
    ) -> impl std::future::Future<Output = Option<bool>> + Send {
        let result = self
            .wait_results
            .lock()
            .unwrap()
            .get(&deploy_id.to_string())
            .copied()
            .unwrap_or(Some(false));
        async move { result }
    }

    fn subscribe_for_deploys(
        &self,
        _wallet_address: WalletAddress,
    ) -> impl futures::Stream<Item = DeployEvent> + Send {
        stream::empty()
    }
}

// -------------------------------------------------------
// Helper functions
// -------------------------------------------------------

pub fn test_secret_key() -> SecretKey {
    SecretKey::from_byte_array([1u8; 32]).expect("valid secret key")
}

pub fn test_public_key() -> PublicKey {
    let secp = Secp256k1::new();
    PublicKey::from_secret_key(&secp, &test_secret_key())
}

pub fn test_uri() -> Uri {
    test_public_key().into()
}

pub fn test_deploy_id() -> DeployId {
    DeployId::from("test-deploy-id".to_owned())
}

pub fn test_signed_code() -> SignedCode {
    SignedCode {
        contract: vec![0; 16],
        sig: vec![0; 64],
        sig_algorithm: "secp256k1".into(),
        deployer: vec![0; 65],
    }
}
