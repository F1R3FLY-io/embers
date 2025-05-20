use std::collections::VecDeque;

use anyhow::Context;
use futures::TryStreamExt;

use crate::models::{Block, BlockInfo, Deploy};

#[derive(Clone)]
pub struct BlocksClient {
    url: String,
    client: reqwest::Client,
}

impl BlocksClient {
    pub fn new(url: String) -> Self {
        Self {
            url,
            client: Default::default(),
        }
    }

    pub async fn get_deploys(&self) -> anyhow::Result<Vec<Deploy>> {
        async_stream::try_stream! {
            let head_blocks: Vec<BlockInfo> = self.get_data("blocks").await?;
            let mut hashes: VecDeque<_> = head_blocks
                .into_iter()
                .map(|block| block.block_hash)
                .collect();

            while let Some(hash) = hashes.pop_front() {
                let block: Block = self.get_data(&format!("block/{hash}")).await?;

                for deploy in block.deploys {
                    yield deploy;
                }

                hashes.extend(block.block_info.parents_hash_list);
            }
        }
        .try_collect()
        .await
    }

    async fn get_data<T>(&self, path: &str) -> anyhow::Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        self.client
            .get(format!("{}/api/{path}", self.url))
            .header("Content-Type", "text/plain")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .context("failed to parse responce as json")
    }
}
