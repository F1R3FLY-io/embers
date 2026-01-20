import pytest

from tests.client import ApiClient, Oslf, Wallet


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_delete(client: ApiClient, funded_wallet: Wallet, oslf: Oslf):
    client.oslfs.delete(funded_wallet, oslf.id).wait_for_sync()

    resp = client.oslfs.list(funded_wallet.address)
    assert resp.status == 200
    assert resp.json["oslfs"] == []


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_delete_unknown_id(client: ApiClient, funded_wallet: Wallet):
    client.oslfs.delete(funded_wallet, "foo")
