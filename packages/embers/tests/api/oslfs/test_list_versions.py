import pytest

from tests.client import ApiClient, Oslf
from tests.conftest import Wallet, assert_match_oslf_header


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_list_versions(client: ApiClient, funded_wallet: Wallet, oslf: Oslf):
    resp = client.oslfs.list_versions(funded_wallet.address, oslf.id)

    assert resp.status == 200
    assert len(resp.json["oslfs"]) == 1
    assert_match_oslf_header(resp.json["oslfs"][0], oslf)


def test_fail_to_list_versions__unknown_id(client: ApiClient, wallet: Wallet):
    resp = client.oslfs.list_versions(wallet.address, "foo")
    assert resp.status == 404
