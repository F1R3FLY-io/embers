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


@pytest.mark.slow
@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
@pytest.mark.parametrize("case", FIXTURES["cases"], ids=lambda case: case["name"])
def test_red_blue_decision_brief(client: ApiClient, funded_wallet: Wallet, case: dict):
    agents = {}
    for name, code in FIXTURES["agents"].items():
        record = client.agents.create(funded_wallet, name=name, code=code).wait_for_sync().first.json["response"]
        agents[name] = (record["id"], record["version"])
    definition = graph_module.build_definition(agents)
    record = client.agents_teams.create(
        funded_wallet, name="RedBlueDecisionReview", definition=definition
    ).wait_for_sync().first.json["response"]
    team = AgentsTeam(id=record["id"], version=record["version"], name="RedBlueDecisionReview", definition=definition)
    client.agents_teams.deploy(
        funded_wallet,
        agents_team=team,
        deploy=insert_signed_deploy(SECP256k1.generate(), datetime.now(UTC), funded_wallet, version=0),
    ).wait_for_sync()
    uri = client.agents_teams.get(funded_wallet.address, team.id, team.version).json["uri"]
    assert uri
    result = client.agents_teams.run(funded_wallet, case["brief"], agents_team=uri, timeout=600).json
    assert result == case["expected"]
