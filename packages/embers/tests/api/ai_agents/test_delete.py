import pytest

from tests.client import Agent, ApiClient, Wallet
from tests.conftest import wait_for_read_node_sync


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_delete_agent(client: ApiClient, funded_wallet: Wallet, agent: Agent):
    resp = client.ai_agents.delete(funded_wallet, agent.id)
    assert resp.status == 200

    wait_for_read_node_sync()

    resp = client.ai_agents.list(funded_wallet.address)
    assert resp.status == 200
    assert resp.json["agents"] == []


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_delete_unknown_agent(client: ApiClient, funded_wallet: Wallet):
    resp = client.ai_agents.delete(funded_wallet, "foo")
    assert resp.status == 200
