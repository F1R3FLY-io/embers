import pytest

from tests.client import AgentsTeam, ApiClient, Wallet


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_delete(client: ApiClient, funded_wallet: Wallet, agents_team: AgentsTeam):
    client.agents_teams.delete(funded_wallet, agents_team.id).wait_for_sync()

    resp = client.agents_teams.list(funded_wallet.address)
    assert resp.status == 200
    assert resp.json["agents_teams"] == []


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_delete_unknown_id(client: ApiClient, funded_wallet: Wallet):
    client.agents_teams.delete(funded_wallet, "foo")
