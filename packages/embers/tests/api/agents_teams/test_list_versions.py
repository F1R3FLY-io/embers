import pytest

from tests.client import AgentsTeam, ApiClient
from tests.conftest import Wallet, assert_match_agents_team_header


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_list_versions(client: ApiClient, funded_wallet: Wallet, agents_team: AgentsTeam):
    resp = client.agents_teams.list_versions(funded_wallet.address, agents_team.id)

    assert resp.status == 200
    assert len(resp.json["agents_teams"]) == 1
    assert_match_agents_team_header(resp.json["agents_teams"][0], agents_team)


def test_fail_to_list_versions__unknown_id(client: ApiClient, wallet: Wallet):
    resp = client.agents_teams.list_versions(wallet.address, "foo")
    assert resp.status == 404
