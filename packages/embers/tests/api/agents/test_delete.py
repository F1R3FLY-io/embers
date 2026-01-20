import pytest

from tests.client import Agent, ApiClient, Wallet


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_delete(client: ApiClient, funded_wallet: Wallet, agent: Agent):
    client.agents.delete(funded_wallet, agent.id).wait_for_sync()

    resp = client.agents.list(funded_wallet.address)
    assert resp.status == 200
    assert resp.json["agents"] == []


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_delete_unknown_id(client: ApiClient, funded_wallet: Wallet):
    client.agents.delete(funded_wallet, "foo")
