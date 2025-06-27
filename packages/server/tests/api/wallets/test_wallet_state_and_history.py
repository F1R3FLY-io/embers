from tests.client import ApiClient
from tests.conftest import ADDRESS


def test_wallet_state_and_history(client: ApiClient):
    resp = client.wallets.get_wallet_state_and_history(ADDRESS)

    assert resp.status == 200
    assert resp.json["balance"] == "0"
    assert resp.json["requests"] == []
    assert resp.json["exchanges"] == []
    assert resp.json["boosts"] == []
    assert resp.json["transfers"] == []
