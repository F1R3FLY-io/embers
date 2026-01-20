import pytest

from tests.client import ApiClient, Wallet


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_create(client: ApiClient, funded_wallet: Wallet):
    resp = client.agents.create(
        funded_wallet,
        name="my_agent",
        description="foo",
        shard="main",
        logo="http://nice-logo",
        code='@Nil!("foo")',
    )
    assert resp.first.json["response"]["id"]
    assert resp.first.json["response"]["version"]
