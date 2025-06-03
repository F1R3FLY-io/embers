use poem_openapi::{Object, Union};

#[derive(Debug, Clone, Object)]
pub struct CreateAgentReq {
    name: String,
    shard: Option<String>,
    filesystem: Option<Directory>,
}

#[derive(Debug, Clone, Union)]
#[oai(discriminator_name = "type", rename_all = "lowercase")]
pub enum Filesystem {
    Directory(Directory),
    File(File),
}

#[derive(Debug, Clone, Object)]
pub struct Directory {
    name: String,
    members: Vec<Filesystem>,
}

#[derive(Debug, Clone, Object)]
pub struct File {
    name: String,
    content: String,
}
