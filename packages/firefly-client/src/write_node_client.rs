use anyhow::{Context, anyhow};
use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use prost::Message as _;
use secp256k1::{Message, Secp256k1, SecretKey};

use crate::helpers::FromExpr;
use crate::models::casper::v1::deploy_service_client::DeployServiceClient;
use crate::models::casper::v1::propose_service_client::ProposeServiceClient;
use crate::models::casper::v1::{deploy_response, propose_response, rho_data_response};
use crate::models::casper::{DataAtNameByBlockQuery, DeployDataProto, ProposeQuery};
use crate::models::rhoapi::expr::ExprInstance;
use crate::models::rhoapi::{Expr, Par};
use crate::models::{BlockId, DeployData, DeployId, SignedCode};

#[derive(Clone)]
pub struct WriteNodeClient {
    deploy_client: DeployServiceClient<tonic::transport::Channel>,
    propose_client: ProposeServiceClient<tonic::transport::Channel>,
}

impl WriteNodeClient {
    pub async fn new(
        deploy_service_url: String,
        propose_service_url: String,
    ) -> anyhow::Result<Self> {
        let deploy_client = DeployServiceClient::connect(deploy_service_url)
            .await
            .context("failed to connect to deploy service")?;

        let propose_client = ProposeServiceClient::connect(propose_service_url)
            .await
            .context("failed to connect to propose service")?;

        Ok(Self {
            deploy_client,
            propose_client,
        })
    }

    pub async fn deploy(
        &mut self,
        key: &SecretKey,
        deploy_data: DeployData,
    ) -> anyhow::Result<DeployId> {
        let msg = {
            let timestamp = chrono::Utc::now().timestamp_millis();
            let mut msg = DeployDataProto {
                term: deploy_data.term,
                timestamp,
                phlo_price: 1,
                phlo_limit: deploy_data.phlo_limit,
                valid_after_block_number: 0,
                shard_id: "root".into(),
                ..Default::default()
            };

            let secp = Secp256k1::new();

            let hash = Blake2b::<U32>::new()
                .chain_update(msg.encode_to_vec())
                .finalize();

            let signature = secp.sign_ecdsa(Message::from_digest(hash.into()), key);

            msg.sig = signature.serialize_der().to_vec();
            msg.sig_algorithm = "secp256k1".into();

            let public_key = key.public_key(&secp);
            msg.deployer = public_key.serialize_uncompressed().into();
            msg
        };

        let resp = self
            .deploy_client
            .do_deploy(msg)
            .await?
            .into_inner()
            .message
            .context("missing do_deploy responce")?;

        let deploy_id = match resp {
            deploy_response::Message::Result(deploy_id) => deploy_id,
            deploy_response::Message::Error(err) => {
                return Err(anyhow!("do_deploy error: {err:?}"));
            }
        };

        deploy_id
            .strip_prefix("Success! DeployId is: ")
            .map(|id| DeployId::from(id.to_owned()))
            .context("failed to extract deploy_id")
    }

    pub async fn deploy_signed_contract(
        &mut self,
        contract: SignedCode,
    ) -> anyhow::Result<DeployId> {
        let msg = {
            let mut msg = DeployDataProto::decode(contract.contract.as_slice())?;

            msg.sig = contract.sig;
            msg.sig_algorithm = contract.sig_algorithm;
            msg.deployer = contract.deployer;

            msg
        };

        let resp = self
            .deploy_client
            .do_deploy(msg)
            .await?
            .into_inner()
            .message
            .context("missing do_deploy responce")?;

        let deploy_id = match resp {
            deploy_response::Message::Result(deploy_id) => deploy_id,
            deploy_response::Message::Error(err) => {
                return Err(anyhow!("do_deploy error: {err:?}"));
            }
        };

        deploy_id
            .strip_prefix("Success! DeployId is: ")
            .map(|id| DeployId::from(id.to_owned()))
            .context("failed to extract deploy_id")
    }

    pub async fn propose(&mut self) -> anyhow::Result<BlockId> {
        let resp = self
            .propose_client
            .propose(ProposeQuery { is_async: false })
            .await
            .context("propose grpc error")?
            .into_inner()
            .message
            .context("missing propose responce")?;

        let block_hash = match resp {
            propose_response::Message::Result(block_hash) => block_hash,
            propose_response::Message::Error(err) => return Err(anyhow!("propose error: {err:?}")),
        };

        block_hash
            .strip_prefix("Success! Block ")
            .and_then(|block_hash| block_hash.strip_suffix(" created and added."))
            .map(|id| BlockId::from(id.to_owned()))
            .context("failed to extract block hash")
    }

    pub async fn full_deploy(
        &mut self,
        key: &SecretKey,
        deploy_data: DeployData,
    ) -> anyhow::Result<BlockId> {
        self.deploy(key, deploy_data).await?;
        self.propose().await
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
