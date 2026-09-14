use std::time::Duration;

use anyhow::{Context, bail, ensure};
use chrono::{DateTime, Utc};
use firefly_client::models::{DeployData, Uri};
use firefly_client::rendering::Render;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};
use serde::{Deserialize, Serialize};

use super::AgentsTeamsService;
use super::continuation_store::ArtifactKind;
use super::models::{DeploySignedRunCommandReq, RunCommandReq, RunCommandResp, RunState};
use super::runtime::{ContinuationEnvelope, ResumeCommand};
use super::workflow::WorkflowDefinition;
use crate::domain::common::{prepare_for_signing, utc_now_protocol};

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/claim_command.rho")]
struct ClaimCommand {
    env_uri: Uri,
    run_id: String,
    expected_revision: i64,
    command_id: String,
    command_class: String,
    target_wait_id: Option<String>,
    command_content_hash: String,
    command_capability_hash: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/clear_command.rho")]
struct ClearCommand {
    env_uri: Uri,
    run_id: String,
    expected_revision: i64,
    command_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredRunCommand {
    pub schema_version: String,
    pub command_id: String,
    pub command_class: String,
    pub target_wait_id: Option<String>,
    pub expected_revision: u64,
    pub issued_at: DateTime<Utc>,
    pub command: ResumeCommand,
}

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    pub async fn prepare_run_command(
        &self,
        request: RunCommandReq,
    ) -> anyhow::Result<RunCommandResp> {
        ensure!(
            request.command.run_id() == request.run_id,
            "command run id mismatch"
        );
        let run = self
            .get_run(request.run_id.clone())
            .await?
            .context("run not found")?;
        validate_commandable_run(&run, request.expected_revision)?;

        let command_id = uuid::Uuid::now_v7().to_string();
        let issued_at = utc_now_protocol();
        let command_class = request.command.authorization_class().to_owned();
        let target_wait_id = request.command.target_wait_id().map(str::to_owned);
        let stored = StoredRunCommand {
            schema_version: "embers-agent-team-run-command-v1".to_owned(),
            command_id: command_id.clone(),
            command_class: command_class.clone(),
            target_wait_id: target_wait_id.clone(),
            expected_revision: request.expected_revision,
            issued_at,
            command: request.command,
        };
        let pointer = self.continuation_store.put(
            ArtifactKind::Command,
            &request.run_id,
            &command_id,
            &stored,
        )?;
        let expected_revision = i64::try_from(request.expected_revision)
            .context("run revision exceeds Rholang integer range")?;
        let code = ClaimCommand {
            env_uri: self.uri.clone(),
            run_id: request.run_id,
            expected_revision,
            command_id: command_id.clone(),
            command_class: command_class.clone(),
            target_wait_id: target_wait_id.clone(),
            command_content_hash: pointer.content_hash.clone(),
            command_capability_hash: pointer.capability_hash.clone(),
        }
        .render()?;
        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(RunCommandResp {
            command_id,
            command_class,
            target_wait_id,
            contract: prepare_for_signing()
                .code(code)
                .valid_after_block_number(valid_after)
                .call(),
            issued_at,
            command_content_hash: pointer.content_hash,
            command_capability_hash: pointer.capability_hash,
        })
    }

    pub async fn deploy_signed_run_command(
        &self,
        request: DeploySignedRunCommandReq,
    ) -> anyhow::Result<serde_json::Value> {
        ensure!(
            request.prepare_request.command.run_id() == request.prepare_request.run_id,
            "command run id mismatch"
        );
        let run_id = request.prepare_request.run_id.clone();
        let command_id = request.prepare_response.command_id.clone();
        let deploy_id = self
            .write_client
            .clone()
            .deploy_signed_contract(request.contract)
            .await?;
        match self
            .wait_for_deploy_finalization(&deploy_id, self.deploy_finalization_timeout)
            .await
        {
            Some(false) => {}
            Some(true) => bail!("command claim deploy {deploy_id} errored on chain"),
            None => bail!("command claim deploy {deploy_id} was not finalized"),
        }

        let lock = self
            .run_locks
            .entry(run_id.clone())
            .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;
        let run = self.wait_for_claimed_command(&run_id, &command_id).await?;
        let result = self
            .resume_claimed_command(
                &run,
                &request.prepare_response.command_id,
                &request.prepare_response.command_content_hash,
                &request.prepare_response.command_capability_hash,
            )
            .await;
        if result.is_err() {
            self.clear_claimed_command(&run_id, run.revision, &command_id)
                .await?;
        }
        result
    }

    async fn wait_for_claimed_command(
        &self,
        run_id: &str,
        command_id: &str,
    ) -> anyhow::Result<RunState> {
        for _ in 0..10 {
            if let Some(run) = self.get_run(run_id.to_owned()).await?
                && run.command_id.as_deref() == Some(command_id)
            {
                return Ok(run);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        bail!("claimed command was not visible from the observer node")
    }

    pub(crate) async fn resume_claimed_command(
        &self,
        run: &RunState,
        expected_command_id: &str,
        expected_content_hash: &str,
        expected_capability_hash: &str,
    ) -> anyhow::Result<serde_json::Value> {
        ensure!(
            run.status == "resuming",
            "run has no command awaiting execution"
        );
        let command_id = run
            .command_id
            .as_deref()
            .context("run command id is missing")?;
        ensure!(
            command_id == expected_command_id,
            "claimed command id mismatch"
        );
        let command_content_hash = run
            .command_content_hash
            .as_deref()
            .context("claimed command content hash is missing")?;
        let command_capability_hash = run
            .command_capability_hash
            .as_deref()
            .context("claimed command capability hash is missing")?;
        ensure!(
            command_content_hash == expected_content_hash
                && command_capability_hash == expected_capability_hash,
            "claimed command hashes differ from prepared command"
        );
        let command_pointer = self.locate_checked_artifact(
            ArtifactKind::Command,
            &run.run_id,
            command_id,
            command_content_hash,
            command_capability_hash,
        )?;
        let stored: StoredRunCommand =
            self.continuation_store
                .get(&command_pointer, &run.run_id, command_id)?;
        ensure!(
            stored.command_id == command_id,
            "stored command identity mismatch"
        );
        ensure!(
            stored.expected_revision == run.revision,
            "stored command revision mismatch"
        );
        ensure!(
            run.command_class.as_deref() == Some(stored.command_class.as_str()),
            "claimed command class differs from encrypted command"
        );
        ensure!(
            run.command_wait_id.as_deref() == stored.target_wait_id.as_deref(),
            "claimed command wait differs from encrypted command"
        );
        ensure!(
            stored.command.authorization_class() == stored.command_class,
            "stored command class is inconsistent"
        );
        ensure!(
            stored.command.target_wait_id() == stored.target_wait_id.as_deref(),
            "stored command wait identity is inconsistent"
        );

        let continuation_pointer = self.locate_checked_artifact(
            ArtifactKind::Continuation,
            &run.run_id,
            &format!("revision-{}", run.revision),
            run.continuation_content_hash
                .as_deref()
                .context("run continuation content hash is missing")?,
            run.continuation_capability_hash
                .as_deref()
                .context("run continuation capability hash is missing")?,
        )?;
        let mut continuation: ContinuationEnvelope = self.continuation_store.get(
            &continuation_pointer,
            &run.run_id,
            &format!("revision-{}", run.revision),
        )?;
        let actor = run
            .command_actor
            .as_deref()
            .context("command actor is missing")?;
        continuation.record_public_event("commandClaimed");
        continuation.apply_command(stored.command, actor, stored.issued_at)?;

        let team_uri = Uri::try_from(continuation.team_uri.clone())?;
        let manifest = self
            .get_team_manifest(team_uri.clone())
            .await?
            .context("command target team has no durable manifest")?;
        ensure!(
            manifest.definition_hash == continuation.definition_hash,
            "command target definition does not match continuation"
        );
        let definition = WorkflowDefinition::from_json(manifest.definition)?;
        definition.parsed().validate_publishable()?;
        self.drive_continuation(team_uri, definition, continuation, stored.issued_at)
            .await
    }

    async fn clear_claimed_command(
        &self,
        run_id: &str,
        expected_revision: u64,
        command_id: &str,
    ) -> anyhow::Result<()> {
        let code = ClearCommand {
            env_uri: self.uri.clone(),
            run_id: run_id.to_owned(),
            expected_revision: i64::try_from(expected_revision)?,
            command_id: command_id.to_owned(),
        }
        .render()?;
        let deploy_id = self
            .write_client
            .clone()
            .deploy(&self.service_key, DeployData::builder(code).build())
            .await?;
        match self
            .wait_for_deploy_finalization(&deploy_id, self.deploy_finalization_timeout)
            .await
        {
            Some(false) => {
                self.wait_for_run_state(run_id, "cleared command", |run| {
                    run.revision == expected_revision
                        && run.status == "suspended"
                        && run.command_id.is_none()
                })
                .await?;
                Ok(())
            }
            Some(true) => bail!("command clear deploy {deploy_id} errored on chain"),
            None => bail!("command clear deploy {deploy_id} was not finalized"),
        }
    }
}

fn validate_commandable_run(run: &RunState, expected_revision: u64) -> anyhow::Result<()> {
    ensure!(
        run.definition_hash.is_some(),
        "legacy runs do not accept durable commands"
    );
    ensure!(run.revision == expected_revision, "run revision is stale");
    ensure!(
        matches!(run.status.as_str(), "running" | "suspended"),
        "run status {} does not accept commands",
        run.status
    );
    ensure!(
        run.command_id.is_none(),
        "run already has a claimed command"
    );
    Ok(())
}
