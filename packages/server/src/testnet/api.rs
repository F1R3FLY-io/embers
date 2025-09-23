use firefly_client::{ReadNodeClient, WriteNodeClient};
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::payload::Json;
use secp256k1::SecretKey;

use crate::common::api::dtos::{ApiTags, TestNet};
use crate::testnet::api::dtos::{
    CreateTestwalletResp,
    DeploySignedTestReq,
    DeploySignedTestResp,
    DeployTestReq,
    DeployTestResp,
};
use crate::testnet::handlers::{create_test_wallet, deploy_test_contract, prepare_test_contract};

mod dtos;

#[derive(Debug, Clone)]
pub struct Testnet;

#[OpenApi(prefix_path = "/testnet", tag = ApiTags::Testnet)]
impl Testnet {
    #[oai(path = "/wallet", method = "post")]
    async fn create_wallet(
        &self,
        Data(test_client): Data<&TestNet<WriteNodeClient>>,
        Data(test_service_key): Data<&TestNet<SecretKey>>,
    ) -> poem::Result<Json<CreateTestwalletResp>> {
        let mut test_client = test_client.0.clone();
        let test_client = create_test_wallet(&mut test_client, &test_service_key.0).await?;
        Ok(Json(test_client.into()))
    }

    #[oai(path = "/deploy/prepare", method = "post")]
    async fn prepare_deploy(
        &self,
        Json(input): Json<DeployTestReq>,
        Data(test_client): Data<&TestNet<WriteNodeClient>>,
    ) -> poem::Result<Json<DeployTestResp>> {
        let mut test_client = test_client.0.clone();
        let contracts = prepare_test_contract(input.into(), &mut test_client).await?;
        Ok(Json(contracts.into()))
    }

    #[oai(path = "/deploy/send", method = "post")]
    async fn deploy(
        &self,
        Json(input): Json<DeploySignedTestReq>,
        Data(test_client): Data<&TestNet<WriteNodeClient>>,
        Data(test_read_client): Data<&TestNet<ReadNodeClient>>,
    ) -> poem::Result<Json<DeploySignedTestResp>> {
        let mut test_client = test_client.0.clone();
        let result =
            deploy_test_contract(&mut test_client, &test_read_client.0, input.into()).await?;
        Ok(Json(result.into()))
    }
}
