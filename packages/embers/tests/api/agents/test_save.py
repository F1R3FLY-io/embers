import pytest

from tests.client import Agent, ApiClient, Wallet


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_save(client: ApiClient, funded_wallet: Wallet, agent: Agent):
    resp = client.agents.save(funded_wallet, agent.id, name="new_agent_name")
    assert resp.first.json["response"]["version"]
