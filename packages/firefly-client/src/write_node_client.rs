use anyhow::{Context, anyhow};
use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use futures::TryStreamExt;
use prost::Message as _;
use secp256k1::{Message, Secp256k1, SecretKey};

use crate::helpers::FromExpr;
use crate::models::casper::v1::deploy_service_client::DeployServiceClient;
use crate::models::casper::v1::{
    block_info_response,
    deploy_response,
    find_deploy_response,
    rho_data_response,
};
use crate::models::casper::{
    BlocksQuery,
    DataAtNameByBlockQuery,
    DeployDataProto,
    FindDeployQuery,
};
use crate::models::rhoapi::expr::ExprInstance;
use crate::models::rhoapi::{Expr, Par};
use crate::models::{BlockId, DeployData, DeployId, SignedCode, ValidAfter};

const DEPLOY_ID_PREFIX: &str = "DeployId is: ";

fn extract_deploy_id(response: &str) -> anyhow::Result<DeployId> {
    response
        .find(DEPLOY_ID_PREFIX)
        .map(|pos| DeployId::from(response[pos + DEPLOY_ID_PREFIX.len()..].trim().to_owned()))
        .context("failed to extract deploy_id")
}

#[derive(Clone)]
pub struct WriteNodeClient {
    deploy_client: DeployServiceClient<tonic::transport::Channel>,
}

impl WriteNodeClient {
    pub async fn new(deploy_service_url: String) -> anyhow::Result<Self> {
        let deploy_client = DeployServiceClient::connect(deploy_service_url)
            .await
            .context("failed to connect to deploy service")?;

        Ok(Self { deploy_client })
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

        let deploy_id = match resp {
            deploy_response::Message::Result(deploy_id) => deploy_id,
            deploy_response::Message::Error(err) => {
                return Err(anyhow!("do_deploy error: {err:?}"));
            }
        };

        extract_deploy_id(&deploy_id)
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

        let deploy_id = match resp {
            deploy_response::Message::Result(deploy_id) => deploy_id,
            deploy_response::Message::Error(err) => {
                return Err(anyhow!("do_deploy error: {err:?}"));
            }
        };

        extract_deploy_id(&deploy_id)
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

    pub async fn find_deploy(&mut self, deploy_id: &DeployId) -> anyhow::Result<BlockId> {
        let deploy_id_bytes = hex::decode(deploy_id.as_ref()).context("invalid deploy_id hex")?;

        let resp = self
            .deploy_client
            .find_deploy(FindDeployQuery {
                deploy_id: deploy_id_bytes.into(),
            })
            .await
            .context("find_deploy grpc error")?
            .into_inner()
            .message
            .context("missing find_deploy response")?;

        match resp {
            find_deploy_response::Message::BlockInfo(info) => Ok(BlockId::from(info.block_hash)),
            find_deploy_response::Message::Error(err) => Err(anyhow!("find_deploy error: {err:?}")),
        }
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
