use poem_openapi::Object;

#[derive(Debug, Clone, Object)]
pub struct DemoReq {
    pub name: String,
}
