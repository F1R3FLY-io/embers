//! The fake validator: a `tonic` [`DeployService`] server on loopback.
//!
//! It implements the exact generated service the real
//! [`WriteNodeClient`](firefly_client::WriteNodeClient) talks to (reusing the
//! protos from `firefly_client::models`, never a divergent copy). Only the three
//! methods the client actually calls carry behaviour — `do_deploy`,
//! `show_main_chain`, `get_data_at_name`. Every other method answers a truthful
//! gRPC `unimplemented` status: this is a test double, not a node.

use std::pin::Pin;

use anyhow::Context as _;
use firefly_client::models::casper::v1::deploy_service_server::{
    DeployService,
    DeployServiceServer,
};
use firefly_client::models::casper::v1::{
    BlockInfoResponse,
    DeployResponse,
    RhoDataPayload,
    RhoDataResponse,
    VisualizeBlocksResponse,
    block_info_response,
    deploy_response,
    rho_data_response,
};
use firefly_client::models::servicemodelapi::ServiceError;
use firefly_client::models::{casper, rhoapi};
use futures::Stream;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status};

use crate::Spawned;
use crate::control::{DeployControl, DeployOutcome};

type BlockInfoStream = Pin<Box<dyn Stream<Item = Result<BlockInfoResponse, Status>> + Send>>;
type VisualizeStream = Pin<Box<dyn Stream<Item = Result<VisualizeBlocksResponse, Status>> + Send>>;

/// Bind an ephemeral loopback port, start the fake validator on it, and return
/// its address plus shutdown/join handles.
pub(crate) async fn spawn(deploy: DeployControl) -> anyhow::Result<Spawned> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .context("failed to bind fake validator gRPC listener")?;
    let addr = listener
        .local_addr()
        .context("failed to read fake validator gRPC address")?;

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let service = FakeDeployService { deploy };
    let incoming = TcpListenerStream::new(listener);

    let handle = tokio::spawn(async move {
        let shutdown = async move {
            let _ = shutdown_rx.await;
        };
        if let Err(err) = tonic::transport::Server::builder()
            .add_service(DeployServiceServer::new(service))
            .serve_with_incoming_shutdown(incoming, shutdown)
            .await
        {
            tracing::debug!("fake validator gRPC server ended: {err}");
        }
    });

    Ok(Spawned {
        addr,
        shutdown: shutdown_tx,
        handle,
    })
}

struct FakeDeployService {
    deploy: DeployControl,
}

/// Extract the channel name the client is reading (it encodes the channel as a
/// single `GString` expression in the query `Par`).
fn channel_name(query: &casper::DataAtNameByBlockQuery) -> Option<String> {
    match query.par.as_ref()?.exprs.first()?.expr_instance.as_ref()? {
        rhoapi::expr::ExprInstance::GString(name) => Some(name.clone()),
        _ => None,
    }
}

fn unimplemented(method: &str) -> Status {
    Status::unimplemented(format!("firefly-test-node: {method} is not implemented"))
}

#[tonic::async_trait]
#[allow(non_camel_case_types)]
impl DeployService for FakeDeployService {
    async fn do_deploy(
        &self,
        request: Request<casper::DeployDataProto>,
    ) -> Result<Response<DeployResponse>, Status> {
        self.deploy.record_deploy_term(request.into_inner().term);
        let message = match self.deploy.deploy_outcome() {
            DeployOutcome::Accepted(id) => {
                deploy_response::Message::Result(format!("Success! DeployId is: {id}"))
            }
            DeployOutcome::Error(messages) => {
                deploy_response::Message::Error(ServiceError { messages })
            }
        };
        Ok(Response::new(DeployResponse {
            message: Some(message),
        }))
    }

    type showMainChainStream = BlockInfoStream;

    async fn show_main_chain(
        &self,
        _request: Request<casper::BlocksQuery>,
    ) -> Result<Response<Self::showMainChainStream>, Status> {
        let block = casper::LightBlockInfo {
            block_number: self.deploy.head_block_number() as i64,
            ..Default::default()
        };
        let response = BlockInfoResponse {
            message: Some(block_info_response::Message::BlockInfo(block)),
        };
        let stream: BlockInfoStream = Box::pin(tokio_stream::once(Ok(response)));
        Ok(Response::new(stream))
    }

    async fn get_data_at_name(
        &self,
        request: Request<casper::DataAtNameByBlockQuery>,
    ) -> Result<Response<RhoDataResponse>, Status> {
        let query = request.into_inner();
        self.deploy.record_read_block_hash(query.block_hash.clone());
        let channel = channel_name(&query);
        let message = self.deploy.channel_par(channel.as_deref()).map_or_else(
            || {
                rho_data_response::Message::Error(ServiceError {
                    messages: vec![format!(
                        "firefly-test-node: no value for channel {channel:?}"
                    )],
                })
            },
            |par| {
                rho_data_response::Message::Payload(RhoDataPayload {
                    par: vec![par],
                    block: None,
                })
            },
        );
        Ok(Response::new(RhoDataResponse {
            message: Some(message),
        }))
    }

    type visualizeDagStream = VisualizeStream;

    async fn visualize_dag(
        &self,
        _request: Request<casper::VisualizeDagQuery>,
    ) -> Result<Response<Self::visualizeDagStream>, Status> {
        Err(unimplemented("visualize_dag"))
    }

    type getBlocksStream = BlockInfoStream;

    async fn get_blocks(
        &self,
        _request: Request<casper::BlocksQuery>,
    ) -> Result<Response<Self::getBlocksStream>, Status> {
        Err(unimplemented("get_blocks"))
    }

    type getBlocksByHeightsStream = BlockInfoStream;

    async fn get_blocks_by_heights(
        &self,
        _request: Request<casper::BlocksQueryByHeight>,
    ) -> Result<Response<Self::getBlocksByHeightsStream>, Status> {
        Err(unimplemented("get_blocks_by_heights"))
    }

    async fn get_block(
        &self,
        _request: Request<casper::BlockQuery>,
    ) -> Result<Response<casper::v1::BlockResponse>, Status> {
        Err(unimplemented("get_block"))
    }

    async fn machine_verifiable_dag(
        &self,
        _request: Request<casper::MachineVerifyQuery>,
    ) -> Result<Response<casper::v1::MachineVerifyResponse>, Status> {
        Err(unimplemented("machine_verifiable_dag"))
    }

    async fn listen_for_continuation_at_name(
        &self,
        _request: Request<casper::ContinuationAtNameQuery>,
    ) -> Result<Response<casper::v1::ContinuationAtNameResponse>, Status> {
        Err(unimplemented("listen_for_continuation_at_name"))
    }

    async fn find_deploy(
        &self,
        _request: Request<casper::FindDeployQuery>,
    ) -> Result<Response<casper::v1::FindDeployResponse>, Status> {
        Err(unimplemented("find_deploy"))
    }

    async fn preview_private_names(
        &self,
        _request: Request<casper::PrivateNamePreviewQuery>,
    ) -> Result<Response<casper::v1::PrivateNamePreviewResponse>, Status> {
        Err(unimplemented("preview_private_names"))
    }

    async fn last_finalized_block(
        &self,
        _request: Request<casper::LastFinalizedBlockQuery>,
    ) -> Result<Response<casper::v1::LastFinalizedBlockResponse>, Status> {
        Err(unimplemented("last_finalized_block"))
    }

    async fn is_finalized(
        &self,
        _request: Request<casper::IsFinalizedQuery>,
    ) -> Result<Response<casper::v1::IsFinalizedResponse>, Status> {
        Err(unimplemented("is_finalized"))
    }

    async fn deploy_finalization_status(
        &self,
        _request: Request<casper::DeployFinalizationStatusQuery>,
    ) -> Result<Response<casper::v1::DeployFinalizationStatusResponse>, Status> {
        Err(unimplemented("deploy_finalization_status"))
    }

    async fn bond_status(
        &self,
        _request: Request<casper::BondStatusQuery>,
    ) -> Result<Response<casper::v1::BondStatusResponse>, Status> {
        Err(unimplemented("bond_status"))
    }

    async fn exploratory_deploy(
        &self,
        _request: Request<casper::ExploratoryDeployQuery>,
    ) -> Result<Response<casper::v1::ExploratoryDeployResponse>, Status> {
        Err(unimplemented("exploratory_deploy"))
    }

    async fn get_event_by_hash(
        &self,
        _request: Request<casper::ReportQuery>,
    ) -> Result<Response<casper::v1::EventInfoResponse>, Status> {
        Err(unimplemented("get_event_by_hash"))
    }

    async fn status(
        &self,
        _request: Request<()>,
    ) -> Result<Response<casper::v1::StatusResponse>, Status> {
        Err(unimplemented("status"))
    }
}
