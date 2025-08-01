from time import sleep

import pytest
import requests
from cryptography.hazmat.primitives.asymmetric import ec

from tests.client import Agent, ApiClient, Wallet


@pytest.fixture
def client() -> ApiClient:
    return ApiClient("http://[::1]:8080/api")


def _wait_for_read_node_sync(write_node: str, read_node: str):
    resp = requests.get(f"http://{write_node}/api/blocks", timeout=15)
    head_hash = resp.json()[0]["blockHash"]

    while True:
        resp = requests.get(f"http://{read_node}/api/block/{head_hash}", timeout=15)
        if resp.status_code == 200:
            break
        sleep(0.2)


def wait_for_read_node_sync():
    _wait_for_read_node_sync("localhost:14403", "localhost:14413")


def wait_for_test_read_node_sync():
    _wait_for_read_node_sync("localhost:15403", "localhost:15413")


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
def test_wallet(client: ApiClient) -> Wallet:
    resp = client.ai_agents.test_wallet()
    assert resp.status == 200

    wait_for_test_read_node_sync()

    private_key_bytes = bytes.fromhex(resp.json["key"])
    return Wallet(private_key=ec.derive_private_key(int.from_bytes(private_key_bytes), ec.SECP256K1()))


def assert_match_transfer(transfer: dict, match: dict):
    assert transfer["direction"] == match["direction"]
    assert transfer["amount"] == match["amount"]
    assert transfer["to_address"] == match["to_address"]


@pytest.fixture
def agent(client: ApiClient, funded_wallet: Wallet) -> Agent:
    resp = client.ai_agents.create(funded_wallet, name="my_agent")
    assert resp.status == 200

    wait_for_read_node_sync()

    return Agent(id=resp.json["id"], version=resp.json["version"], name="my_agent")


def assert_match_agent(agent: dict, match: Agent):
    assert agent["id"] == match.id
    assert agent["version"] == match.version
    assert agent["name"] == match.name
    assert agent.get("shard") == match.shard
    assert agent.get("code") == match.code
