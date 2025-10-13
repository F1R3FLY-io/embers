import pytest

from tests.client import AgentsTeam, ApiClient, Wallet
from tests.conftest import wait_for_read_node_sync


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_delete_agents_team(client: ApiClient, funded_wallet: Wallet, agents_team: AgentsTeam):
    resp = client.ai_agents_teams.delete(funded_wallet, agents_team.id)
    assert resp.status == 200

    wait_for_read_node_sync()

    resp = client.ai_agents_teams.list(funded_wallet.address)
    assert resp.status == 200
    assert resp.json["agents_teams"] == []


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_delete_unknown_agents_team(client: ApiClient, funded_wallet: Wallet):
    resp = client.ai_agents_teams.delete(funded_wallet, "foo")
    assert resp.status == 200
