import pytest

from tests.client import AgentsTeam, ApiClient, Wallet


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_save_agents_team(client: ApiClient, funded_wallet: Wallet, agents_team: AgentsTeam):
    resp = client.ai_agents_teams.save(funded_wallet, agents_team.id, name="new_agents_team_name")
    assert resp.first.json["response"]["version"]
