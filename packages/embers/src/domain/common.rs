use aes_gcm::aead::{Aead, AeadCore, KeyInit, Nonce, OsRng};
use aes_gcm::{Aes256Gcm, Key};
use atrium_api::agent::Agent;
use atrium_api::types::BlobRef;
use serde::{Deserialize, Serialize};

use crate::domain::ai_agents_teams::models::EncryptedMsg;

pub fn serialize_encrypted<T>(val: T, key: &Key<Aes256Gcm>) -> anyhow::Result<EncryptedMsg>
where
    T: Serialize,
{
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let data = serde_json::to_string(&val)?;
    let ciphertext = cipher.encrypt(&nonce, data.as_ref())?;

    Ok(EncryptedMsg {
        nonce: nonce.to_vec(),
        ciphertext,
    })
}

#[allow(unused)]
pub fn deserialize_decrypted<T>(
    EncryptedMsg { ciphertext, nonce }: EncryptedMsg,
    key: &Key<Aes256Gcm>,
) -> anyhow::Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let cipher = Aes256Gcm::new(key);
    #[allow(deprecated)]
    let nonce = Nonce::<Aes256Gcm>::from_exact_iter(nonce)
        .ok_or_else(|| anyhow::anyhow!("invalid nonce length"))?;
    let data = cipher.decrypt(&nonce, ciphertext.as_ref())?;
    serde_json::from_slice(&data).map_err(Into::into)
}

pub async fn upload_blob_from_url<S>(agent: &Agent<S>, url: &str) -> anyhow::Result<BlobRef>
where
    S: atrium_api::agent::SessionManager + Send + Sync,
{
    let resp = reqwest::get(url).await?;
    let bytes = resp.bytes().await?;
    let blob_ref = agent
        .api
        .com
        .atproto
        .repo
        .upload_blob(bytes.to_vec())
        .await?;

    Ok(blob_ref.data.blob)
}
