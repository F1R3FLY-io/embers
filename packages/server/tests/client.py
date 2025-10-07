from __future__ import annotations

import base64
import json
from dataclasses import dataclass
from functools import cached_property
from hashlib import blake2b
from typing import Any

import base58
import requests
from Crypto.Hash import keccak
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric import ec, utils
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

FIRECAP_ID = bytes([0, 0, 0])
FIRECAP_VERSION = bytes([0])


@dataclass
class Responce:
    status: int
    body: str

    def __init__(self, r: requests.Response):
        self.status = r.status_code
        self.body = r.text

    @cached_property
    def json(self) -> Any:
        return json.loads(self.body)


class HttpClient:
    def __init__(self, base_url: str):
        self._base_url = base_url

    def get(self, url: str, timeout: int = 15) -> Responce:
        url = self._base_url + url
        r = requests.get(url, timeout=timeout)
        return Responce(r)

    def post(self, url: str, json: Any | None = None, timeout: int = 15) -> Responce:
        url = self._base_url + url
        r = requests.post(url, json=json, timeout=timeout)
        return Responce(r)


class TestnetApi:
    def __init__(self, client: HttpClient):
        self._client = client

    def test_wallet(self) -> Responce:
        return self._client.post("/testnet/wallet")

    def deploy(self, wallet: Wallet, test: str, env: str | None = None) -> Responce:
        resp = self._client.post("/testnet/deploy/prepare", json={"test": test, "env": env})
        assert resp.status == 200

        json = {
            "test": sing_contract(wallet, resp.json["test_contract"]),
            "env": sing_contract(wallet, resp.json["env_contract"])
            if resp.json.get("env_contract") is not None
            else None,
        }

        resp_next = self._client.post("/testnet/deploy/send", json=json)
        assert resp_next.status == 200

        return resp_next


@dataclass
class Wallet:
    private_key: ec.EllipticCurvePrivateKey

    @cached_property
    def public_key(self) -> ec.EllipticCurvePublicKey:
        return self.private_key.public_key()

    @cached_property
    def public_key_bytes(self) -> bytes:
        return self.public_key.public_bytes(encoding=Encoding.X962, format=PublicFormat.UncompressedPoint)

    @cached_property
    def address(self) -> str:
        key_hash = keccak.new(digest_bits=256).update(self.public_key_bytes[1:]).digest()
        eth_hash = keccak.new(digest_bits=256).update(key_hash[-20:]).digest()

        checksum_hash = blake2b(FIRECAP_ID + FIRECAP_VERSION + eth_hash, digest_size=32).digest()
        checksum = checksum_hash[:4]

        return base58.b58encode(FIRECAP_ID + FIRECAP_VERSION + eth_hash + checksum).decode()

    def sign(self, contract: bytes) -> bytes:
        prehashed = blake2b(contract, digest_size=32).digest()
        return self.private_key.sign(prehashed, ec.ECDSA(utils.Prehashed(hashes.BLAKE2s(digest_size=32))))


class WalletsApi:
    def __init__(self, client: HttpClient):
        self._client = client

    def get_wallet_state_and_history(self, address: str) -> Responce:
        return self._client.get(f"/wallets/{address}/state")

    def transfer(self, from_wallet: Wallet, to_wallet: Wallet, amount: int, description: str | None = None) -> Responce:
        resp = self._client.post(
            "/wallets/transfer/prepare",
            json={"from": from_wallet.address, "to": to_wallet.address, "amount": amount, "description": description},
        )
        assert resp.status == 200

        resp_next = self._client.post("/wallets/transfer/send", json=sing_contract(from_wallet, resp.json["contract"]))
        assert resp_next.status == 200

        return resp


@dataclass
class Agent:
    id: str
    version: str
    name: str
    shard: str | None = None
    logo: str | None = None
    code: str | None = None


class AiAgentsApi:
    def __init__(self, client: HttpClient):
        self._client = client

    def list(self, address: str) -> Responce:
        return self._client.get(f"/ai-agents/{address}")

    def list_versions(self, address: str, agent_id: str) -> Responce:
        return self._client.get(f"/ai-agents/{address}/{agent_id}/versions")

    def get(self, address: str, agent_id: str, agent_version: str) -> Responce:
        return self._client.get(f"/ai-agents/{address}/{agent_id}/versions/{agent_version}")

    def create(
        self,
        wallet: Wallet,
        name: str,
        shard: str | None = None,
        logo: str | None = None,
        code: str | None = None,
    ) -> Responce:
        resp = self._client.post(
            "/ai-agents/create/prepare",
            json={"name": name, "shard": shard, "logo": logo, "code": code},
        )
        assert resp.status == 200

        resp_next = self._client.post("/ai-agents/create/send", json=sing_contract(wallet, resp.json["contract"]))
        assert resp_next.status == 200

        return resp

    def save(
        self,
        wallet: Wallet,
        agent_id: str,
        name: str,
        shard: str | None = None,
        logo: str | None = None,
        code: str | None = None,
    ) -> Responce:
        resp = self._client.post(
            f"/ai-agents/{agent_id}/save/prepare",
            json={"name": name, "shard": shard, "logo": logo, "code": code},
        )
        assert resp.status == 200

        resp_next = self._client.post(
            f"/ai-agents/{agent_id}/save/send",
            json=sing_contract(wallet, resp.json["contract"]),
        )
        assert resp_next.status == 200

        return resp


@dataclass
class AgentsTeam:
    id: str
    version: str
    name: str
    shard: str | None = None
    logo: str | None = None
    graph: str | None = None


class AiAgentsTeamsApi:
    def __init__(self, client: HttpClient):
        self._client = client

    def list(self, address: str) -> Responce:
        return self._client.get(f"/ai-agents-teams/{address}")

    def list_versions(self, address: str, agent_id: str) -> Responce:
        return self._client.get(f"/ai-agents-teams/{address}/{agent_id}/versions")

    def get(self, address: str, agent_id: str, agent_version: str) -> Responce:
        return self._client.get(f"/ai-agents-teams/{address}/{agent_id}/versions/{agent_version}")

    def create(
        self,
        wallet: Wallet,
        name: str,
        shard: str | None = None,
        logo: str | None = None,
        graph: str | None = None,
    ) -> Responce:
        resp = self._client.post(
            "/ai-agents-teams/create/prepare",
            json={"name": name, "shard": shard, "logo": logo, "graph": graph},
        )
        assert resp.status == 200

        resp_next = self._client.post("/ai-agents-teams/create/send", json=sing_contract(wallet, resp.json["contract"]))
        assert resp_next.status == 200

        return resp

    def deploy(self, wallet: Wallet, graph: str, phlo_limit: int) -> Responce:
        resp = self._client.post(
            "/ai-agents-teams/deploy/prepare",
            json={"type": "Graph", "graph": graph, "phlo_limit": phlo_limit},
        )
        assert resp.status == 200

        resp_next = self._client.post("/ai-agents-teams/deploy/send", json=sing_contract(wallet, resp.json["contract"]))
        assert resp_next.status == 200

        return resp

    def save(
        self,
        wallet: Wallet,
        agent_id: str,
        name: str,
        shard: str | None = None,
        logo: str | None = None,
        graph: str | None = None,
    ) -> Responce:
        resp = self._client.post(
            f"/ai-agents-teams/{agent_id}/save/prepare",
            json={"name": name, "shard": shard, "logo": logo, "graph": graph},
        )
        assert resp.status == 200

        resp_next = self._client.post(
            f"/ai-agents-teams/{agent_id}/save/send",
            json=sing_contract(wallet, resp.json["contract"]),
        )
        assert resp_next.status == 200

        return resp


class ApiClient:
    def __init__(self, backend_url: str):
        self._http_client = HttpClient(backend_url)
        self.testnet = TestnetApi(self._http_client)
        self.wallets = WalletsApi(self._http_client)
        self.ai_agents = AiAgentsApi(self._http_client)
        self.ai_agents_teams = AiAgentsTeamsApi(self._http_client)


def sing_contract(wallet: Wallet, contract: Any) -> dict:
    signature = wallet.sign(base64.b64decode(contract, validate=True))

    return {
        "contract": contract,
        "sig_algorithm": "secp256k1",
        "sig": base64.b64encode(signature).decode(),
        "deployer": base64.b64encode(wallet.public_key_bytes).decode(),
    }
