"""Local PDS publication/query and opt-in restart between review rounds."""

import os
import subprocess

import pytest
import requests

from tests.api.agents_teams.test_firesky_integration import create_mention_post, replies_by
from tests.api.agents_teams.test_live_trace import unlock
from tests.api.agents_teams.test_red_blue_demo import FIXTURES, deploy_demo
from tests.api.agents_teams.test_reveal_output import _run


def trace_entries(client, wallet, run_id):
    response, _ = unlock(client, wallet, run_id)
    result = requests.get(
        f"http://{client.agents_teams._client.base_url}/api/ai-agents-teams/runs/{run_id}/trace",
        headers={"Authorization": f"Bearer {response.json['accessToken']}"},
        timeout=60,
    )
    assert result.ok, result.text
    return result.json()["entries"]


@pytest.mark.pds
@pytest.mark.slow
@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_published_red_blue_team_accepts_ordinary_query(client, funded_wallet):
    team, uri = deploy_demo(client, funded_wallet, FIXTURES["agents"])
    handle = f"rb-{funded_wallet.address[-10:].lower()}.test"
    client.agents_teams.publish_to_firesky(
        funded_wallet, team.id, handle=handle, email=f"{handle}@example.com"
    ).wait_for_sync()
    publication = client.agents_teams.published_state(funded_wallet.address, team.id).json
    case = FIXTURES["cases"][0]
    post = create_mention_post(handle, publication["did"], case["brief"])
    ref = {"cid": post["cid"], "uri": post["uri"]}
    result = client.agents_teams.run_on_firesky(
        funded_wallet, prompt=case["brief"], uri=uri, reply_to={"parent": ref, "root": ref}, timeout=900
    )
    run_id = result.first.json["response"]["run_id"]
    replies = [
        record["value"]
        for record in replies_by(publication["did"])
        if record["value"].get("reply", {}).get("parent", {}).get("uri") == post["uri"]
    ]
    assert len(replies) == 1
    assert f"/runs/{run_id}" in replies[0]["text"]
    assert len(replies[0]["text"]) <= 280
    writer = [
        entry for entry in trace_entries(client, funded_wallet, run_id) if entry.get("agentName") == "DecisionWriter"
    ]
    assert len(writer) == 1 and writer[0]["outputAvailable"]
    assert "Stop reason: converged: no blockers" in writer[0]["output"]


@pytest.mark.recovery
@pytest.mark.slow
@pytest.mark.skipif(
    not os.environ.get("EMBERS_SERVICE_UNIT"),
    reason="Set EMBERS_SERVICE_UNIT; this rehearsal restarts the local backend",
)
@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_nested_review_resumes_after_restart_without_losing_round_history(client, funded_wallet):
    def pause_once(definition):
        definition["nodes"]["Pause"] = {
            "id": "Pause",
            "kind": "wait",
            "label": "Rehearsal pause",
            "scopeId": "root",
            "waitFor": "signal",
            "signal": "continue",
        }
        definition["routeGroups"]["ReviewCoverage"]["mode"] = "firstMatch"
        definition["transitions"]["edge-8"].update(
            targetNodeId="Pause",
            guard={
                "kind": "compare",
                "left": {"kind": "path", "root": "output", "path": ["round"]},
                "operator": "eq",
                "right": {"kind": "literal", "value": 1},
            },
        )
        definition["transitions"]["skip-pause"] = {
            "id": "skip-pause",
            "sourceNodeId": "ReviewCoverage",
            "targetNodeId": "Decide",
            "trigger": "completed",
            "priority": 1,
            "routeGroupId": "ReviewCoverage",
        }
        definition["routeGroups"]["resume"] = {"id": "resume", "mode": "allMatches", "noMatch": "fail"}
        definition["transitions"]["resume"] = {
            "id": "resume",
            "sourceNodeId": "Pause",
            "targetNodeId": "Decide",
            "trigger": "completed",
            "priority": 0,
            "routeGroupId": "resume",
        }

    _, uri = deploy_demo(client, funded_wallet, FIXTURES["agents"], nested=True, configure=pause_once)
    case = next(case for case in FIXTURES["cases"] if case["name"] == "revision")
    run_id, suspended = _run(client, funded_wallet, case["brief"], uri, timeout=900)
    assert suspended["disposition"] == "suspended"
    before = trace_entries(client, funded_wallet, run_id)
    first = next(entry for entry in before if entry.get("agentName") == "ReviewCoverage")
    assert first["output"]["round"] == 1
    subprocess.run(["systemctl", "--user", "restart", os.environ["EMBERS_SERVICE_UNIT"]], check=True)
    from tests.api.agents_teams.test_durable_recovery import _wait_embers_ready

    _wait_embers_ready()
    after = trace_entries(client, funded_wallet, run_id)
    assert [entry["id"] for entry in after] == [entry["id"] for entry in before]
    result = client.agents_teams.run_command(
        funded_wallet,
        run_id,
        expected_revision=int(suspended["revision"]),
        command_type="signal",
        wait_id=suspended["waits"][0]["id"],
        name="continue",
        payload=first["output"],
        timeout=900,
    ).json
    assert "Rounds: 2/3" in result and "Stop reason: converged: no blockers" in result
    reviewed = [
        entry for entry in trace_entries(client, funded_wallet, run_id) if entry.get("agentName") == "ReviewCoverage"
    ]
    assert len(reviewed) == 2
    assert len({entry["id"] for entry in reviewed}) == 2
    assert reviewed[-1]["output"]["history"][0]["round"] == 1
