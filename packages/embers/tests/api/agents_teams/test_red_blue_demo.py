"""Live-chain acceptance for every red/blue broadcast and join; no model calls."""

import importlib.util
import json
from datetime import UTC, datetime
from pathlib import Path

import pytest

from tests.client import AgentsTeam, ApiClient, Wallet
from tests.conftest import insert_signed_deploy
from tests.key import SECP256k1

DEMO = Path(__file__).resolve().parents[5].parent / "embers-frontend/demo/red-blue"
FIXTURES = json.loads((DEMO / "fixtures/deterministic.json").read_text())
spec = importlib.util.spec_from_file_location("red_blue_graph", DEMO / "graph.py")
assert spec is not None and spec.loader is not None
graph_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(graph_module)


def deploy_demo(client, funded_wallet, codes, nested=False, configure=None):
    agents = {}
    for name, code in codes.items():
        record = client.agents.create(funded_wallet, name=name, code=code).wait_for_sync().first.json["response"]
        agents[name] = (record["id"], record["version"])
    definition = graph_module.build_definition(agents)
    if nested:
        for container in definition["deploymentContainers"].values():
            container["parentId"] = "OuterDeployment"
        definition["deploymentContainers"]["OuterDeployment"] = {
            "id": "OuterDeployment",
            "label": "Engineering Review",
            "nodeIds": [],
            "target": "current",
        }
    if configure is not None:
        configure(definition)
    record = (
        client.agents_teams.create(funded_wallet, name="RedBlueDecisionReview", definition=definition)
        .wait_for_sync()
        .first.json["response"]
    )
    team = AgentsTeam(id=record["id"], version=record["version"], name="RedBlueDecisionReview", definition=definition)
    client.agents_teams.deploy(
        funded_wallet,
        agents_team=team,
        deploy=insert_signed_deploy(SECP256k1.generate(), datetime.now(UTC), funded_wallet, version=0),
    ).wait_for_sync()
    uri = client.agents_teams.get(funded_wallet.address, team.id, team.version).json["uri"]
    assert uri
    return team, uri


@pytest.mark.slow
@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
@pytest.mark.parametrize(
    "case",
    [*FIXTURES["cases"], {**FIXTURES["cases"][0], "name": "nested-deployment", "nested": True}],
    ids=lambda case: case["name"],
)
def test_red_blue_decision_brief(client: ApiClient, funded_wallet: Wallet, case: dict):
    team, uri = deploy_demo(client, funded_wallet, FIXTURES["agents"], nested=case.get("nested", False))
    from tests.api.agents_teams.test_reveal_output import _run
    from tests.api.agents_teams.test_live_trace import unlock
    import requests

    run_id, result = _run(client, funded_wallet, case["brief"], uri, timeout=900)
    assert isinstance(result, str)
    assert f"Rounds: {case['rounds']}/3" in result
    assert f"Stop reason: {case['stop']}" in result
    assert "Task constraint checks" in result
    assert "${" not in result
    if case["name"] == "revision":
        assert "Round 1/3: 0/1 findings addressed; 1 open blockers" in result
        assert "Round 2/3: 1/1 findings addressed; 0 open blockers" in result
        assert "actual=80000/1000 USD/month" in result
    if case["name"] == "accepted-risk":
        assert "owner=SRE lead; follow-up=Review after seven days" in result

    access, _ = unlock(client, funded_wallet, run_id)
    trace = requests.get(
        f"http://{client.agents_teams._client.base_url}/api/ai-agents-teams/runs/{run_id}/trace",
        headers={"Authorization": f"Bearer {access.json['accessToken']}"},
        timeout=60,
    )
    assert trace.ok, trace.text
    entries = trace.json()["entries"]
    for name in ("ConstraintCheck", "ReviewCoverage", "RedLogic", "RedEvidence", "RedOperations"):
        executions = [entry for entry in entries if entry.get("agentName") == name]
        assert len(executions) == case["rounds"], (name, executions)
        # executionId identifies the enclosing workflow ("root" here).
        # Each repeated invocation has its own effect key in id.
        assert len({entry["id"] for entry in executions}) == case["rounds"]
        assert all(entry["outputAvailable"] for entry in executions)
        assert sorted(entry["output"]["round"] for entry in executions) == list(range(1, case["rounds"] + 1))
    assert len([e for e in entries if e.get("agentName") == "BlueResponder"]) == case["rounds"] - 1
