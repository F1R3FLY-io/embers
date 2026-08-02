use std::time::Duration;

use anyhow::{Context, ensure};
use chrono::Utc;
use firefly_client::helpers::sign_prepared_contract;
use firefly_client::models::Uri;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};

use super::AgentsTeamsService;
use super::continuation_store::ArtifactKind;
use super::models::{DeploySignedRunCommandReq, RunCommandReq, RunState};
use super::run_agents_team::DurableRunRequest;
use super::runtime::{ContinuationEnvelope, ResumeCommand};
use super::workflow::WorkflowDefinition;

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    pub async fn run_recovery_loop(self, poll_interval: Duration) {
        let mut interval = tokio::time::interval(poll_interval);
        loop {
            interval.tick().await;
            if let Err(error) = self.recover_runs_once().await {
                tracing::error!(error = %error, "Agent Teams recovery scan failed");
            }
        }
    }

    pub(crate) async fn recover_runs_once(&self) -> anyhow::Result<()> {
        let runs = self.list_runs().await?;
        for run in runs {
            if run.definition_hash.is_none() {
                continue;
            }
            let run_id = run.run_id.clone();
            if run.status == "suspended"
                && run.due_at.is_some_and(|deadline| deadline <= Utc::now())
            {
                if let Err(error) = self.fire_due_timer(&run).await {
                    tracing::error!(run_id, error = %error, "Agent Team timer recovery failed");
                }
                continue;
            }
            let lock = self
                .run_locks
                .entry(run_id.clone())
                .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
                .clone();
            let Ok(_guard) = lock.try_lock() else {
                continue;
            };
            let Some(current) = self.get_run(run_id.clone()).await? else {
                continue;
            };
            let result = match current.status.as_str() {
                "running" => self.resume_running_run(&current).await.map(|_| ()),
                "resuming" => self.resume_recoverable_command(&current).await.map(|_| ()),
                _ => Ok(()),
            };
            if let Err(error) = result {
                tracing::error!(run_id, status = %current.status, error = %error, "Agent Team run recovery failed");
            }
        }
        Ok(())
    }

    async fn resume_running_run(&self, run: &RunState) -> anyhow::Result<serde_json::Value> {
        let request_pointer = self.locate_checked_artifact(
            ArtifactKind::RunRequest,
            &run.run_id,
            "request",
            run.request_content_hash
                .as_deref()
                .context("run request content hash is missing")?,
            run.request_capability_hash
                .as_deref()
                .context("run request capability hash is missing")?,
        )?;
        let request: DurableRunRequest =
            self.continuation_store
                .get(&request_pointer, &run.run_id, "request")?;
        let continuation_object = format!("revision-{}", run.revision);
        let continuation_pointer = self.locate_checked_artifact(
            ArtifactKind::Continuation,
            &run.run_id,
            &continuation_object,
            run.continuation_content_hash
                .as_deref()
                .context("continuation content hash is missing")?,
            run.continuation_capability_hash
                .as_deref()
                .context("continuation capability hash is missing")?,
        )?;
        let continuation: ContinuationEnvelope = self.continuation_store.get(
            &continuation_pointer,
            &run.run_id,
            &continuation_object,
        )?;
        let team_uri = Uri::try_from(request.agents_team)?;
        let manifest = self
            .get_team_manifest(team_uri.clone())
            .await?
            .context("recoverable team manifest is unavailable")?;
        ensure!(
            Some(manifest.definition_hash.as_str()) == run.definition_hash.as_deref(),
            "recoverable run definition hash mismatch"
        );
        let definition = WorkflowDefinition::from_json(manifest.definition)?;
        definition.parsed().validate_publishable()?;
        let logical_time = if run.revision == 0 {
            request.created_at
        } else {
            Utc::now()
        };
        self.drive_continuation(team_uri, definition, continuation, logical_time)
            .await
    }

    async fn resume_recoverable_command(
        &self,
        run: &RunState,
    ) -> anyhow::Result<serde_json::Value> {
        self.resume_claimed_command(
            run,
            run.command_id.as_deref().context("command id is missing")?,
            run.command_content_hash
                .as_deref()
                .context("command content hash is missing")?,
            run.command_capability_hash
                .as_deref()
                .context("command capability hash is missing")?,
        )
        .await
    }

    async fn fire_due_timer(&self, run: &RunState) -> anyhow::Result<()> {
        let continuation_object = format!("revision-{}", run.revision);
        let pointer = self.locate_checked_artifact(
            ArtifactKind::Continuation,
            &run.run_id,
            &continuation_object,
            run.continuation_content_hash
                .as_deref()
                .context("continuation content hash is missing")?,
            run.continuation_capability_hash
                .as_deref()
                .context("continuation capability hash is missing")?,
        )?;
        let continuation: ContinuationEnvelope =
            self.continuation_store
                .get(&pointer, &run.run_id, &continuation_object)?;
        let now = Utc::now();
        let (wait_id, expected_deadline) = continuation
            .earliest_due_timer(now)
            .context("run due_at does not correspond to a pending timer")?;
        ensure!(
            run.due_at == Some(expected_deadline),
            "public run deadline differs from the encrypted continuation"
        );
        let command = RunCommandReq {
            run_id: run.run_id.clone(),
            expected_revision: run.revision,
            command: ResumeCommand::TimerFired {
                run_id: run.run_id.clone(),
                wait_id,
                expected_deadline,
            },
        };
        let prepared = self.prepare_run_command(command.clone()).await?;
        let signed = sign_prepared_contract(prepared.contract.0.clone(), &self.service_key)?;
        self.deploy_signed_run_command(DeploySignedRunCommandReq {
            contract: signed,
            prepare_request: command,
            prepare_response: prepared,
        })
        .await?;
        Ok(())
    }
}
