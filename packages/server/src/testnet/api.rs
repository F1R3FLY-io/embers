use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::payload::Json;

use crate::common::api::dtos::ApiTags;
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
        let mut testnet = testnet.clone();
        let wallet = testnet.create_test_wallet().await?;
        Ok(Json(wallet.into()))
    }

    #[oai(path = "/deploy/prepare", method = "post")]
    async fn prepare_deploy(
        &self,
        Json(input): Json<DeployTestReq>,
        Data(testnet): Data<&TestnetService>,
    ) -> poem::Result<Json<DeployTestResp>> {
        let mut testnet = testnet.clone();
        let contracts = testnet.prepare_test_contract(input.into()).await?;
        Ok(Json(contracts.into()))
    }

    #[oai(path = "/deploy/send", method = "post")]
    async fn deploy(
        &self,
        Json(input): Json<DeploySignedTestReq>,
        Data(testnet): Data<&TestnetService>,
    ) -> poem::Result<Json<DeploySignedTestResp>> {
        let mut testnet = testnet.clone();
        let result = testnet.deploy_test_contract(input.into()).await?;
        Ok(Json(result.into()))
    }
}
