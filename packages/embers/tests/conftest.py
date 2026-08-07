import base64
import os
import shutil
import subprocess
from datetime import datetime
from hashlib import blake2b

import pytest
import requests
import zbase32
from crc import Calculator, Configuration
from ecdsa import VerifyingKey

from tests.client import Agent, AgentsTeam, ApiClient, Oslf, Wallet
from tests.key import SECP256k1
from tests.protobuf.rhoapi import ETuple, Expr, Par

# Where the Embers backend under test listens. The suite drives a *running*
# server (it is the L2 dev-net acceptance layer), so the target is configurable:
# locally it defaults to the IPv6 loopback the stack binds; L2 orchestration and
# L3 runs override it via the environment. All L1/L2 targets must stay on
# loopback (the campaign's hard no-egress constraint); the remote guard below
# enforces that L3 does not.
EMBERS_API_URL = os.environ.get("EMBERS_API_URL", "[::1]:8080")

_LOOPBACK_HOSTS = frozenset({"[::1]", "::1", "127.0.0.1", "localhost"})

# Markers whose availability is probed before a test runs. `slow` carries no
# probe; `remote` is handled by its own stronger guard in pytest_runtest_setup.
_SERVICE_MARKERS = ("devnet", "pds", "chroma", "gpt", "recovery")

# One probe result per service per session — services do not appear or vanish
# mid-run, and re-probing per test would add a network round trip to every case.
_PROBE_CACHE: dict[str, tuple[bool, str]] = {}


def pytest_addoption(parser: pytest.Parser) -> None:
    parser.addoption(
        "--strict-services",
        action="store_true",
        default=False,
        help="Fail (instead of skip) when a service a marked test needs is unavailable.",
    )


def _http_ok(url: str) -> tuple[bool, str]:
    try:
        response = requests.get(url, timeout=2)
    except requests.RequestException as exc:
        return False, f"{url}: {exc}"
    # Any non-5xx answer proves the service is listening and handling requests;
    # a readiness path may legitimately return 200, 404, or 401.
    if response.status_code >= 500:
        return False, f"{url}: HTTP {response.status_code}"
    return True, ""


def _host_of(api_url: str) -> str:
    # api_url is "host:port"; the host may be a bracketed IPv6 literal.
    if api_url.startswith("["):
        return api_url[: api_url.index("]") + 1]
    return api_url.rsplit(":", 1)[0]


def _probe_recovery() -> tuple[bool, str]:
    unit = os.environ.get("EMBERS_SERVICE_UNIT")
    if not unit:
        return False, "EMBERS_SERVICE_UNIT is not set"
    systemctl = shutil.which("systemctl")
    if systemctl is None:
        return False, "systemctl is not available"
    # Fixed argv (no shell), absolute path from shutil.which, and `unit` is
    # operator-provided dev configuration rather than untrusted input.
    completed = subprocess.run(  # noqa: S603
        [systemctl, "--user", "is-active", "--quiet", unit],
        check=False,
    )
    if completed.returncode == 0:
        return True, ""
    return False, f"systemd unit {unit} is not active"


def _run_probe(name: str) -> tuple[bool, str]:
    http_probes = {
        "devnet": f"http://{EMBERS_API_URL}/swagger-ui/index.html",
        "pds": "http://localhost:2583/xrpc/_health",
        "chroma": "http://localhost:8000/api/v2/heartbeat",
        "gpt": "http://127.0.0.1:8571/v1/models",
    }
    if name in http_probes:
        return _http_ok(http_probes[name])
    if name == "recovery":
        return _probe_recovery()
    return True, ""


def _probe(name: str) -> tuple[bool, str]:
    if name not in _PROBE_CACHE:
        _PROBE_CACHE[name] = _run_probe(name)
    return _PROBE_CACHE[name]


def pytest_collection_modifyitems(items: list[pytest.Item]) -> None:
    # Every acceptance test under tests/api/** drives the running backend, so it
    # uniformly needs the dev-net stack. Applying `devnet` here keeps that one
    # invariant in a single place; the sharper needs (pds/chroma/gpt/recovery/slow)
    # stay as explicit pytestmark in the modules that have them.
    for item in items:
        if "/api/" in item.nodeid or item.nodeid.startswith("api/"):
            item.add_marker("devnet")


def pytest_runtest_setup(item: pytest.Item) -> None:
    markers = {marker.name for marker in item.iter_markers()}

    if "remote" in markers:
        # L3: never run against loopback, and never without explicit opt-in.
        if os.environ.get("EMBERS_L3_CONFIRM") != "1":
            pytest.fail(
                "remote (L3) tests call real external services and must never run by "
                "default; set EMBERS_L3_CONFIRM=1 to acknowledge before running them",
            )
        if _host_of(EMBERS_API_URL) in _LOOPBACK_HOSTS:
            pytest.fail(
                f"remote (L3) tests must not target loopback (EMBERS_API_URL={EMBERS_API_URL}); "
                "point them at a real remote deployment",
            )
        # Remote services are assumed present; do not run the local-service probes.
        return

    strict = item.config.getoption("--strict-services")
    for marker in _SERVICE_MARKERS:
        if marker not in markers:
            continue
        available, reason = _probe(marker)
        if available:
            continue
        message = f"service '{marker}' unavailable: {reason}"
        if strict:
            pytest.fail(message)
        pytest.skip(message)


ECHO_TEAM = 'context "{}" for A8f132a6a0f9340b59053b2689b8e040b in let A8f132a6a0f9340b59053b2689b8e040b = context "{\\"type\\":\\"input-node\\"}" for a7e2c4e97cb7145aea7ae4af15ba6789f in let a7e2c4e97cb7145aea7ae4af15ba6789f = < a7e2c4e97cb7145aea7ae4af15ba6789f > in < a7e2c4e97cb7145aea7ae4af15ba6789f > | {context "{\\"type\\":\\"output-node\\"}" for a02b50756f24f45f793b84b3d33e965c7 in let a02b50756f24f45f793b84b3d33e965c7 = < a02b50756f24f45f793b84b3d33e965c7 > in < a02b50756f24f45f793b84b3d33e965c7 > | 0} in 0 * {(let a7e2c4e97cb7145aea7ae4af15ba6789f = < a7e2c4e97cb7145aea7ae4af15ba6789f > in 0, let a02b50756f24f45f793b84b3d33e965c7 = < a02b50756f24f45f793b84b3d33e965c7 > in 0) * 0} '  # noqa: E501
COMPRESS_TEAM = 'context "{}" for A0fe7f81a4a71433c9b708d8c8484f9b2 in let A0fe7f81a4a71433c9b708d8c8484f9b2 = context "{\\"type\\":\\"input-node\\"}" for a8f6d37c2166f4aa3936a2df6c65dd9af in let a8f6d37c2166f4aa3936a2df6c65dd9af = < a8f6d37c2166f4aa3936a2df6c65dd9af > in < a8f6d37c2166f4aa3936a2df6c65dd9af > | {context "{\\"type\\":\\"output-node\\"}" for a72f0d65ffc19427c823558d0224cc964 in let a72f0d65ffc19427c823558d0224cc964 = < a72f0d65ffc19427c823558d0224cc964 > in < a72f0d65ffc19427c823558d0224cc964 > | {context "{\\"type\\":\\"compress\\"}" for a5653f99668994404a9a6ae4bf6889b0e in let a5653f99668994404a9a6ae4bf6889b0e = < a5653f99668994404a9a6ae4bf6889b0e > in < a5653f99668994404a9a6ae4bf6889b0e > | 0}} in 0 * {(let a8f6d37c2166f4aa3936a2df6c65dd9af = < a8f6d37c2166f4aa3936a2df6c65dd9af > in 0, let a5653f99668994404a9a6ae4bf6889b0e = < a5653f99668994404a9a6ae4bf6889b0e > in 0) * {(let a5653f99668994404a9a6ae4bf6889b0e = < a5653f99668994404a9a6ae4bf6889b0e > in 0, let a72f0d65ffc19427c823558d0224cc964 = < a72f0d65ffc19427c823558d0224cc964 > in 0) * 0}} '  # noqa: E501
GPT_COMPRESS_TEAM = 'context "{}" for A005ef27270144757997243d5c7892749 in let A005ef27270144757997243d5c7892749 = context "{\\"type\\":\\"input-node\\"}" for a2210aecc94d849e38d623251b158a3dd in let a2210aecc94d849e38d623251b158a3dd = < a2210aecc94d849e38d623251b158a3dd > in < a2210aecc94d849e38d623251b158a3dd > | {context "{\\"type\\":\\"output-node\\"}" for a5822ec2e8030498296ecb192cffc759d in let a5822ec2e8030498296ecb192cffc759d = < a5822ec2e8030498296ecb192cffc759d > in < a5822ec2e8030498296ecb192cffc759d > | {context "{\\"type\\":\\"text-model\\"}" for ad0d3b3ff43b34655807f61eafade165b in let ad0d3b3ff43b34655807f61eafade165b = < ad0d3b3ff43b34655807f61eafade165b > in < ad0d3b3ff43b34655807f61eafade165b > | {context "{\\"type\\":\\"compress\\"}" for a820a9e638bf24771ae6317cf4a9b7d18 in let a820a9e638bf24771ae6317cf4a9b7d18 = < a820a9e638bf24771ae6317cf4a9b7d18 > in < a820a9e638bf24771ae6317cf4a9b7d18 > | 0}}} in 0 * {(let a2210aecc94d849e38d623251b158a3dd = < a2210aecc94d849e38d623251b158a3dd > in 0, let ad0d3b3ff43b34655807f61eafade165b = < ad0d3b3ff43b34655807f61eafade165b > in 0) * {(let ad0d3b3ff43b34655807f61eafade165b = < ad0d3b3ff43b34655807f61eafade165b > in 0, let a820a9e638bf24771ae6317cf4a9b7d18 = < a820a9e638bf24771ae6317cf4a9b7d18 > in 0) * {(let a820a9e638bf24771ae6317cf4a9b7d18 = < a820a9e638bf24771ae6317cf4a9b7d18 > in 0, let a5822ec2e8030498296ecb192cffc759d = < a5822ec2e8030498296ecb192cffc759d > in 0) * 0}}} '  # noqa: E501


@pytest.fixture
def client() -> ApiClient:
    return ApiClient(EMBERS_API_URL)


@pytest.fixture
def prepopulated_wallet(client: ApiClient) -> Wallet:
    wallet = Wallet(key=SECP256k1.from_hex("0B4E12EC24D2F42F3FC826194750E3168A5F03071F382375C29A5E801DBBE8A5"))
    client.wallets.listen_for_deploys(wallet)
    return wallet


@pytest.fixture
def wallet() -> Wallet:
    return Wallet(key=SECP256k1.generate())


@pytest.fixture
def funded_wallet(client: ApiClient, prepopulated_wallet: Wallet, request: pytest.FixtureRequest) -> Wallet:
    wallet = Wallet(key=SECP256k1.generate())
    client.wallets.listen_for_deploys(wallet)
    client.wallets.transfer(from_wallet=prepopulated_wallet, to_wallet=wallet, amount=request.param).wait_for_sync()
    return wallet


@pytest.fixture
def test_wallet(client: ApiClient) -> Wallet:
    resp = client.testnet.test_wallet()
    return Wallet(key=SECP256k1.from_hex(resp.json["key"]))


def assert_match_transfer(transfer: dict, match: dict):
    assert transfer["from"] == match["from"]
    assert transfer["to"] == match["to"]
    assert transfer["amount"] == match["amount"]
    assert transfer.get("description") == match.get("description")


@pytest.fixture
def agent(client: ApiClient, funded_wallet: Wallet, request: pytest.FixtureRequest) -> Agent:
    resp = client.agents.create(
        funded_wallet,
        name="my_agent",
        code='@Nil!("foo")' if not hasattr(request, "param") else request.param,
    ).wait_for_sync()

    return Agent(
        id=resp.first.json["response"]["id"],
        version=resp.first.json["response"]["version"],
        name="my_agent",
        code='@Nil!("foo")',
    )


def assert_match_agent_header(agent: dict, match: Agent):
    assert agent["id"] == match.id
    assert agent["version"] == match.version
    assert agent.get("created_at")
    assert agent["name"] == match.name
    assert agent.get("shard") == match.shard
    assert agent.get("logo") == match.logo


def assert_match_agent(agent: dict, match: Agent):
    assert agent["id"] == match.id
    assert agent["version"] == match.version
    assert agent.get("created_at")
    assert agent["name"] == match.name
    assert agent.get("shard") == match.shard
    assert agent.get("logo") == match.logo
    assert agent.get("code") == match.code


@pytest.fixture
def agents_team(client: ApiClient, funded_wallet: Wallet, request: pytest.FixtureRequest) -> AgentsTeam:
    resp = client.agents_teams.create(
        funded_wallet,
        name="my_agents_team",
        graph="< foo > | 0 " if not hasattr(request, "param") else request.param,
    ).wait_for_sync()

    return AgentsTeam(
        id=resp.first.json["response"]["id"],
        version=resp.first.json["response"]["version"],
        name="my_agents_team",
        graph="< foo > | 0 ",
    )


def assert_match_agents_team_header(team: dict, match: AgentsTeam):
    assert team["id"] == match.id
    assert team["version"] == match.version
    assert team.get("created_at")
    assert team["name"] == match.name
    assert team.get("shard") == match.shard
    assert team.get("logo") == match.logo


def assert_match_agents_team(team: dict, match: AgentsTeam):
    assert team["id"] == match.id
    assert team["version"] == match.version
    assert team.get("created_at")
    assert team["name"] == match.name
    assert team.get("shard") == match.shard
    assert team.get("logo") == match.logo
    assert team.get("graph") == match.graph


@pytest.fixture
def oslf(client: ApiClient, funded_wallet: Wallet) -> Oslf:
    resp = client.oslfs.create(funded_wallet, name="my_oslf", query="foo-bar").wait_for_sync()

    return Oslf(
        id=resp.first.json["response"]["id"],
        version=resp.first.json["response"]["version"],
        name="my_oslf",
        query="foo-bar",
    )


def assert_match_oslf_header(oslf: dict, match: Oslf):
    assert oslf["id"] == match.id
    assert oslf["version"] == match.version
    assert oslf.get("created_at")
    assert oslf["name"] == match.name


def assert_match_oslf(oslf: dict, match: Oslf):
    assert oslf["id"] == match.id
    assert oslf["version"] == match.version
    assert oslf.get("created_at")
    assert oslf["name"] == match.name
    assert oslf.get("query") == match.query


def insert_signed_signature(key: SECP256k1, timestamp: int, wallet: Wallet, version: int) -> bytes:
    to_sign = bytes(
        Par(
            exprs=[
                Expr(
                    e_tuple_body=ETuple(
                        ps=[
                            Par(exprs=[Expr(g_int=timestamp)]),
                            Par(exprs=[Expr(g_byte_array=wallet.key.public_key_bytes)]),
                            Par(exprs=[Expr(g_int=version)]),
                        ],
                    ),
                ),
            ],
        ),
    )

    return key.sign(to_sign)


def insert_signed_deploy(key: SECP256k1, timestamp: datetime, wallet: Wallet, version: int) -> dict:
    millis = int(timestamp.timestamp() * 1000)
    signature = insert_signed_signature(key, millis, wallet, version)

    return {
        "timestamp": str(millis),
        "version": str(version),
        "uri_pub_key": key.public_key_bytes.hex(),
        "signature": base64.b64encode(signature).decode(),
    }


def public_key_to_uri(public_key: VerifyingKey) -> str:
    hash_bytes = blake2b(public_key.to_string("uncompressed"), digest_size=32).digest()

    crc_config = Configuration(
        width=14,
        polynomial=0x4805,
        init_value=0x0000,
        final_xor_value=0x0000,
        reverse_input=False,
        reverse_output=False,
    )

    crc14 = Calculator(crc_config, optimized=False)
    crc_val = crc14.checksum(hash_bytes)
    crc_le = crc_val.to_bytes(2, "little")

    full_key = hash_bytes + bytes([crc_le[0]]) + bytes([(crc_le[1] << 2) & 0xFF])
    encoded = zbase32.encode(full_key)[: 270 // 5]

    return f"rho:id:{encoded}"
