from __future__ import annotations

import base64
import json
import os
import threading
from dataclasses import dataclass
from functools import cached_property
from hashlib import blake2b
from typing import Any, Self

import base58
import requests
import websocket
from Crypto.Hash import keccak

from tests.key import SECP256k1
from tests.protobuf.casper import DeployDataProto

# Covers both the HTTP read and the wait for a deploy to be observed as finalised.
#
# Was 30s, which assumed a chain that finalises in seconds. Measured against the local
# network under load, a run of four publishing tests had its later fixtures give up at
# exactly 30.0s waiting for a transfer to finalise while the node was still working
# through the deploys of the earlier ones -- a passing suite turned into a failing one
# purely by the order it ran in. The wait costs nothing when the chain is quick, so the
# only thing a tight bound buys is false failures.
# Keep the caller's request budget above the server's default 120-second
# finalized-deploy budget so response transport and observer reads retain headroom.
DEFAULT_TIMEOUT = 180
DEFAULT_TEST_ACCOUNT_PASSWORD = os.environ.get("EMBERS_TEST_PDS_PASSWORD", "hunter22")

FIRECAP_ID = bytes([0, 0, 0])
FIRECAP_VERSION = bytes([0])


class ApiSync:
    def __init__(self):
        self._lock = threading.Lock()
        self._seen = set()
        self._waiting = {}

    def register(self, deploy_id: str) -> threading.Event:
        event = threading.Event()
        with self._lock:
            if deploy_id in self._seen:
                event.set()
                return event
            self._waiting[deploy_id] = event
            return event

    def notify(self, deploy_id: str):
        with self._lock:
            self._seen.add(deploy_id)
            event = self._waiting.get(deploy_id)
            if event is not None:
                event.set()


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


class UpdateResponce:
    def __init__(self, first: Responce, second: Responce, accepted: threading.Event):
        self.first = first
        self.second = second
        self._accepted = accepted

    def wait_for_sync(self) -> Self:
        assert self._accepted.wait(timeout=DEFAULT_TIMEOUT)
        return self


class HttpClient:
    def __init__(self, base_url: str):
        self.base_url = base_url
        self.listeners: dict[str, ApiSync] = {}

    def get(self, url: str, timeout: int = DEFAULT_TIMEOUT) -> Responce:
        url = f"http://{self.base_url}/api/{url}"
        r = requests.get(url, timeout=timeout)
        return Responce(r)

    def post(self, url: str, json: Any | None = None, timeout: int = DEFAULT_TIMEOUT) -> Responce:
        url = f"http://{self.base_url}/api/{url}"
        r = requests.post(url, json=json, timeout=timeout)
        return Responce(r)


class TestnetApi:
    def __init__(self, client: HttpClient):
        self._client = client

    def test_wallet(self) -> Responce:
        resp = self._client.post("/testnet/wallet")
        assert resp.status == 200, resp.body
        return resp

    def deploy(self, wallet: Wallet, test: str, env: str | None = None, timeout: int = DEFAULT_TIMEOUT) -> Responce:
        prepare_request = {"test": test, "env": env}
        resp = self._client.post("/testnet/deploy/prepare", json=prepare_request)
        assert resp.status == 200

        resp_next = self._client.post(
            "/testnet/deploy/send",
            timeout=timeout,
            json={
                "prepare_request": prepare_request,
                "prepare_response": resp.json["response"],
                "request": {
                    "test": sing_contract(wallet, resp.json["response"]["test_contract"]),
                    "env": sing_contract(wallet, resp.json["response"]["env_contract"])
                    if resp.json["response"].get("env_contract") is not None
                    else None,
                },
                "token": resp.json["token"],
            },
        )
        assert resp_next.status == 200, resp_next.body

        return resp_next


@dataclass
class Wallet:
    key: SECP256k1

    @cached_property
    def address(self) -> str:
        key_hash = keccak.new(digest_bits=256).update(self.key.public_key_bytes[1:]).digest()
        eth_hash = keccak.new(digest_bits=256).update(key_hash[-20:]).digest()

        checksum_hash = blake2b(FIRECAP_ID + FIRECAP_VERSION + eth_hash, digest_size=32).digest()
        checksum = checksum_hash[:4]

        return base58.b58encode(FIRECAP_ID + FIRECAP_VERSION + eth_hash + checksum).decode()


class WalletsApi:
    def __init__(self, client: HttpClient):
        self._client = client

    def get_wallet_state_and_history(self, address: str) -> Responce:
        return self._client.get(f"/wallets/{address}/state")

    def transfer(
        self,
        from_wallet: Wallet,
        to_wallet: Wallet,
        amount: int,
        description: str | None = None,
    ) -> UpdateResponce:
        prepare_request = {
            "from": from_wallet.address,
            "to": to_wallet.address,
            "amount": amount,
            "description": description,
        }
        resp = self._client.post("/wallets/transfer/prepare", json=prepare_request)
        assert resp.status == 200, resp.body

        resp_next = self._client.post(
            "/wallets/transfer/send",
            json={
                "prepare_request": prepare_request,
                "prepare_response": resp.json["response"],
                "request": sing_contract(from_wallet, resp.json["response"]["contract"]),
                "token": resp.json["token"],
            },
        )
        assert resp_next.status == 200, resp_next.body

        return UpdateResponce(
            first=resp,
            second=resp_next,
            accepted=self._client.listeners[from_wallet.address].register(resp_next.json["deploy_id"]),
        )

    def boost(
        self,
        from_wallet: Wallet,
        to_wallet: Wallet,
        amount: int,
        post_author_did: str,
        description: str | None = None,
        post_id: str | None = None,
    ) -> UpdateResponce:
        prepare_request = {
            "from": from_wallet.address,
            "to": to_wallet.address,
            "amount": amount,
            "description": description,
            "post_author_did": post_author_did,
            "post_id": post_id,
        }
        resp = self._client.post("/wallets/boost/prepare", json=prepare_request)
        assert resp.status == 200

        resp_next = self._client.post(
            "/wallets/boost/send",
            json={
                "prepare_request": prepare_request,
                "prepare_response": resp.json["response"],
                "request": sing_contract(from_wallet, resp.json["response"]["contract"]),
                "token": resp.json["token"],
            },
        )
        assert resp_next.status == 200

        return UpdateResponce(
            first=resp,
            second=resp_next,
            accepted=self._client.listeners[from_wallet.address].register(resp_next.json["deploy_id"]),
        )

    def listen_for_deploys(self, wallet: Wallet):
        api_sync = ApiSync()

        def on_message(_: Any, msg: str):
            event = json.loads(msg)
            if event.get("node_type") == "Observer":
                api_sync.notify(event["deploy_id"])

        ws = websocket.WebSocketApp(
            url=f"ws://{self._client.base_url}/api/wallets/{wallet.address}/deploys",
            on_message=on_message,
        )

        thread = threading.Thread(target=ws.run_forever, daemon=True)
        thread.start()

        self._client.listeners[wallet.address] = api_sync


@dataclass
class Agent:
    id: str
    version: str
    name: str
    description: str | None = None
    shard: str | None = None
    logo: str | None = None
    code: str | None = None


class AgentsApi:
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
        description: str | None = None,
        shard: str | None = None,
        logo: str | None = None,
        code: str | None = None,
        timeout: int = DEFAULT_TIMEOUT,
    ) -> UpdateResponce:
        prepare_request = {"name": name, "description": description, "shard": shard, "logo": logo, "code": code}
        resp = self._client.post("/ai-agents/create/prepare", json=prepare_request)
        assert resp.status == 200

        resp_next = self._client.post(
            "/ai-agents/create/send",
            timeout=timeout,
            json={
                "prepare_request": prepare_request,
                "prepare_response": resp.json["response"],
                "request": sing_contract(wallet, resp.json["response"]["contract"]),
                "token": resp.json["token"],
            },
        )
        assert resp_next.status == 200

        return UpdateResponce(
            first=resp,
            second=resp_next,
            accepted=self._client.listeners[wallet.address].register(resp_next.json["deploy_id"]),
        )

    def save(
        self,
        wallet: Wallet,
        agent_id: str,
        name: str,
        description: str | None = None,
        shard: str | None = None,
        logo: str | None = None,
        code: str | None = None,
        timeout: int = DEFAULT_TIMEOUT,
    ) -> UpdateResponce:
        prepare_request = {"name": name, "description": description, "shard": shard, "logo": logo, "code": code}
        resp = self._client.post(f"/ai-agents/{agent_id}/save/prepare", json=prepare_request)
        assert resp.status == 200

        resp_next = self._client.post(
            f"/ai-agents/{agent_id}/save/send",
            timeout=timeout,
            json={
                "prepare_request": prepare_request,
                "prepare_response": resp.json["response"],
                "request": sing_contract(wallet, resp.json["response"]["contract"]),
                "token": resp.json["token"],
            },
        )
        assert resp_next.status == 200

        return UpdateResponce(
            first=resp,
            second=resp_next,
            accepted=self._client.listeners[wallet.address].register(resp_next.json["deploy_id"]),
        )

    def delete(self, wallet: Wallet, agent_id: str) -> UpdateResponce:
        resp = self._client.post(f"/ai-agents/{agent_id}/delete/prepare")
        assert resp.status == 200

        resp_next = self._client.post(
            f"/ai-agents/{agent_id}/delete/send",
            json=sing_contract(wallet, resp.json["contract"]),
        )
        assert resp_next.status == 200

        return UpdateResponce(
            first=resp,
            second=resp_next,
            accepted=self._client.listeners[wallet.address].register(resp_next.json["deploy_id"]),
        )

    def deploy(self, wallet: Wallet, agent: Agent, timeout: int = DEFAULT_TIMEOUT) -> UpdateResponce:
        prepare_request = {
            "type": "Agent",
            "id": agent.id,
            "version": agent.version,
            "address": wallet.address,
        }
        resp = self._client.post("/ai-agents/deploy/prepare", json=prepare_request)
        assert resp.status == 200, resp.body

        resp_next = self._client.post(
            "/ai-agents/deploy/send",
            timeout=timeout,
            json={
                "prepare_request": prepare_request,
                "prepare_response": resp.json["response"],
                "request": {
                    "contract": sing_contract(wallet, resp.json["response"]["contract"]),
                    "system": sing_contract(wallet, resp.json["response"]["system"])
                    if resp.json["response"].get("system") is not None
                    else None,
                },
                "token": resp.json["token"],
            },
        )
        assert resp_next.status == 200, (
            f"{resp_next.body}\nprepared Rholang:\n{numbered_prepared_contract(resp.json['response']['contract'])}"
        )

        return UpdateResponce(
            first=resp,
            second=resp_next,
            accepted=self._client.listeners[wallet.address].register(resp_next.json["deploy_id"]),
        )

    def deploy_code(self, wallet: Wallet, code: str, timeout: int = DEFAULT_TIMEOUT) -> UpdateResponce:
        prepare_request = {"type": "Code", "code": code}
        resp = self._client.post("/ai-agents/deploy/prepare", json=prepare_request)
        assert resp.status == 200, resp.body

        resp_next = self._client.post(
            "/ai-agents/deploy/send",
            timeout=timeout,
            json={
                "prepare_request": prepare_request,
                "prepare_response": resp.json["response"],
                "request": {
                    "contract": sing_contract(wallet, resp.json["response"]["contract"]),
                    "system": sing_contract(wallet, resp.json["response"]["system"])
                    if resp.json["response"].get("system") is not None
                    else None,
                },
                "token": resp.json["token"],
            },
        )
        assert resp_next.status == 200, (
            f"{resp_next.body}\nprepared Rholang:\n{numbered_prepared_contract(resp.json['response']['contract'])}"
        )

        return UpdateResponce(
            first=resp,
            second=resp_next,
            accepted=self._client.listeners[wallet.address].register(resp_next.json["deploy_id"]),
        )


@dataclass
class AgentsTeam:
    id: str
    version: str
    name: str
    description: str | None = None
    shard: str | None = None
    logo: str | None = None
    graph: str | None = None
    definition: dict[str, Any] | None = None


class AgentsTeamsApi:
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
        description: str | None = None,
        shard: str | None = None,
        logo: str | None = None,
        graph: str | None = None,
        definition: dict[str, Any] | None = None,
        timeout: int = DEFAULT_TIMEOUT,
    ) -> UpdateResponce:
        prepare_request = {
            "name": name,
            "description": description,
            "shard": shard,
            "logo": logo,
            "graph": graph,
            "definition": definition,
        }
        resp = self._client.post("/ai-agents-teams/create/prepare", json=prepare_request)
        assert resp.status == 200, resp.body

        resp_next = self._client.post(
            "/ai-agents-teams/create/send",
            timeout=timeout,
            json={
                "prepare_request": prepare_request,
                "prepare_response": resp.json["response"],
                "request": sing_contract(wallet, resp.json["response"]["contract"]),
                "token": resp.json["token"],
            },
        )
        assert resp_next.status == 200, resp_next.body

        return UpdateResponce(
            first=resp,
            second=resp_next,
            accepted=self._client.listeners[wallet.address].register(resp_next.json["deploy_id"]),
        )

    def deploy(
        self, wallet: Wallet, agents_team: AgentsTeam, deploy: dict, timeout: int = DEFAULT_TIMEOUT
    ) -> UpdateResponce:
        prepare_request = {
            "type": "AgentsTeam",
            "id": agents_team.id,
            "version": agents_team.version,
            "address": wallet.address,
            "deploy": deploy,
        }
        resp = self._client.post("/ai-agents-teams/deploy/prepare", json=prepare_request)
        assert resp.status == 200, resp.body

        resp_next = self._client.post(
            "/ai-agents-teams/deploy/send",
            timeout=timeout,
            json={
                "prepare_request": prepare_request,
                "prepare_response": resp.json["response"],
                "request": {
                    "contract": sing_contract(wallet, resp.json["response"]["contract"]),
                    "system": sing_contract(wallet, resp.json["response"]["system"])
                    if resp.json["response"].get("system") is not None
                    else None,
                },
                "token": resp.json["token"],
            },
        )
        assert resp_next.status == 200, (
            f"{resp_next.body}\nprepared Rholang:\n{numbered_prepared_contract(resp.json['response']['contract'])}"
        )

        return UpdateResponce(
            first=resp,
            second=resp_next,
            accepted=self._client.listeners[wallet.address].register(resp_next.json["deploy_id"]),
        )

    def deploy_graph(self, wallet: Wallet, graph: str, deploy: dict, timeout: int = DEFAULT_TIMEOUT) -> UpdateResponce:
        prepare_request = {"type": "Graph", "graph": graph, "deploy": deploy}
        resp = self._client.post("/ai-agents-teams/deploy/prepare", json=prepare_request)
        assert resp.status == 200, resp.body

        resp_next = self._client.post(
            "/ai-agents-teams/deploy/send",
            timeout=timeout,
            json={
                "prepare_request": prepare_request,
                "prepare_response": resp.json["response"],
                "request": {
                    "contract": sing_contract(wallet, resp.json["response"]["contract"]),
                    "system": sing_contract(wallet, resp.json["response"]["system"])
                    if resp.json["response"].get("system") is not None
                    else None,
                },
                "token": resp.json["token"],
            },
        )
        assert resp_next.status == 200, resp_next.body

        return UpdateResponce(
            first=resp,
            second=resp_next,
            accepted=self._client.listeners[wallet.address].register(resp_next.json["deploy_id"]),
        )

    def run(self, wallet: Wallet, prompt: str, agents_team: str, timeout: int = DEFAULT_TIMEOUT) -> Responce:
        prepare_request = {"prompt": prompt, "agents_team": agents_team}
        resp = self._client.post("/ai-agents-teams/run/prepare", json=prepare_request)
        assert resp.status == 200, resp.body

        resp_next = self._client.post(
            "/ai-agents-teams/run/send",
            json={
                "prepare_request": prepare_request,
                "prepare_response": resp.json["response"],
                "request": sing_contract(wallet, resp.json["response"]["contract"]),
                "token": resp.json["token"],
            },
            timeout=timeout,
        )
        assert resp_next.status == 200, resp_next.body

        return resp_next

    def get_run(self, run_id: str) -> Responce:
        resp = self._client.get(f"/ai-agents-teams/runs/{run_id}")
        assert resp.status == 200, resp.body
        return resp

    def run_command(
        self,
        wallet: Wallet,
        run_id: str,
        *,
        expected_revision: int,
        command_type: str,
        wait_id: str | None = None,
        name: str | None = None,
        payload: Any | None = None,
        decision_id: str | None = None,
        expected_deadline: str | None = None,
        reason: str | None = None,
        from_definition_hash: str | None = None,
        to_definition_hash: str | None = None,
        target_team_uri: str | None = None,
        state_patch: list[dict[str, Any]] | None = None,
        expected_status: int = 200,
        timeout: int = DEFAULT_TIMEOUT,
    ) -> Responce:
        prepare_request = {
            "expected_revision": str(expected_revision),
            "type": command_type,
            "wait_id": wait_id,
            "name": name,
            "payload": payload,
            "decision_id": decision_id,
            "expected_deadline": expected_deadline,
            "reason": reason,
            "from_definition_hash": from_definition_hash,
            "to_definition_hash": to_definition_hash,
            "target_team_uri": target_team_uri,
            "state_patch": state_patch,
        }
        resp = self._client.post(
            f"/ai-agents-teams/runs/{run_id}/commands/prepare",
            json=prepare_request,
        )
        assert resp.status == 200, resp.body

        resp_next = self._client.post(
            f"/ai-agents-teams/runs/{run_id}/commands/send",
            json={
                "prepare_request": prepare_request,
                "prepare_response": resp.json["response"],
                "request": sing_contract(wallet, resp.json["response"]["contract"]),
                "token": resp.json["token"],
            },
            timeout=timeout,
        )
        assert resp_next.status == expected_status, resp_next.body
        return resp_next

    def reveal_output(
        self,
        wallet: Wallet,
        run_id: str,
        *,
        expected_prepare_status: int = 200,
        expected_status: int = 200,
        timeout: int = DEFAULT_TIMEOUT,
    ) -> Responce:
        # The prepare phase takes no body: the run id rides the path and is
        # bound into the JWT as the hashed prepare request.
        resp = self._client.post(f"/ai-agents-teams/runs/{run_id}/output/prepare")
        assert resp.status == expected_prepare_status, resp.body
        if expected_prepare_status != 200:
            return resp

        resp_next = self._client.post(
            f"/ai-agents-teams/runs/{run_id}/output/send",
            json={
                "prepare_request": run_id,
                "prepare_response": resp.json["response"],
                "request": sing_contract(wallet, resp.json["response"]["contract"]),
                "token": resp.json["token"],
            },
            timeout=timeout,
        )
        assert resp_next.status == expected_status, resp_next.body
        return resp_next

    def save(
        self,
        wallet: Wallet,
        agent_id: str,
        name: str,
        description: str | None = None,
        shard: str | None = None,
        logo: str | None = None,
        graph: str | None = None,
    ) -> UpdateResponce:
        prepare_request = {"name": name, "description": description, "shard": shard, "logo": logo, "graph": graph}
        resp = self._client.post(f"/ai-agents-teams/{agent_id}/save/prepare", json=prepare_request)
        assert resp.status == 200

        resp_next = self._client.post(
            f"/ai-agents-teams/{agent_id}/save/send",
            json={
                "prepare_request": prepare_request,
                "prepare_response": resp.json["response"],
                "request": sing_contract(wallet, resp.json["response"]["contract"]),
                "token": resp.json["token"],
            },
        )
        assert resp_next.status == 200

        return UpdateResponce(
            first=resp,
            second=resp_next,
            accepted=self._client.listeners[wallet.address].register(resp_next.json["deploy_id"]),
        )

    def delete(self, wallet: Wallet, agent_id: str) -> UpdateResponce:
        resp = self._client.post(f"/ai-agents-teams/{agent_id}/delete/prepare")
        assert resp.status == 200

        resp_next = self._client.post(
            f"/ai-agents-teams/{agent_id}/delete/send",
            json=sing_contract(wallet, resp.json["contract"]),
        )
        assert resp_next.status == 200

        return UpdateResponce(
            first=resp,
            second=resp_next,
            accepted=self._client.listeners[wallet.address].register(resp_next.json["deploy_id"]),
        )

    def publish_to_firesky(
        self,
        wallet: Wallet,
        agent_id: str,
        handle: str = "bar.test",
        email: str = "bar@bar.com",
        password: str = DEFAULT_TEST_ACCOUNT_PASSWORD,
        account_mode: str | None = None,
    ) -> UpdateResponce:
        # Parameterised so a test can pick a handle nobody else has taken: the PDS
        # rejects a duplicate outright, so a hardcoded one makes the test pass once
        # and fail on every later run against the same dev network.
        prepare_request = {
            "pds_url": "http://localhost:2583",
            "email": email,
            "handle": handle,
            "password": password,
        }
        if account_mode is not None:
            prepare_request["account_mode"] = account_mode
        resp = self._client.post(
            f"/ai-agents-teams/{wallet.address}/{agent_id}/publish-to-firesky/prepare",
            json=prepare_request,
        )
        assert resp.status == 200

        resp_next = self._client.post(
            f"/ai-agents-teams/{wallet.address}/{agent_id}/publish-to-firesky/send",
            json={
                "prepare_request": prepare_request,
                "prepare_response": resp.json["response"],
                "request": sing_contract(wallet, resp.json["response"]["contract"]),
                "token": resp.json["token"],
            },
        )
        assert resp_next.status == 200

        return UpdateResponce(
            first=resp,
            second=resp_next,
            accepted=self._client.listeners[wallet.address].register(resp_next.json["deploy_id"]),
        )

    # The reads behind these three are the slowest in the API and the default 30s is
    # not enough for them. One `listVersions` for a seven-agent team measured 23.5s,
    # 30.5s and 33.6s on three consecutive runs against a local network of 13.7k
    # blocks, and `referencing` issues one per team plus one publication lookup each
    # -- concurrently, but still bounded by the slowest. Timing out here would report
    # a working endpoint as broken.
    def referencing_teams(self, address: str, agent_id: str) -> Responce:
        return self._client.get(f"/ai-agents-teams/{address}/referencing/{agent_id}", timeout=300)

    def unpublish(self, address: str, agent_id: str) -> Responce:
        return self._client.post(f"/ai-agents-teams/{address}/{agent_id}/unpublish", timeout=300)

    def published_state(self, address: str, agent_id: str) -> Responce:
        return self._client.get(f"/ai-agents-teams/{address}/{agent_id}/published", timeout=300)

    def run_on_firesky(
        self,
        wallet: Wallet,
        prompt: str,
        uri: str,
        reply_to: dict[str, Any] | None = None,
    ) -> UpdateResponce:
        prepare_request = {"prompt": prompt, "agents_team": uri}
        resp = self._client.post("/ai-agents-teams/run-on-firesky/prepare", json=prepare_request)
        assert resp.status == 200

        resp_next = self._client.post(
            "/ai-agents-teams/run-on-firesky/send",
            json={
                "prepare_request": prepare_request,
                "prepare_response": resp.json["response"],
                "request": {
                    "contract": sing_contract(wallet, resp.json["response"]["contract"]),
                    "reply_to": reply_to,
                },
                "token": resp.json["token"],
            },
        )
        assert resp_next.status == 200, resp_next.body

        accepted = threading.Event()
        accepted.set()

        return UpdateResponce(
            first=resp,
            second=resp_next,
            accepted=accepted,
        )


@dataclass
class Oslf:
    id: str
    version: str
    name: str
    description: str | None = None
    query: str | None = None


class OslfsApi:
    def __init__(self, client: HttpClient):
        self._client = client

    def list(self, address: str) -> Responce:
        return self._client.get(f"/oslfs/{address}")

    def list_versions(self, address: str, oslf_id: str) -> Responce:
        return self._client.get(f"/oslfs/{address}/{oslf_id}/versions")

    def get(self, address: str, oslf_id: str, oslf_version: str) -> Responce:
        return self._client.get(f"/oslfs/{address}/{oslf_id}/versions/{oslf_version}")

    def create(
        self,
        wallet: Wallet,
        name: str,
        description: str | None = None,
        query: str | None = None,
    ) -> UpdateResponce:
        prepare_request = {"name": name, "description": description, "query": query}
        resp = self._client.post("/oslfs/create/prepare", json=prepare_request)
        assert resp.status == 200

        resp_next = self._client.post(
            "/oslfs/create/send",
            json={
                "prepare_request": prepare_request,
                "prepare_response": resp.json["response"],
                "request": sing_contract(wallet, resp.json["response"]["contract"]),
                "token": resp.json["token"],
            },
        )
        assert resp_next.status == 200

        return UpdateResponce(
            first=resp,
            second=resp_next,
            accepted=self._client.listeners[wallet.address].register(resp_next.json["deploy_id"]),
        )

    def save(
        self,
        wallet: Wallet,
        oslf_id: str,
        name: str,
        description: str | None = None,
        query: str | None = None,
    ) -> UpdateResponce:
        prepare_request = {"name": name, "description": description, "query": query}
        resp = self._client.post(f"/oslfs/{oslf_id}/save/prepare", json=prepare_request)
        assert resp.status == 200

        resp_next = self._client.post(
            f"/oslfs/{oslf_id}/save/send",
            json={
                "prepare_request": prepare_request,
                "prepare_response": resp.json["response"],
                "request": sing_contract(wallet, resp.json["response"]["contract"]),
                "token": resp.json["token"],
            },
        )
        assert resp_next.status == 200

        return UpdateResponce(
            first=resp,
            second=resp_next,
            accepted=self._client.listeners[wallet.address].register(resp_next.json["deploy_id"]),
        )

    def delete(self, wallet: Wallet, oslf_id: str) -> UpdateResponce:
        resp = self._client.post(f"/oslfs/{oslf_id}/delete/prepare")
        assert resp.status == 200

        resp_next = self._client.post(
            f"/oslfs/{oslf_id}/delete/send",
            json=sing_contract(wallet, resp.json["contract"]),
        )
        assert resp_next.status == 200

        return UpdateResponce(
            first=resp,
            second=resp_next,
            accepted=self._client.listeners[wallet.address].register(resp_next.json["deploy_id"]),
        )


class ApiClient:
    def __init__(self, backend_url: str):
        self._http_client = HttpClient(backend_url)
        self.testnet = TestnetApi(self._http_client)
        self.wallets = WalletsApi(self._http_client)
        self.agents = AgentsApi(self._http_client)
        self.agents_teams = AgentsTeamsApi(self._http_client)
        self.oslfs = OslfsApi(self._http_client)


def sing_contract(wallet: Wallet, contract: Any) -> dict:
    signature = wallet.key.sign(base64.b64decode(contract, validate=True))

    return {
        "contract": contract,
        "sig_algorithm": "secp256k1",
        "sig": base64.b64encode(signature).decode(),
        "deployer": base64.b64encode(wallet.key.public_key_bytes).decode(),
    }


def numbered_prepared_contract(contract: str) -> str:
    """Decode a prepared deploy and number its Rholang for parser diagnostics."""
    deploy = DeployDataProto.FromString(base64.b64decode(contract, validate=True))
    return "\n".join(f"{line_number:04d}: {line}" for line_number, line in enumerate(deploy.term.splitlines(), start=1))
