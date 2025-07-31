mod ai_agents;
mod common;
mod wallets;

use ai_agents::api::AIAgents;
use poem::test::TestClient;
use poem_openapi::OpenApiService;
use wallets::api::WalletsApi;

#[tokio::main]
async fn main() {
    let api = OpenApiService::new((WalletsApi, AIAgents), "Embers API", "0.1.0");

    let spec = api.spec_endpoint();

    fn save_to_file<T: AsRef<[u8]>>(content: T) -> std::result::Result<(), std::io::Error> {
        std::fs::write("schema.json", content)
    }

    let _ = TestClient::new(spec)
        .get("/schema.json")
        .send()
        .await
        .0
        .into_body()
        .into_bytes()
        .await
        .map(save_to_file)
        .unwrap();
}
