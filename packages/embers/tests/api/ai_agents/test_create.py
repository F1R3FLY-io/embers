import pytest

from tests.client import ApiClient, Wallet


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_create_agent(client: ApiClient, funded_wallet: Wallet):
    resp = client.ai_agents.create(funded_wallet, name="my_agent", code='@Nil!("foo")')

    assert resp.status == 200
    assert resp.json["id"]
    assert resp.json["version"]
