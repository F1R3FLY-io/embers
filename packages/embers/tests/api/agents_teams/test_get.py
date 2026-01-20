import pytest

from tests.client import AgentsTeam, ApiClient, Wallet
from tests.conftest import assert_match_agents_team


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_get(client: ApiClient, funded_wallet: Wallet, agents_team: AgentsTeam):
    resp = client.agents_teams.get(funded_wallet.address, agents_team.id, agents_team.version)

    assert resp.status == 200
    assert_match_agents_team(resp.json, agents_team)


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_fail_to_get__unknown_id(client: ApiClient, funded_wallet: Wallet, agents_team: AgentsTeam):
    resp = client.agents_teams.get(funded_wallet.address, "foo", agents_team.version)
    assert resp.status == 404


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_fail_to_get__unknown_version(client: ApiClient, funded_wallet: Wallet, agents_team: AgentsTeam):
    resp = client.agents_teams.get(funded_wallet.address, agents_team.id, "foo")
    assert resp.status == 404
