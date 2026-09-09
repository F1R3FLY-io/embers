"""Real-chain trace authorization and retained nested/cyclic agent outputs."""
import copy
from datetime import UTC, datetime

import pytest
import requests

from tests.api.agents_teams.test_reveal_output import ECHO_WORKFLOW_V2, GATED_WORKFLOW_V2, _run
from tests.client import AgentsTeam, sing_contract
from tests.conftest import insert_signed_deploy, public_key_to_uri
from tests.key import SECP256k1


def deploy(client, wallet, name, definition):
    record = client.agents_teams.create(wallet, name=name, definition=definition, timeout=600).wait_for_sync().first.json["response"]
    team = AgentsTeam(id=record["id"], version=record["version"], name=name, definition=definition)
    key = SECP256k1.generate()
    client.agents_teams.deploy(wallet, agents_team=team, deploy=insert_signed_deploy(key, datetime.now(UTC), wallet, version=0), timeout=600).wait_for_sync()
    return record, public_key_to_uri(key.public_key)


def unlock(client, wallet, run_id, expected_status=200):
    http = client.agents_teams._client
    prepared = http.post(f"/ai-agents-teams/runs/{run_id}/trace/prepare")
    assert prepared.status == 200, prepared.body
    body = {"prepare_request":run_id, "prepare_response":prepared.json["response"], "token":prepared.json["token"], "request":sing_contract(wallet, prepared.json["response"]["contract"])}
    response = http.post(f"/ai-agents-teams/runs/{run_id}/trace/send", json=body, timeout=150)
    assert response.status == expected_status, response.body
    return response, body


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_live_nested_cycle_outputs_and_authorization(client, funded_wallet, test_wallet):
    agent = client.agents.create(funded_wallet, name="Trace echo critic", code='contract main(@invocation, ret) = { ret!({"output": invocation.get("input"), "disposition": "completed"}) }').wait_for_sync().first.json["response"]
    child = copy.deepcopy(ECHO_WORKFLOW_V2)
    child["nodes"]["critic"] = {"id":"critic", "label":"Echo critic", "scopeId":"root", "kind":"activity", "activity":{"kind":"agent", "agentId":agent["id"], "agentVersion":agent["version"], "protocol":"structured-v1"}}
    child["transitions"]["echo"]["targetNodeId"] = "critic"
    child["routeGroups"]["return"] = {"id":"return", "mode":"allMatches", "noMatch":"fail"}
    child["transitions"]["return"] = {"id":"return", "sourceNodeId":"critic", "targetNodeId":"output", "trigger":"completed", "priority":0, "routeGroupId":"return"}
    _, child_uri = deploy(client, funded_wallet, "Trace nested team", child)
    child_prepared = client.agents_teams._client.post("/ai-agents-teams/run/prepare", json={"agents_team":child_uri,"prompt":"manifest commitment"})
    assert child_prepared.status == 200, child_prepared.body
    child_hash = child_prepared.json["response"]["definition_hash"]
    parent = copy.deepcopy(GATED_WORKFLOW_V2)
    parent["nodes"]["gate"]["approvers"] = [funded_wallet.address]
    parent["nodes"]["nested"] = {"id":"nested", "label":"Review team", "scopeId":"root", "kind":"activity", "activity":{"kind":"workflow", "teamId":child_uri, "version":child_hash}}
    parent["transitions"]["start"]["targetNodeId"] = "nested"
    parent["routeGroups"]["again"] = {"id":"again", "mode":"allMatches", "noMatch":"fail"}
    parent["routeGroups"]["limit"] = {"id":"limit", "mode":"allMatches", "noMatch":"fail"}
    parent["transitions"]["again"] = {"id":"again", "sourceNodeId":"nested", "targetNodeId":"nested", "trigger":"completed", "priority":0, "routeGroupId":"again", "loop":{"maxIterations":1}}
    parent["transitions"]["limit"] = {"id":"limit", "sourceNodeId":"nested", "targetNodeId":"gate", "trigger":"iterationLimit", "priority":0, "routeGroupId":"limit"}
    _, uri = deploy(client, funded_wallet, "Trace live cyclic review", parent)
    secret = "private trace input and output"
    run_id, result = _run(client, funded_wallet, secret, uri)
    assert result["disposition"] == "suspended", result
    before = client.agents_teams.get_run(run_id).json
    response, envelope = unlock(client, funded_wallet, run_id)
    url = f"http://{client.agents_teams._client.base_url}/api/ai-agents-teams/runs/{run_id}/trace"
    headers = {"Authorization":f"Bearer {response.json['accessToken']}"}
    trace = requests.get(url, headers=headers, timeout=60)
    assert trace.status_code == 200, trace.text
    entries = trace.json()["entries"]
    critics = [entry for entry in entries if entry["agentName"] == "Echo critic"]
    assert len(critics) == 2, entries
    assert len({entry["id"] for entry in critics}) == 2
    assert len({entry["executionId"] for entry in critics}) == 2
    assert all(entry["input"] == secret and entry["output"] == secret for entry in critics)
    assert all(entry["outputAvailable"] for entry in critics)
    assert client.agents_teams.get_run(run_id).json == before
    assert requests.get(url, timeout=15).status_code == 403
    replay = client.agents_teams._client.post(f"/ai-agents-teams/runs/{run_id}/trace/send", json=envelope)
    assert replay.status == 410
    unlock(client, test_wallet, run_id, expected_status=403)
    # End the test-owned wait rather than leaving work suspended in the demo.
    client.agents_teams.run_command(funded_wallet, run_id, command_type="cancel", expected_revision=int(before["revision"]), reason="Trace integration test completed", timeout=600)
