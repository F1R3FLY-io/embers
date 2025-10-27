import pytest

from tests.client import Agent, ApiClient, Wallet


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_save_agent(client: ApiClient, funded_wallet: Wallet, agent: Agent):
    resp = client.ai_agents.save(funded_wallet, agent.id, name="new_agent_name")
    assert resp.first.json["version"]
