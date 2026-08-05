"""Acceptance for the owner-authenticated full-response reveal.

A completed durable run's inspector page is public metadata only; the final
output value rests as an encrypted artifact whose hashes the terminal
advancement committed on chain. ``POST /runs/{run_id}/output/prepare`` +
``/send`` let exactly the run's on-chain owner read that value back through
the platform's signed two-phase protocol: the ``claimOutputReveal`` clause
adjudicates ownership on chain (the ledger scrubs ``owner`` from every read),
appends an actor-less ``outputRevealed`` event, and the backend releases the
decrypted value only over the signer's response channel.

Policy: hermetic — the team under test is the zero-agent echo workflow, so no
model provider and no network beyond localhost is ever touched.
"""

import uuid
from datetime import UTC, datetime

import pytest

from tests.client import AgentsTeam, ApiClient, Wallet, sing_contract
from tests.conftest import insert_signed_deploy, public_key_to_uri
from tests.key import SECP256k1

RUN_TIMEOUT = 600

ECHO_WORKFLOW_V2 = {
    "schemaVersion": "2.0",
    "entryScopeId": "root",
    "interface": {
        "inputs": {"input": {"kind": "string"}},
        "outputs": {"output": {"kind": "string"}},
    },
    "nodes": {
        "input": {"id": "input", "label": "Input", "scopeId": "root", "kind": "input"},
        "output": {"id": "output", "label": "Output", "scopeId": "root", "kind": "output"},
    },
    "policies": {"defaultExecution": "durable", "visibility": "private"},
    "presentation": {"edges": {}, "nodes": {}},
    "routeGroups": {
        "echo": {"id": "echo", "mode": "allMatches", "noMatch": "fail"},
    },
    "scopes": {
        "root": {
            "id": "root",
            "label": "Root",
            "kind": "root",
            "initialNodeIds": ["input"],
            "entryActivities": [],
            "exitActivities": [],
            "history": "none",
        }
    },
    "transitions": {
        "echo": {
            "id": "echo",
            "sourceNodeId": "input",
            "targetNodeId": "output",
            "trigger": "completed",
            "priority": 0,
            "routeGroupId": "echo",
        }
    },
    "types": {},
}

# Input straight into an owner-quorum-1 approval: running it suspends, which
# is exactly the not-yet-revealable state the 409 leg needs.
GATED_WORKFLOW_V2 = {
    "schemaVersion": "2.0",
    "entryScopeId": "root",
    "interface": {
        "inputs": {"input": {"kind": "string"}},
        "outputs": {"output": {"kind": "string"}},
    },
    "nodes": {
        "input": {"id": "input", "label": "Input", "scopeId": "root", "kind": "input"},
        "gate": {
            "id": "gate",
            "label": "Gate",
            "scopeId": "root",
            "kind": "approval",
            "policyId": "owner",
            "approvers": [],
            "quorum": 1,
        },
        "output": {"id": "output", "label": "Output", "scopeId": "root", "kind": "output"},
    },
    "policies": {"defaultExecution": "durable", "visibility": "private"},
    "presentation": {"edges": {}, "nodes": {}},
    "routeGroups": {
        "start": {"id": "start", "mode": "allMatches", "noMatch": "fail"},
        "gate-completed": {"id": "gate-completed", "mode": "allMatches", "noMatch": "fail"},
    },
    "scopes": {
        "root": {
            "id": "root",
            "label": "Root",
            "kind": "root",
            "initialNodeIds": ["input"],
            "entryActivities": [],
            "exitActivities": [],
            "history": "none",
        }
    },
    "transitions": {
        "start": {
            "id": "start",
            "sourceNodeId": "input",
            "targetNodeId": "gate",
            "trigger": "completed",
            "priority": 0,
            "routeGroupId": "start",
        },
        "approved": {
            "id": "approved",
            "sourceNodeId": "gate",
            "targetNodeId": "output",
            "trigger": "completed",
            "priority": 0,
            "routeGroupId": "gate-completed",
        },
    },
    "types": {},
}


def _run(client: ApiClient, wallet: Wallet, prompt: str, uri: str) -> tuple[str, object]:
    """Two-phase run that also captures the run id.

    The client's ``run`` helper returns only the send response, but the run id
    is minted in the prepare response — and a completed run's send response
    carries the value alone, so the id must be captured here.
    """
    http = client.agents_teams._client
    prepare_request = {"prompt": prompt, "agents_team": uri}
    prepared = http.post("/ai-agents-teams/run/prepare", json=prepare_request)
    assert prepared.status == 200, prepared.body
    run_id = prepared.json["response"]["run_id"]
    sent = http.post(
        "/ai-agents-teams/run/send",
        json={
            "prepare_request": prepare_request,
            "prepare_response": prepared.json["response"],
            "request": sing_contract(wallet, prepared.json["response"]["contract"]),
            "token": prepared.json["token"],
        },
        timeout=RUN_TIMEOUT,
    )
    assert sent.status == 200, sent.body
    return run_id, sent.json


def _deploy_team(client: ApiClient, wallet: Wallet, name: str, definition: dict) -> str:
    created = client.agents_teams.create(
        wallet, name=name, definition=definition, timeout=RUN_TIMEOUT
    ).wait_for_sync()
    team = AgentsTeam(
        id=created.first.json["response"]["id"],
        version=created.first.json["response"]["version"],
        name=name,
        definition=definition,
    )
    deployment_key = SECP256k1.generate()
    client.agents_teams.deploy(
        wallet,
        agents_team=team,
        deploy=insert_signed_deploy(deployment_key, datetime.now(UTC), wallet, version=0),
        timeout=RUN_TIMEOUT,
    ).wait_for_sync()
    return public_key_to_uri(deployment_key.public_key)


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_owner_reveals_exact_value_and_the_ledger_records_the_occurrence(
    client: ApiClient, funded_wallet: Wallet, test_wallet: Wallet
):
    uri = _deploy_team(client, funded_wallet, "reveal-output-echo", ECHO_WORKFLOW_V2)
    secret = f"reveal-me-{uuid.uuid4().hex[:8]}"
    run_id, completed = _run(client, funded_wallet, secret, uri)
    assert completed == secret

    # The public record carries hashes only — never the value.
    public_run = client.agents_teams.get_run(run_id).json
    assert public_run["output_content_hash"]
    assert secret not in public_run["output_content_hash"]

    # A wallet that is not the run's owner is refused with an honest 403:
    # run existence is already public through the bearer link, so absence-
    # hiding would only misreport an authorization failure.
    client.agents_teams.reveal_output(
        test_wallet, run_id, expected_status=403, timeout=RUN_TIMEOUT
    )

    revealed = client.agents_teams.reveal_output(
        funded_wallet, run_id, timeout=RUN_TIMEOUT
    ).json
    assert revealed == secret

    events = [e["event_type"] for e in client.agents_teams.get_run(run_id).json["events"]]
    assert "outputRevealed" in events
    # The stranger's aborted claim landed nothing on the record.
    assert events.count("outputRevealed") == 1


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_unrevealable_states_are_refused_before_any_signature(
    client: ApiClient, funded_wallet: Wallet
):
    # Unknown run: 404 at prepare.
    client.agents_teams.reveal_output(
        funded_wallet, str(uuid.uuid4()), expected_prepare_status=404
    )

    # Suspended run: the record exists but commits to no output artifact yet,
    # so prepare refuses with 409 and nothing is ever signed.
    uri = _deploy_team(client, funded_wallet, "reveal-output-gated", GATED_WORKFLOW_V2)
    suspended = client.agents_teams.run(
        funded_wallet, "hold this", agents_team=uri, timeout=RUN_TIMEOUT
    ).json
    assert suspended.get("disposition") == "suspended"
    client.agents_teams.reveal_output(
        funded_wallet, suspended["runId"], expected_prepare_status=409
    )


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_tampered_and_replayed_envelopes_cannot_reveal(
    client: ApiClient, funded_wallet: Wallet
):
    uri = _deploy_team(client, funded_wallet, "reveal-output-replay", ECHO_WORKFLOW_V2)
    secret = f"replay-target-{uuid.uuid4().hex[:8]}"
    run_id, completed = _run(client, funded_wallet, secret, uri)
    assert completed == secret

    http = client.agents_teams._client
    prepared = http.post(f"/ai-agents-teams/runs/{run_id}/output/prepare")
    assert prepared.status == 200, prepared.body

    # Tampering with the echoed prepare response breaks the JWT binding: the
    # extractor rejects the envelope before any deploy.
    tampered_response = dict(prepared.json["response"])
    tampered_response["output_content_hash"] = "0" * len(
        tampered_response["output_content_hash"]
    )
    tampered = http.post(
        f"/ai-agents-teams/runs/{run_id}/output/send",
        json={
            "prepare_request": run_id,
            "prepare_response": tampered_response,
            "request": sing_contract(funded_wallet, prepared.json["response"]["contract"]),
            "token": prepared.json["token"],
        },
        timeout=RUN_TIMEOUT,
    )
    assert tampered.status == 400, tampered.body

    # The untampered envelope reveals once; the identical envelope replayed is
    # a duplicate deploy the chain rejects, so a captured send cannot re-land.
    envelope = {
        "prepare_request": run_id,
        "prepare_response": prepared.json["response"],
        "request": sing_contract(funded_wallet, prepared.json["response"]["contract"]),
        "token": prepared.json["token"],
    }
    first = http.post(
        f"/ai-agents-teams/runs/{run_id}/output/send", json=envelope, timeout=RUN_TIMEOUT
    )
    assert first.status == 200, first.body
    assert first.json == secret
    replayed = http.post(
        f"/ai-agents-teams/runs/{run_id}/output/send", json=envelope, timeout=RUN_TIMEOUT
    )
    assert replayed.status != 200, replayed.body
