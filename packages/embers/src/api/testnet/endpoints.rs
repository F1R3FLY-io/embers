use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::payload::Json;

use crate::api::common::{ApiTags, PrepareResponse, SendRequest};
use crate::api::testnet::models::{
    CreateTestwalletResp,
    DeploySignedTestReq,
    DeploySignedTestResp,
    DeployTestReq,
    DeployTestResp,
};
use crate::domain::testnet::TestnetService;

#[derive(Debug, Clone)]
pub struct TestnetApi;

#[OpenApi(prefix_path = "/testnet", tag = ApiTags::Testnet)]
impl TestnetApi {
    #[oai(path = "/wallet", method = "post")]
    async fn create_wallet(
        &self,
        Data(testnet): Data<&TestnetService>,
    ) -> poem::Result<Json<CreateTestwalletResp>> {
        let wallet = testnet.create_wallet().await?;
        Ok(Json(wallet.into()))
    }

    #[oai(path = "/deploy/prepare", method = "post")]
    async fn prepare_deploy(
        &self,
        Json(body): Json<DeployTestReq>,
        Data(testnet): Data<&TestnetService>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> poem::Result<Json<PrepareResponse<DeployTestResp>>> {
        PrepareResponse::from_call(
            body,
            |body| testnet.prepare_test_contract(body.into()),
            encoding_key,
        )
        .await
        .map(Json)
        .map_err(Into::into)
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
