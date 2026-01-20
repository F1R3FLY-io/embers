import pytest

from tests.client import Agent, ApiClient
from tests.conftest import Wallet, assert_match_agent_header


def test_list_empty(client: ApiClient, wallet: Wallet):
    resp = client.agents.list(wallet.address)

    assert resp.status == 200
    assert resp.json["agents"] == []


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_list(client: ApiClient, funded_wallet: Wallet, agent: Agent):
    resp = client.agents.list(funded_wallet.address)

    assert resp.status == 200
    assert len(resp.json["agents"]) == 1
    assert_match_agent_header(resp.json["agents"][0], agent)
