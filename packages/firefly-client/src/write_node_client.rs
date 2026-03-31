use anyhow::{Context, anyhow};
use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use futures::TryStreamExt;
use prost::Message as _;
use secp256k1::{Message, Secp256k1, SecretKey};

use crate::helpers::FromExpr;
use crate::models::casper::v1::deploy_service_client::DeployServiceClient;
use crate::traits::WriteNode;
use crate::models::casper::v1::{
    block_info_response,
    deploy_response,
    rho_data_response,
};
use crate::models::casper::{BlocksQuery, DataAtNameByBlockQuery, DeployDataProto};
use crate::models::rhoapi::expr::ExprInstance;
use crate::models::rhoapi::{Expr, Par};
use crate::models::{BlockId, DeployData, DeployId, SignedCode, ValidAfter};

#[derive(Clone)]
pub struct WriteNodeClient {
    deploy_client: DeployServiceClient<tonic::transport::Channel>,
}

/// Extract a deploy ID from the node's response string.
/// The Scala node returns `"Success! DeployId is: <id>"` while the
/// Rust node returns `"Success!\nDeployId is: <id>"`.
fn extract_deploy_id(response: &str) -> anyhow::Result<DeployId> {
    response
        .strip_prefix("Success! DeployId is: ")
        .or_else(|| response.strip_prefix("Success!\nDeployId is: "))
        .map(|id| DeployId::from(id.to_owned()))
        .context(format!("failed to extract deploy_id from response: {response:?}"))
}

impl WriteNodeClient {
    pub async fn new(
        deploy_service_url: String,
    ) -> anyhow::Result<Self> {
        let deploy_client = DeployServiceClient::connect(deploy_service_url)
            .await
            .context("failed to connect to deploy service")?;

        Ok(Self {
            deploy_client,
        })
    }

    pub async fn deploy(
        &mut self,
        key: &SecretKey,
        deploy_data: DeployData,
    ) -> anyhow::Result<DeployId> {
        let valid_after_block_number = match deploy_data.valid_after_block_number {
            ValidAfter::Head => self.get_head_block_index().await?,
            ValidAfter::Index(i) => i,
        };

        let mut msg = DeployDataProto {
            term: deploy_data.term,
            timestamp: deploy_data.timestamp.timestamp_millis(),
            phlo_price: 1,
            phlo_limit: deploy_data.phlo_limit as _,
            valid_after_block_number: valid_after_block_number as _,
            shard_id: "root".into(),
            ..Default::default()
        };

        let secp = Secp256k1::new();

        let hash = Blake2b::<U32>::new()
            .chain_update(msg.encode_to_vec())
            .finalize();

        let signature = secp.sign_ecdsa(Message::from_digest(hash.into()), key);

        msg.sig = signature.serialize_der().to_vec().into();
        msg.sig_algorithm = "secp256k1".into();

        let public_key = key.public_key(&secp);
        msg.deployer = public_key.serialize_uncompressed().to_vec().into();

        let resp = self
            .deploy_client
            .do_deploy(msg)
            .await?
            .into_inner()
            .message
            .context("missing do_deploy responce")?;

        match resp {
            deploy_response::Message::Result(msg) => extract_deploy_id(&msg),
            deploy_response::Message::Error(err) => {
                Err(anyhow!("do_deploy error: {err:?}"))
            }
        }
    }

    pub async fn deploy_signed_contract(
        &mut self,
        contract: SignedCode,
    ) -> anyhow::Result<DeployId> {
        let mut msg = DeployDataProto::decode(contract.contract.as_slice())?;

        msg.sig = contract.sig.into();
        msg.sig_algorithm = contract.sig_algorithm;
        msg.deployer = contract.deployer.into();

        let resp = self
            .deploy_client
            .do_deploy(msg)
            .await?
            .into_inner()
            .message
            .context("missing do_deploy responce")?;

        match resp {
            deploy_response::Message::Result(msg) => extract_deploy_id(&msg),
            deploy_response::Message::Error(err) => {
                Err(anyhow!("do_deploy error: {err:?}"))
            }
        }
    }

    pub async fn full_deploy(
        &mut self,
        key: &SecretKey,
        deploy_data: DeployData,
    ) -> anyhow::Result<DeployId> {
        self.deploy(key, deploy_data).await
    }

    pub async fn get_head_block_index(&mut self) -> anyhow::Result<u64> {
        let mut stream = self
            .deploy_client
            .show_main_chain(BlocksQuery { depth: 1 })
            .await?
            .into_inner();

        stream
            .try_next()
            .await?
            .and_then(|block| block.message)
            .map_or(Ok(0), |m| match m {
                block_info_response::Message::Error(err) => {
                    Err(anyhow!("show_main_chain error: {err:?}"))
                }
                block_info_response::Message::BlockInfo(light_block_info) => {
                    Ok(light_block_info.block_number as _)
                }
            })
    }

    pub async fn get_channel_value<T>(
        &mut self,
        hash: BlockId,
        channel: String,
    ) -> anyhow::Result<T>
    where
        T: FromExpr,
    {
        let mut par = Par::default();
        par.exprs.push(Expr {
            expr_instance: Some(ExprInstance::GString(channel)),
        });

        let resp = self
            .deploy_client
            .get_data_at_name(DataAtNameByBlockQuery {
                par: Some(par),
                block_hash: hash.into(),
                use_pre_state_hash: false,
            })
            .await
            .context("get_data_at_name grpc error")?
            .into_inner()
            .message
            .context("missing get_data_at_name responce")?;

        let payload = match resp {
            rho_data_response::Message::Payload(payload) => payload,
            rho_data_response::Message::Error(err) => {
                return Err(anyhow!("get_data_at_name error: {err:?}"));
            }
        };

        let par = payload
            .par
            .into_iter()
            .next_back()
            .context("missing par in get_data_at_name")?;
        let expr = par
            .exprs
            .into_iter()
            .next()
            .context("missing exprs in get_data_at_name")?;
        let expr = expr
            .expr_instance
            .context("missing expr_instance in get_data_at_name")?;

        T::from(expr)
    }
}

impl WriteNode for WriteNodeClient {
    async fn deploy(
        &mut self,
        key: &SecretKey,
        deploy_data: DeployData,
    ) -> anyhow::Result<DeployId> {
        self.deploy(key, deploy_data).await
    }

    async fn deploy_signed_contract(
        &mut self,
        contract: SignedCode,
    ) -> anyhow::Result<DeployId> {
        self.deploy_signed_contract(contract).await
    }

    async fn full_deploy(
        &mut self,
        key: &SecretKey,
        deploy_data: DeployData,
    ) -> anyhow::Result<DeployId> {
        self.full_deploy(key, deploy_data).await
    }

    async fn get_head_block_index(&mut self) -> anyhow::Result<u64> {
        self.get_head_block_index().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------
    // extract_deploy_id tests (pure function)
    // -------------------------------------------------------

    #[test]
    fn test_extract_deploy_id_scala_format() {
        let result =
            extract_deploy_id("Success! DeployId is: abc123def456").expect("should parse");
        assert_eq!(result, DeployId::from("abc123def456".to_owned()));
    }

    #[test]
    fn test_extract_deploy_id_rust_node_format() {
        let result =
            extract_deploy_id("Success!\nDeployId is: abc123def456").expect("should parse");
        assert_eq!(result, DeployId::from("abc123def456".to_owned()));
    }

    #[test]
    fn test_extract_deploy_id_error_response() {
        let result = extract_deploy_id("Error: something went wrong");
        assert!(result.is_err(), "expected error for error response");
    }

    #[test]
    fn test_extract_deploy_id_empty() {
        let result = extract_deploy_id("");
        assert!(result.is_err(), "expected error for empty string");
    }

    #[test]
    fn test_extract_deploy_id_partial_prefix_no_trailing_space() {
        // The prefix includes a trailing space: "Success! DeployId is: "
        // Without that space, the prefix doesn't match
        let result = extract_deploy_id("Success! DeployId is:");
        assert!(result.is_err(), "expected error when trailing space is missing");
    }

    #[test]
    fn test_extract_deploy_id_with_whitespace() {
        let result = extract_deploy_id("Success! DeployId is:  spaced_id  ")
            .expect("should parse with surrounding spaces");
        assert_eq!(result, DeployId::from(" spaced_id  ".to_owned()));
    }

    // -------------------------------------------------------
    // DeployData construction tests
    // -------------------------------------------------------

    #[test]
    fn test_deploy_data_builder_defaults() {
        let now = chrono::Utc::now();
        let data = DeployData::builder("new Nil".into()).build();

        assert_eq!(data.term, "new Nil");
        assert_eq!(data.phlo_limit, 5_000_000);
        assert!(matches!(data.valid_after_block_number, ValidAfter::Head));
        // Timestamp should be very close to now
        let diff = (data.timestamp - now).num_milliseconds().unsigned_abs();
        assert!(diff < 1000, "timestamp should be close to now, diff={diff}ms");
    }

    #[test]
    fn test_deploy_data_builder_custom_values() {
        let ts = chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .to_utc();
        let data = DeployData::builder("code".into())
            .phlo_limit(1_000_000)
            .timestamp(ts)
            .valid_after_block_number(ValidAfter::Index(42))
            .build();

        assert_eq!(data.term, "code");
        assert_eq!(data.phlo_limit, 1_000_000);
        assert_eq!(data.timestamp, ts);
        assert!(matches!(
            data.valid_after_block_number,
            ValidAfter::Index(42)
        ));
    }

    // -------------------------------------------------------
    // SignedCode construction test
    // -------------------------------------------------------

    #[test]
    fn test_signed_code_fields() {
        let code = SignedCode {
            contract: vec![1, 2, 3],
            sig: vec![4, 5, 6],
            sig_algorithm: "secp256k1".into(),
            deployer: vec![7, 8, 9],
        };
        assert_eq!(code.contract, vec![1, 2, 3]);
        assert_eq!(code.sig, vec![4, 5, 6]);
        assert_eq!(code.sig_algorithm, "secp256k1");
        assert_eq!(code.deployer, vec![7, 8, 9]);
    }
}
