"""End-to-end cover for a team that broadcasts one agent's output to several
sinks and then joins those back into one agent.

The shape under test is a red team:

    Input ─┬─▶ Hypothesize ─┬─▶ LogicCritic ───────┐
           │                ├─▶ EvidenceCritic ────┼─▶ Refine ─▶ Condense ─▶ Output
           │                ├─▶ FeasibilityCritic ─┘     ▲          ▲
           │                └────── hypothesis ──────────┘          │
           └──── prompt ──▶ (critics, Refine, Condense) ────────────┘

Every agent here carries a canned Rholang body that never calls `rho:ai:gpt4`,
so the run is hermetic: no network, no API key, no model latency, and no
exposure to the 60s `wait_for_deploy` cap. What it does exercise is the real
pipeline -- `parse` resolving the arities, `resolve_agent_codes` fetching each
body from agents storage, `render` emitting the joins, and an actual deploy and
run on chain.

Correctness rests on a single exact-output assertion. That is deliberate: the
expected string can only be produced if every broadcast edge and every join key
is right, so a dropped edge or a mislabelled key fails loudly rather than
degrading quietly.
"""

from datetime import UTC, datetime

import pytest

from tests.client import AgentsTeam, ApiClient, Wallet
from tests.conftest import insert_signed_deploy
from tests.key import SECP256k1

# Canned bodies. Single-input agents receive the value; multi-input agents
# receive a map keyed by the producing node's name, so the two shapes read their
# input differently -- which is itself part of what these assertions pin down.
HYPOTHESIZE = 'contract main(@input, return) = { return!("H<" ++ input ++ ">") }'


def critic(tag: str) -> str:
    return (
        f'contract main(@input, return) = {{ return!("{tag}(" ++ input.get("input") '
        '++ "/" ++ input.get("hypothesize") ++ ")") }'
    )


REFINE = (
    'contract main(@input, return) = { return!("R[" ++ input.get("input") ++ "|" '
    '++ input.get("hypothesize") ++ "|" ++ input.get("logicCritic") ++ "|" '
    '++ input.get("evidenceCritic") ++ "|" ++ input.get("feasibilityCritic") ++ "]") }'
)

CONDENSE = (
    'contract main(@input, return) = { return!("C{" ++ input.get("input") ++ "|" ++ input.get("refine") ++ "}") }'
)

AGENTS = {
    "Hypothesize": HYPOTHESIZE,
    "LogicCritic": critic("L"),
    "EvidenceCritic": critic("E"),
    "FeasibilityCritic": critic("F"),
    "Refine": REFINE,
    "Condense": CONDENSE,
}

VERTICES = {
    "Hypothesize": "aHyp",
    "LogicCritic": "aLog",
    "EvidenceCritic": "aEvi",
    "FeasibilityCritic": "aFea",
    "Refine": "aRef",
    "Condense": "aCon",
}

EDGES = [
    # the prompt reaches every reasoning node
    ("aIn", "aHyp"),
    ("aIn", "aLog"),
    ("aIn", "aEvi"),
    ("aIn", "aFea"),
    ("aIn", "aRef"),
    ("aIn", "aCon"),
    # the hypothesis broadcasts to the three critics and the refiner
    ("aHyp", "aLog"),
    ("aHyp", "aEvi"),
    ("aHyp", "aFea"),
    ("aHyp", "aRef"),
    # the critiques join back into the refiner
    ("aLog", "aRef"),
    ("aEvi", "aRef"),
    ("aFea", "aRef"),
    # and out
    ("aRef", "aCon"),
    ("aCon", "aOut"),
]

PROMPT = "p"
EXPECTED = "C{p|R[p|H<p>|L(p/H<p>)|E(p/H<p>)|F(p/H<p>)]}"


def _escape(json: str) -> str:
    return json.replace('"', '\\"')


def _graphl(contexts: list[tuple[str, str]], edges: list[tuple[str, str]]) -> str:
    """Assembles graphl the way the editor does: vertices nested inside their
    contexts, then a right-nested tensor chain of anonymous edges."""
    body = "0"
    for name, json in reversed(contexts):
        body = f'{{context "{_escape(json)}" for {name} in let {name} = < {name} > in < {name} > | {body}}}'

    chain = "0"
    for source, target in reversed(edges):
        chain = f"{{(let {source} = < {source} > in 0, let {target} = < {target} > in 0) * {chain}}}"

    return f"0 * {body} * {chain}"


def _create_agents(client: ApiClient, wallet: Wallet) -> dict[str, tuple[str, str]]:
    """Creates each canned agent, returning its (id, version) by name."""
    created: dict[str, tuple[str, str]] = {}
    for name, code in AGENTS.items():
        resp = client.agents.create(wallet, name=name, code=code).wait_for_sync()
        created[name] = (
            resp.first.json["response"]["id"],
            resp.first.json["response"]["version"],
        )
    return created


def _red_team_graphl(agents: dict[str, tuple[str, str]]) -> str:
    contexts = [("aIn", '{"type":"input-node"}')]
    for name, vertex in VERTICES.items():
        agent_id, version = agents[name]
        contexts.append(
            (
                vertex,
                f'{{"type":"agent","agentId":"{agent_id}","agentName":"{name}","agentVersion":"{version}"}}',
            )
        )
    contexts.append(("aOut", '{"type":"output-node"}'))
    return _graphl(contexts, EDGES)


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_broadcast_and_join_produce_every_input(client: ApiClient, funded_wallet: Wallet):
    agents = _create_agents(client, funded_wallet)
    graph = _red_team_graphl(agents)

    created = client.agents_teams.create(
        funded_wallet,
        name="red_team",
        graph=graph,
    ).wait_for_sync()
    team = AgentsTeam(
        id=created.first.json["response"]["id"],
        version=created.first.json["response"]["version"],
        name="red_team",
        graph=graph,
    )

    registry_key = SECP256k1.generate()
    client.agents_teams.deploy(
        funded_wallet,
        agents_team=team,
        deploy=insert_signed_deploy(registry_key, datetime.now(UTC), funded_wallet, version=0),
    ).wait_for_sync()

    uri = client.agents_teams.get(funded_wallet.address, team.id, team.version).json["uri"]
    assert uri, "deploying the team should record its registry uri"

    resp = client.agents_teams.run(
        funded_wallet,
        PROMPT,
        agents_team=uri,
    )

    # One assertion pins the whole topology: each critic must have received both
    # the prompt and the hypothesis, the refiner all five of its inputs under the
    # right keys, and the condenser both of its two.
    assert resp.json == EXPECTED


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_raw_graph_deploy_cannot_resolve_agent_nodes(client: ApiClient, funded_wallet: Wallet):
    """The raw-graph deploy path carries no wallet address, and agents are stored
    per wallet, so an agent node cannot be resolved through it. Real usage always
    deploys a saved team by id and version, which does carry the address. This is
    a genuine limitation of that endpoint rather than a bug, so it is pinned by a
    test instead of a comment."""
    agents = _create_agents(client, funded_wallet)
    graph = _red_team_graphl(agents)

    resp = client.agents_teams._client.post(  # noqa: SLF001 - prepare only, no signing needed
        "/ai-agents-teams/deploy/prepare",
        json={
            "type": "Graph",
            "graph": graph,
            "deploy": insert_signed_deploy(SECP256k1.generate(), datetime.now(UTC), funded_wallet, version=0),
        },
    )

    assert resp.status == 500
    assert "wallet address" in resp.body
