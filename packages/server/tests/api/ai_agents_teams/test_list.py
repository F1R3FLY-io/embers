import pytest

from tests.client import AgentsTeam, ApiClient
from tests.conftest import Wallet, assert_match_agents_team


def test_list_agents_teams__no_teams(client: ApiClient, wallet: Wallet):
    resp = client.ai_agents_teams.list(wallet.address)

    assert resp.status == 200
    assert resp.json["agents_teams"] == []


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_list_agents_teams(client: ApiClient, funded_wallet: Wallet, agents_team: AgentsTeam):
    resp = client.ai_agents_teams.list(funded_wallet.address)

    assert resp.status == 200
    assert len(resp.json["agents_teams"]) == 1
    assert_match_agents_team(resp.json["agents_teams"][0], agents_team)
