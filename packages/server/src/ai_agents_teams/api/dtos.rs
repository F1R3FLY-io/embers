use poem_openapi::Object;

#[derive(Debug, Clone, Object)]
pub struct DeployDemoReq {
    pub name: String,
}

#[derive(Debug, Clone, Object)]
pub struct RunDemoReq {
    pub name: String,
    pub prompt: String,
}
