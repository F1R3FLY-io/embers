import pytest

from tests.client import ApiClient, Oslf
from tests.conftest import Wallet, assert_match_oslf_header


def test_list_empty(client: ApiClient, wallet: Wallet):
    resp = client.oslfs.list(wallet.address)

    assert resp.status == 200
    assert resp.json["oslfs"] == []


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_list(client: ApiClient, funded_wallet: Wallet, oslf: Oslf):
    resp = client.oslfs.list(funded_wallet.address)

    assert resp.status == 200
    assert len(resp.json["oslfs"]) == 1
    assert_match_oslf_header(resp.json["oslfs"][0], oslf)
