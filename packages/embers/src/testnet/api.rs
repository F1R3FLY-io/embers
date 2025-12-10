use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::payload::Json;

use crate::common::api::dtos::{ApiTags, PrepareResponse, SendRequest};
use crate::testnet::api::dtos::{
    CreateTestwalletResp,
    DeploySignedTestReq,
    DeploySignedTestResp,
    DeployTestReq,
    DeployTestResp,
};
use crate::testnet::handlers::TestnetService;

mod dtos;

#[derive(Debug, Clone)]
pub struct Testnet;

#[OpenApi(prefix_path = "/testnet", tag = ApiTags::Testnet)]
impl Testnet {
    #[oai(path = "/wallet", method = "post")]
    async fn create_wallet(
        &self,
        Data(testnet): Data<&TestnetService>,
    ) -> poem::Result<Json<CreateTestwalletResp>> {
        let wallet = testnet.create_test_wallet().await?;
        Ok(Json(wallet.into()))
    }

    #[oai(path = "/deploy/prepare", method = "post")]
    async fn prepare_deploy(
        &self,
        Json(body): Json<DeployTestReq>,
        Data(testnet): Data<&TestnetService>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> poem::Result<Json<PrepareResponse<DeployTestResp>>> {
        let contracts = testnet.prepare_test_contract(body.clone().into()).await?;
        Ok(Json(PrepareResponse::new(
            &body,
            contracts.into(),
            encoding_key,
        )))
    }

    #[oai(path = "/deploy/send", method = "post")]
    async fn deploy(
        &self,
        SendRequest(body): SendRequest<DeploySignedTestReq, DeployTestReq, DeployTestResp>,
        Data(testnet): Data<&TestnetService>,
    ) -> poem::Result<Json<DeploySignedTestResp>> {
        let result = testnet.deploy_test_contract(body.request.into()).await?;
        Ok(Json(result.into()))
    }
}
