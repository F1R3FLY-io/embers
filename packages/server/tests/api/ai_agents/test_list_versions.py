import pytest

from tests.client import Agent, ApiClient
from tests.conftest import Wallet, assert_match_agent


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_list_agent_versions(client: ApiClient, funded_wallet: Wallet, agent: Agent):
    resp = client.ai_agents.list_versions(funded_wallet.address, agent.id)

    assert resp.status == 200
    assert len(resp.json["agents"]) == 1
    assert_match_agent(resp.json["agents"][0], agent)


def test_fail_to_list_agent_versions__unknown_agent(client: ApiClient, wallet: Wallet):
    resp = client.ai_agents.list_versions(wallet.address, "foo")
    assert resp.status == 404
