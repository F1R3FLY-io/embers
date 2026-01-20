import pytest

from tests.client import ApiClient, Oslf, Wallet
from tests.conftest import assert_match_oslf


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_get(client: ApiClient, funded_wallet: Wallet, oslf: Oslf):
    resp = client.oslfs.get(funded_wallet.address, oslf.id, oslf.version)
    assert resp.status == 200
    assert_match_oslf(resp.json, oslf)


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_fail_to_get__unknown_id(client: ApiClient, funded_wallet: Wallet, oslf: Oslf):
    resp = client.oslfs.get(funded_wallet.address, "foo", oslf.version)
    assert resp.status == 404


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_fail_to_get__unknown_version(client: ApiClient, funded_wallet: Wallet, oslf: Oslf):
    resp = client.oslfs.get(funded_wallet.address, oslf.id, "foo")
    assert resp.status == 404
