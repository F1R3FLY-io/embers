from time import sleep

import pytest
import requests
from cryptography.hazmat.primitives.asymmetric import ec

from tests.client import ApiClient, Wallet


@pytest.fixture
def prepopulated_wallet() -> Wallet:
    private_key_bytes = bytes.fromhex("0B4E12EC24D2F42F3FC826194750E3168A5F03071F382375C29A5E801DBBE8A5")
    return Wallet(private_key=ec.derive_private_key(int.from_bytes(private_key_bytes), ec.SECP256K1()))


@pytest.fixture
def wallet() -> Wallet:
    return Wallet(private_key=ec.generate_private_key(ec.SECP256K1()))


@pytest.fixture
def funded_wallet(client: ApiClient, prepopulated_wallet: Wallet, request: pytest.FixtureRequest) -> Wallet:
    wallet = Wallet(private_key=ec.generate_private_key(ec.SECP256K1()))

    resp = client.wallets.transfer(from_wallet=prepopulated_wallet, to_wallet=wallet, amount=request.param)
    assert resp.status == 200

    wait_for_read_node_sync()

    return wallet


@pytest.fixture
def client() -> ApiClient:
    return ApiClient("http://[::1]:8080/api")


def wait_for_read_node_sync():
    resp = requests.get("http://localhost:14403/api/blocks", timeout=10)
    head_hash = resp.json()[0]["blockHash"]

    while True:
        resp = requests.get(f"http://localhost:14413/api/block/{head_hash}", timeout=10)
        if resp.status_code == 200:
            break
        sleep(1)


def assert_match_transfer(transfer: dict, match: dict):
    assert transfer["direction"] == match["direction"]
    assert transfer["amount"] == match["amount"]
    assert transfer["to_address"] == match["to_address"]
