# Rholang templates

Every Rholang program embers deploys is rendered from one of these templates by the
`#[derive(Render)]` machinery — embers never hand-concatenates Rholang. The pipeline
(Askama template + `IntoValue` field lowering + the escaping discipline) is described
in [Rholang templating](../../../docs/design/rholang-templating.md); the templates
are compiled into the binary at build time, so editing any file here requires a
rebuild, and every template is rendered with fixture data and parsed under the real
Rholang grammar by the [oracle](../../../docs/testing/test-architecture.md) (campaign
steps A5/A9), so a template that renders invalid Rholang fails a test rather than a
live deploy.

The 54 templates are grouped by the API surface they back. Each domain has an
`init.rho` that stands up the domain's on-chain registry (all sharing the
`common/insert_signed.rho` primitive), plus the per-operation contracts.

## `common/`

| Template | Renders |
| -------- | ------- |
| `insert_signed.rho` | The signed-registry insertion primitive: writes a value under an environment-key-derived URI on the signed registry. Shared by every domain's `init` and by the [environment registry](../../../docs/design/environment-registry.md). |

## `wallets/`

| Template | Renders |
| -------- | ------- |
| `init.rho` | Stand up the wallet domain's registry. |
| `transfer.rho` | A wallet-to-wallet transfer. |
| `boost.rho` | A boost (a transfer annotated with a post author/id and description). |
| `get_balance_and_history.rho` | Read a wallet's balance and transfer history. |

## `agents/`

Single-agent lifecycle and deploy.

| Template | Renders |
| -------- | ------- |
| `init.rho` | Stand up the agents registry. |
| `create.rho` / `save.rho` | Create / update an agent definition. |
| `get.rho` / `list.rho` | Read one / list agents. |
| `list_versions.rho` | List an agent's versions. |
| `delete.rho` | Remove an agent. |
| `deploy_agent.rho` | Deploy an agent's Rholang body to chain. |
| `record_deploy.rho` | Record a deploy against the agent record. |

## `agents_teams/`

The durable agent-team kernel — the largest group. The CRUD/versioning contracts
mirror `agents/`; the rest are the durable run machinery.

| Template | Renders |
| -------- | ------- |
| `init.rho` | Stand up the agent-teams registry. |
| `create.rho` / `save.rho` / `get.rho` / `list.rho` / `list_versions.rho` / `delete.rho` | Team-definition CRUD + versioning. |
| `deploy.rho` / `deploy_workflow.rho` | Deploy a team / its compiled workflow. |
| `record_deploy.rho` | Record a deploy against the team record. |
| `get_manifest.rho` | Read a team's manifest. |
| `begin_durable_run.rho` | Start a durable run (the kernel entry point). |
| `run.rho` | The compiled run choreography. |
| `advance_run.rho` | Advance a run past a committed step (revision-fenced). |
| `claim_command.rho` / `clear_command.rho` | Claim (CAS on revision) / clear a pending command — the one-claimed-command fence. |
| `claim_output_reveal.rho` | The owner-authenticated output-reveal clause (single-use; see [reveal_output](../src/domain/agents_teams/reveal_output.rs)). |
| `invoke_activity.rho` / `invoke_guard.rho` | Invoke an activity / evaluate a guard. |
| `get_run.rho` / `get_run_result.rho` / `list_runs.rho` | Read a run / its result / list runs. |
| `get_firesky_tokens.rho` / `save_firesky_token.rho` | Read / persist F1r3sky (ATProto) session tokens. |

### `agents_teams/nodes/`

The per-node-kind compiled bodies the workflow render pass splices into `run.rho`:

| Template | Node kind |
| -------- | --------- |
| `agent.rho` | An agent invocation node. |
| `compress.rho` | A context-compression node. |
| `output.rho` | The terminal output node (whose value the reveal protocol discloses). |
| `text_model.rho` | A text model (`rho:ai:gpt4`) node. |
| `tti_model.rho` | A text-to-image (`rho:ai:dalle3`) node. |
| `tts_model.rho` | A text-to-speech (`rho:ai:textToAudio`) node. |

## `oslfs/`

Operational-Semantics-in-Logical-Form object lifecycle (same CRUD shape as `agents/`).

| Template | Renders |
| -------- | ------- |
| `init.rho` | Stand up the OSLF registry. |
| `create.rho` / `save.rho` / `get.rho` / `list.rho` / `list_versions.rho` / `delete.rho` | OSLF object CRUD + versioning. |

## `testnet/`

| Template | Renders |
| -------- | ------- |
| `init.rho` | Stand up the testnet utilities. |
| `fund_test_wallet.rho` | Fund a test wallet from the faucet. |
| `get_logs.rho` | Read test-network logs. |
