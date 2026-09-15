use std::sync::LazyLock;
use std::time::Duration;

use atrium_api::agent::Agent;
use atrium_api::agent::atp_agent::CredentialSession;
use atrium_api::agent::atp_agent::store::MemorySessionStore;
use atrium_api::app::bsky::actor::{Profile, profile};
use atrium_api::client::AtpServiceClient;
use atrium_api::com::atproto::{repo, server};
use atrium_api::record::KnownRecord;
use atrium_api::types::string::{AtIdentifier, Datetime, Handle, RecordKey};
use atrium_api::types::{Collection, TryIntoUnknown};
use atrium_xrpc_client::reqwest::ReqwestClient;
use firefly_client::models::{DeployId, SignedCode, Uri, WalletAddress};
use firefly_client::rendering::Render;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};
use futures::FutureExt;
use futures::future::OptionFuture;
use serde::{Deserialize, Serialize};

use crate::blockchain::agents_teams::models;
use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::models::{
    EncryptedMsg,
    FireskyCredentials,
    PublishToFireskyReq,
    PublishToFireskyResp,
};
use crate::domain::common::{
    prepare_for_signing,
    record_trace,
    serialize_encrypted,
    upload_blob_from_url,
};

/// An app-password name this account is not already using.
///
/// Names are unique per account, so a second `Embers` password is refused -- which
/// made publishing to an account that had been published to before fail at the last
/// step, after the config record had already been written. Numbering from the names
/// actually present keeps every publication holding its own credential, so revoking
/// one never silently disarms another.
///
/// Bounded rather than open-ended: an account carrying a hundred Embers passwords is
/// a symptom, not a case to accommodate, and atproto caps app passwords anyway.
async fn free_app_password_name<S>(agent: &Agent<S>) -> anyhow::Result<String>
where
    S: atrium_api::agent::SessionManager + Send + Sync,
{
    const BASE: &str = "Embers";
    const LIMIT: usize = 100;

    let taken: std::collections::HashSet<String> = agent
        .api
        .com
        .atproto
        .server
        .list_app_passwords()
        .await?
        .data
        .passwords
        .into_iter()
        .map(|password| password.data.name)
        .collect();

    if !taken.contains(BASE) {
        return Ok(BASE.to_owned());
    }

    (2..=LIMIT)
        .map(|n| format!("{BASE} {n}"))
        .find(|name| !taken.contains(name))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "this account already holds {LIMIT} Embers app passwords; revoke the \
                 ones no longer in use before publishing again"
            )
        })
}

pub(super) static SELF_RKEY: LazyLock<RecordKey> =
    LazyLock::new(|| RecordKey::new("self".into()).unwrap());

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/save_firesky_token.rho")]
struct SaveFireskyToken {
    env_uri: Uri,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    #[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
    pub async fn prepare_publish_to_firesky_contract(
        &self,
        address: WalletAddress,
        id: String,
        mut request: PublishToFireskyReq,
    ) -> anyhow::Result<PublishToFireskyResp> {
        use crate::domain::validation::ValidationError;
        let existing_mode = request.account_mode.as_deref() == Some("existing");
        if !matches!(
            request.account_mode.as_deref(),
            None | Some("create" | "existing")
        ) {
            return Err(ValidationError::field(
                "account_mode",
                "invalid_account_mode",
                "Choose Create account or Use existing account.",
            )
            .into());
        }
        let pds = reqwest::Url::parse(request.pds_url.trim())
            .ok()
            .filter(|url| {
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
            });
        let pds = pds.ok_or_else(|| {
            ValidationError::field(
                "pds_url",
                "invalid_pds_url",
                "Enter the PDS's HTTP or HTTPS address.",
            )
        })?;
        request.pds_url = pds.as_str().trim_end_matches('/').to_owned();
        request.handle = request.handle.trim().trim_start_matches('@').to_lowercase();
        let domains = discover_pds(&request.pds_url).await?;
        if !request.handle.contains('.') {
            let suffix = domains
                .get("availableUserDomains")
                .and_then(serde_json::Value::as_array)
                .and_then(|values| values.first())
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    ValidationError::field(
                        "handle",
                        "handle_suffix_required",
                        "Enter a complete handle including its domain suffix.",
                    )
                })?;
            request.handle = format!("{}.{}", request.handle, suffix.trim_start_matches('.'));
        }
        let handle = Handle::new(request.handle.clone()).map_err(|_| ValidationError::field("handle", "invalid_handle", "Enter a valid complete handle, including the PDS suffix (for example redblue.test)."))?;
        if request.password.is_empty() {
            return Err(ValidationError::field(
                "password",
                "password_required",
                "Enter the team account password.",
            )
            .into());
        }
        if !existing_mode
            && (request
                .email
                .trim()
                .split_once('@')
                .is_none_or(|(local, domain)| local.is_empty() || !domain.contains('.'))
                || request.email.chars().any(char::is_whitespace))
        {
            return Err(ValidationError::field(
                "email",
                "invalid_email",
                "Enter a valid contact email for the team account.",
            )
            .into());
        }
        if !existing_mode
            && domains
                .get("inviteCodeRequired")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            && request.invite_code.as_deref().is_none_or(str::is_empty)
        {
            return Err(ValidationError::field(
                "invite_code",
                "invite_required",
                "This PDS requires an invitation code.",
            )
            .into());
        }
        // Captured before `handle` and `did` are consumed below: these are what let
        // a later read say *which* account a team was published under, and so
        // whether that account still exists.
        let handle_str = handle.as_str().to_owned();

        let agent_team = self
            .get_deployed_with_retry(address, id, "latest".into(), 10, Duration::from_millis(500))
            .await?
            .ok_or_else(|| anyhow::anyhow!("agents team not found"))?;

        let uri = agent_team
            .uri
            .ok_or_else(|| anyhow::anyhow!("agents team not deployed"))?;

        let http_client = ReqwestClient::new(request.pds_url.clone());
        let client = AtpServiceClient::new(http_client.clone());

        let session = CredentialSession::new(http_client, MemorySessionStore::default());
        let did = if existing_mode {
            let account = session
                .login(&request.handle, &request.password)
                .await
                .map_err(|_| {
                    ValidationError::field(
                        "password",
                        "account_signin_failed",
                        "Could not sign in. Check the handle and the account password.",
                    )
                })?;
            if account.data.handle != handle {
                return Err(ValidationError::field(
                    "handle",
                    "account_handle_mismatch",
                    "These credentials belong to another handle.",
                )
                .into());
            }
            account.data.did
        } else {
            let created = client
                .service
                .com
                .atproto
                .server
                .create_account(
                    server::create_account::InputData {
                        did: None,
                        email: Some(request.email.clone()),
                        handle: handle.clone(),
                        invite_code: request.invite_code,
                        password: Some(request.password.clone()),
                        plc_op: None,
                        recovery_key: None,
                        verification_code: None,
                        verification_phone: None,
                    }
                    .into(),
                )
                .await;
            match created {
                Ok(account) => {
                    session.login(&request.handle, &request.password).await
                        .map_err(|_| ValidationError::field("password", "account_signin_failed", "The account was created, but sign-in failed. Retry using this handle and password."))?;
                    account.data.did
                }
                Err(error) => {
                    // A retry after partial publication may find the account already
                    // created. Authenticate the exact requested handle before reuse.
                    let description = error.to_string().to_lowercase();
                    if description.contains("taken")
                        || description.contains("already")
                        || request.account_mode.is_none()
                    {
                        if let Ok(account) = session.login(&request.handle, &request.password).await
                        {
                            if account.data.handle == handle {
                                account.data.did
                            } else {
                                return Err(ValidationError::field(
                                    "handle",
                                    "account_handle_mismatch",
                                    "These credentials belong to another handle.",
                                )
                                .into());
                            }
                        } else {
                            return Err(publication_error(&description).into());
                        }
                    } else {
                        return Err(publication_error(&description).into());
                    }
                }
            }
        };

        let did_str = did.as_str().to_owned();
        let agent = Agent::new(session);
        let agent_repo = AtIdentifier::Did(did);

        if agent_team.description.is_some() || agent_team.logo.is_some() {
            let avatar = OptionFuture::from(
                agent_team
                    .logo
                    .as_ref()
                    .map(|logo| upload_blob_from_url(&agent, logo).map(Result::ok)),
            )
            .await
            .flatten();

            agent
                .api
                .com
                .atproto
                .repo
                .put_record(
                    repo::put_record::InputData {
                        collection: Profile::nsid(),
                        record: KnownRecord::AppBskyActorProfile(Box::new(
                            profile::RecordData {
                                avatar,
                                banner: None,
                                created_at: Some(Datetime::now()),
                                description: agent_team.description,
                                display_name: None,
                                joined_via_starter_pack: None,
                                labels: None,
                                pinned_post: None,
                                pronouns: None,
                                website: None,
                            }
                            .into(),
                        ))
                        .try_into_unknown()?,
                        repo: agent_repo.clone(),
                        rkey: SELF_RKEY.clone(),
                        swap_record: None,
                        swap_commit: None,
                        validate: None,
                    }
                    .into(),
                )
                .await?;
        }

        // `put` rather than `create`: the record is a singleton at rkey `self`, and
        // creating it fails outright when one is already there. That made publishing
        // twice impossible -- both after unpublishing and after a re-deploy, where
        // the point of publishing again is precisely to replace the uri this record
        // holds with the newer one.
        agent
            .api
            .com
            .atproto
            .repo
            .put_record(
                repo::put_record::InputData {
                    collection: AgentsTeamConfig::nsid(),
                    record: AgentsTeamConfig {
                        uri: uri.clone().into(),
                    }
                    .try_into_unknown()?,
                    repo: agent_repo,
                    rkey: SELF_RKEY.clone(),
                    swap_commit: None,
                    swap_record: None,
                    validate: None,
                }
                .into(),
            )
            .await?;

        let result = agent
            .api
            .com
            .atproto
            .server
            .create_app_password(
                server::create_app_password::InputData {
                    name: free_app_password_name(&agent).await?,
                    privileged: Some(false),
                }
                .into(),
            )
            .await?;

        let payload = models::FireskyCredentials {
            uri: uri.clone().into(),
            pds_url: request.pds_url.clone(),
            email: request.email.clone(),
            token: result.data.password.clone(),
            handle: Some(handle_str.clone()),
            did: Some(did_str.clone()),
        };
        let EncryptedMsg { ciphertext, nonce } =
            serialize_encrypted(payload, &self.aes_encryption_key)?;

        self.firesky_accounts.insert(
            uri,
            FireskyCredentials {
                pds_url: request.pds_url,
                email: request.email,
                token: result.data.password,
                handle: Some(handle_str),
                did: Some(did_str),
            },
        );

        let contract = SaveFireskyToken {
            env_uri: self.uri.clone(),
            nonce,
            ciphertext,
        }
        .render()?;

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(PublishToFireskyResp {
            contract: prepare_for_signing()
                .code(contract)
                .valid_after_block_number(valid_after)
                .call(),
        })
    }

    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(contract),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn deploy_signed_publish_to_firesky(
        &self,
        contract: SignedCode,
    ) -> anyhow::Result<DeployId> {
        record_trace!(contract);

        let mut write_client = self.write_client.clone();

        let deploy_id = write_client.deploy_signed_contract(contract).await?;
        Ok(deploy_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AgentsTeamConfig {
    uri: String,
}

impl Collection for AgentsTeamConfig {
    const NSID: &'static str = "com.f1r3sky.agentsteam.config";
    type Record = Self;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use dashmap::DashMap;
    use firefly_client::models::WalletAddress;
    use serde_json::json;

    use crate::domain::agents_teams::AgentsTeamsService;
    use crate::domain::agents_teams::models::PublishToFireskyReq;
    use crate::domain::agents_teams::testing::fake_pds::FakePds;
    use crate::domain::test_helpers::{
        MockNodeEventSource,
        MockReadNode,
        MockWriteNode,
        test_public_key,
        test_secret_key,
        test_uri,
    };

    /// The handle every test publishes under. The fake PDS is started to return
    /// this same handle, so the client's post-login handle check passes.
    const TEST_HANDLE: &str = "embers-team.test";

    #[test]
    fn publication_handle_errors_preserve_the_rejection_reason() {
        for (message, code, advice) in [
            ("Handle too long", "handle_too_long", "18 characters"),
            ("Handle too short", "handle_too_short", "3 characters"),
            ("Reserved handle", "handle_unavailable", "reserved"),
            (
                "Not a supported handle domain",
                "handle_domain_rejected",
                "suffix",
            ),
            (
                "Invalid characters in handle",
                "handle_rejected",
                "characters",
            ),
        ] {
            let upstream: atrium_api::com::atproto::server::create_account::Error =
                serde_json::from_value(json!({"error": "InvalidHandle", "message": message}))
                    .expect("PDS error response");
            let error = super::publication_error(&format!("xrpc response error: 400 {upstream}"));
            assert_eq!(error.code, code, "{message}");
            assert!(error.fields.contains_key("handle"));
            assert!(error.description.contains(advice), "{}", error.description);
            if message == "Handle too long" || message == "Handle too short" {
                assert!(!error.description.contains("Include one of the suffixes"));
            }
        }
    }

    /// Build a hermetic service backed by the mock nodes and a real temp-backed
    /// continuation store, with a read node that answers the deployed-team read.
    fn service_with(
        read: MockReadNode,
    ) -> AgentsTeamsService<MockReadNode, MockWriteNode, MockNodeEventSource> {
        AgentsTeamsService {
            uri: test_uri(),
            write_client: MockWriteNode::new(),
            read_client: read,
            observer_node_events: MockNodeEventSource::new(),
            service_key: test_secret_key(),
            aes_encryption_key: [42u8; 32].into(),
            continuation_store:
                crate::domain::agents_teams::continuation_store::F1r3driveContinuationStore::open_for_test(
                    tempfile::tempdir().expect("temporary store").path(),
                    [7u8; 32],
                    [9u8; 32],
                )
                .expect("test continuation store"),
            deploy_finalization_timeout: Duration::from_secs(5),
            run_locks: Arc::new(DashMap::new()),
            firesky_accounts: Arc::new(DashMap::new()),
        }
    }

    /// The blockchain-layer team projection a `get` read deserializes.
    fn team_json(uri: &str, logo: Option<&str>, description: Option<&str>) -> serde_json::Value {
        json!({
            "id": "team-id",
            "version": "team-version",
            "created_at": 0,
            "last_deploy": null,
            "uri": uri,
            "name": "Team",
            "description": description,
            "shard": null,
            "logo": logo,
            "graph": null,
            "definition": null,
        })
    }

    fn request(pds: &FakePds) -> PublishToFireskyReq {
        PublishToFireskyReq {
            account_mode: None,
            pds_url: pds.origin().to_owned(),
            email: "publisher@example.com".to_owned(),
            handle: TEST_HANDLE.to_owned(),
            password: "primary-password".to_owned(),
            invite_code: None,
        }
    }

    // A published team's URI: reuse the service's env URI so it is a valid `Uri`.
    fn deployed_uri() -> firefly_client::models::Uri {
        test_uri()
    }

    #[tokio::test]
    async fn publish_happy_path_succeeds_against_fake_pds() {
        let pds = FakePds::start(TEST_HANDLE).await.expect("start fake PDS");
        let uri = deployed_uri();
        let read = MockReadNode::new().on_code_containing(
            "team-id",
            team_json(
                AsRef::<String>::as_ref(&uri),
                Some(&pds.logo_url()),
                Some("A hermetic test team"),
            ),
        );
        let service = service_with(read);

        let response = service
            .prepare_publish_to_firesky_contract(
                WalletAddress::from(test_public_key()),
                "team-id".to_owned(),
                request(&pds),
            )
            .await
            .expect("publish should succeed against the fake PDS");

        // A signed-deployable contract was produced.
        assert!(!response.contract.0.is_empty());
        // The credential was recorded in memory for the deployed URI.
        assert!(service.firesky_accounts.contains_key(&uri));

        // The client made exactly the XRPC calls the flow prescribes.
        let calls = pds.calls();
        assert!(calls.contains(&"com.atproto.server.createSession".to_owned()));
        assert!(calls.contains(&"com.atproto.repo.uploadBlob".to_owned()));
        assert!(calls.contains(&"com.atproto.server.listAppPasswords".to_owned()));
        assert!(calls.contains(&"com.atproto.server.createAppPassword".to_owned()));
        // Two putRecord calls: the profile record, then the config record.
        assert_eq!(
            calls
                .iter()
                .filter(|nsid| nsid.as_str() == "com.atproto.repo.putRecord")
                .count(),
            2,
        );

        pds.shutdown().await;
    }

    #[tokio::test]
    async fn publish_fails_when_app_password_cap_reached() {
        let pds = FakePds::start(TEST_HANDLE).await.expect("start fake PDS");
        // Every Embers app-password name is already taken, so no free name exists.
        let mut taken = vec!["Embers".to_owned()];
        taken.extend((2..=100).map(|n| format!("Embers {n}")));
        pds.set_existing_app_passwords(taken);

        let uri = deployed_uri();
        let read = MockReadNode::new().on_code_containing(
            "team-id",
            team_json(
                AsRef::<String>::as_ref(&uri),
                None,
                Some("A hermetic test team"),
            ),
        );
        let service = service_with(read);

        let error = service
            .prepare_publish_to_firesky_contract(
                WalletAddress::from(test_public_key()),
                "team-id".to_owned(),
                request(&pds),
            )
            .await
            .expect_err("publish should fail once the app-password cap is reached");

        assert!(
            error.to_string().contains("app passwords"),
            "unexpected error: {error}"
        );
        // No credential recorded on the failure path.
        assert!(!service.firesky_accounts.contains_key(&uri));

        pds.shutdown().await;
    }

    #[tokio::test]
    async fn publish_fails_when_put_record_rejected() {
        let pds = FakePds::start(TEST_HANDLE).await.expect("start fake PDS");
        pds.set_reject_put_record(true);

        let uri = deployed_uri();
        let read = MockReadNode::new().on_code_containing(
            "team-id",
            team_json(
                AsRef::<String>::as_ref(&uri),
                None,
                Some("A hermetic test team"),
            ),
        );
        let service = service_with(read);

        let error = service
            .prepare_publish_to_firesky_contract(
                WalletAddress::from(test_public_key()),
                "team-id".to_owned(),
                request(&pds),
            )
            .await
            .expect_err("publish should surface a rejected putRecord");

        // atrium surfaces the PDS's 400 as an XRPC response error.
        assert!(
            error.to_string().contains("400") || error.to_string().contains("InvalidRecord"),
            "unexpected error: {error}"
        );
        assert!(!service.firesky_accounts.contains_key(&uri));

        // The putRecord was attempted, and creation was never reached.
        let calls = pds.calls();
        assert!(calls.contains(&"com.atproto.repo.putRecord".to_owned()));
        assert!(!calls.contains(&"com.atproto.server.createAppPassword".to_owned()));

        pds.shutdown().await;
    }
}

/// Public PDS metadata only. A bounded request keeps a mistyped host from
/// leaving the form indefinitely submitting.
pub async fn discover_pds(pds_url: &str) -> anyhow::Result<serde_json::Value> {
    use crate::domain::validation::ValidationError;
    let url = reqwest::Url::parse(pds_url)
        .ok()
        .filter(|url| {
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
        })
        .ok_or_else(|| {
            ValidationError::field(
                "pds_url",
                "invalid_pds_url",
                "Enter the PDS's HTTP or HTTPS address.",
            )
        })?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let response = client
        .get(format!(
            "{}/xrpc/com.atproto.server.describeServer",
            url.as_str().trim_end_matches('/')
        ))
        .send()
        .await
        .map_err(|_| {
            ValidationError::field(
                "pds_url",
                "pds_unreachable",
                "Could not reach this PDS. Check its address and that it is running.",
            )
        })?;
    let value: serde_json::Value = response
        .error_for_status()
        .map_err(|_| {
            ValidationError::field(
                "pds_url",
                "invalid_pds",
                "This address did not return PDS server information.",
            )
        })?
        .json()
        .await
        .map_err(|_| {
            ValidationError::field(
                "pds_url",
                "invalid_pds",
                "This address did not return valid PDS server information.",
            )
        })?;
    if !value
        .get("availableUserDomains")
        .is_some_and(serde_json::Value::is_array)
    {
        return Err(ValidationError::field(
            "pds_url",
            "invalid_pds",
            "The server did not advertise its handle domains.",
        )
        .into());
    }
    Ok(
        serde_json::json!({"availableUserDomains": value["availableUserDomains"], "inviteCodeRequired": value["inviteCodeRequired"]}),
    )
}

fn publication_error(message: &str) -> crate::domain::validation::ValidationError {
    use crate::domain::validation::ValidationError;
    let message = message.to_lowercase();
    let (field, code, text) = if message.contains("email") {
        (
            "email",
            "email_rejected",
            "This email is unavailable or unsupported. Use another email, or choose Use existing account.",
        )
    } else if message.contains("password") {
        (
            "password",
            "password_rejected",
            "The PDS rejected this password. Check its password requirements.",
        )
    } else if message.contains("invite") {
        (
            "invite_code",
            "invite_rejected",
            "The invitation code is missing, invalid, or already used.",
        )
    } else if message.contains("handle too long") {
        (
            "handle",
            "handle_too_long",
            "The username before the domain is too long. Shorten it; the demo PDS allows at most 18 characters before the suffix.",
        )
    } else if message.contains("handle too short") {
        (
            "handle",
            "handle_too_short",
            "The username before the domain is too short. Lengthen it; the demo PDS requires at least 3 characters before the suffix.",
        )
    } else if message.contains("reserved") && message.contains("handle") {
        (
            "handle",
            "handle_unavailable",
            "This handle is reserved by the PDS. Choose another username.",
        )
    } else if message.contains("taken")
        || message.contains("already")
        || message.contains("handlenotavailable")
    {
        (
            "handle",
            "handle_unavailable",
            "This handle is already registered. Choose Use existing account and enter its password, or choose another handle.",
        )
    } else if message.contains("domain") || message.contains("tld") {
        (
            "handle",
            "handle_domain_rejected",
            "The PDS rejected this handle. Include one of the suffixes advertised by this PDS, such as .test on the demo server.",
        )
    } else if message.contains("handle") {
        (
            "handle",
            "handle_rejected",
            "The PDS rejected this handle. Check the username's characters and the server's naming policy, or choose another username.",
        )
    } else {
        (
            "pds_url",
            "pds_publication_failed",
            "The PDS could not create the account. Check the server address and try again.",
        )
    };
    ValidationError::field(field, code, text)
}
