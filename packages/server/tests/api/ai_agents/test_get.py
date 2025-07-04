import pytest

from tests.client import Agent, ApiClient, Wallet
from tests.conftest import assert_match_agent


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_get_agent(client: ApiClient, funded_wallet: Wallet, agent: Agent):
    resp = client.ai_agents.get(funded_wallet.address, agent.id, agent.version)

    assert resp.status == 200
    assert_match_agent(resp.json, agent)


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_fail_to_get_agent__unknown_agent(client: ApiClient, funded_wallet: Wallet, agent: Agent):
    resp = client.ai_agents.get(funded_wallet.address, "foo", agent.version)
    assert resp.status == 404


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_fail_to_get_agent__unknown_version(client: ApiClient, funded_wallet: Wallet, agent: Agent):
    resp = client.ai_agents.get(funded_wallet.address, agent.id, "foo")
    assert resp.status == 404
