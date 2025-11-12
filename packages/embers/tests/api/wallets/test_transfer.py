from tests.client import ApiClient
from tests.conftest import Wallet, assert_match_transfer


def test_transfer(client: ApiClient, prepopulated_wallet: Wallet, wallet: Wallet):
    client.wallets.transfer(from_wallet=prepopulated_wallet, to_wallet=wallet, amount=10000).wait_for_sync()

    resp = client.wallets.get_wallet_state_and_history(prepopulated_wallet.address)
    assert_match_transfer(
        resp.json["transfers"][-1],
        {"from": prepopulated_wallet.address, "to": wallet.address, "amount": "10000"},
    )

    resp = client.wallets.get_wallet_state_and_history(wallet.address)
    assert resp.status == 200
    assert resp.json["balance"] == "10000"
    assert resp.json["requests"] == []
    assert resp.json["exchanges"] == []
    assert resp.json["boosts"] == []
    assert len(resp.json["transfers"]) == 1
    assert_match_transfer(
        resp.json["transfers"][0],
        {"from": prepopulated_wallet.address, "to": wallet.address, "amount": "10000"},
    )
