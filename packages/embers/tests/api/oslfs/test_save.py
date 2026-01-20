import pytest

from tests.client import ApiClient, Oslf, Wallet


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_save(client: ApiClient, funded_wallet: Wallet, oslf: Oslf):
    resp = client.oslfs.save(funded_wallet, oslf.id, name="new_oslf_name")
    assert resp.first.json["response"]["version"]
