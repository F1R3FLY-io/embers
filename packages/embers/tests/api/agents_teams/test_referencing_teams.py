"""Cover for naming the teams that would break if an agent were deleted.

Deleting an agent a team uses leaves that team permanently undeployable, and nothing
shows it: the team's graph carries the agent's name, so the editor keeps drawing the
node, and the failure surfaces only at deploy. Unlike a dangling edge there is no
repair, because the node is well formed and only its referent is gone. This endpoint
is what lets deletion warn first.

Hermetic: the agents carry canned Rholang that never calls `rho:ai:gpt4`, so nothing
here reaches a model.
"""

import json
import re
from datetime import UTC, datetime

import pytest

from tests.client import AgentsTeam, ApiClient, Wallet
from tests.conftest import insert_signed_deploy
from tests.key import SECP256k1

ECHO_AGENT = "contract main(@input, return) = { return!(input) }"


def _graphl(contexts: list[tuple[str, str]], edges: list[tuple[str, str]]) -> str:
    body = "0"
    for name, ctx in reversed(contexts):
        escaped = ctx.replace('"', '\\"')
        body = f'{{context "{escaped}" for {name} in let {name} = < {name} > in < {name} > | {body}}}'

    chain = "0"
    for source, target in reversed(edges):
        chain = f"{{(let {source} = < {source} > in 0, let {target} = < {target} > in 0) * {chain}}}"

    return f"0 * {body} * {chain}"


def _team_using(agent_id: str, agent_name: str, agent_version: str) -> str:
    """Input -> agent -> Output."""
    return _graphl(
        [
            ("aIn", '{"type":"input-node"}'),
            (
                "aAgent",
                json.dumps(
                    {
                        "agentId": agent_id,
                        "agentName": agent_name,
                        "agentVersion": agent_version,
                        "type": "agent",
                    }
                ),
            ),
            ("aOut", '{"type":"output-node"}'),
        ],
        [("aIn", "aAgent"), ("aAgent", "aOut")],
    )


def _create_agent(client: ApiClient, wallet: Wallet, name: str) -> tuple[str, str]:
    resp = client.agents.create(wallet, name=name, code=ECHO_AGENT).wait_for_sync()
    response = resp.first.json["response"]
    return response["id"], response["version"]


def _create_team(client: ApiClient, wallet: Wallet, name: str, graph: str) -> AgentsTeam:
    resp = client.agents_teams.create(wallet, name=name, graph=graph).wait_for_sync()
    response = resp.first.json["response"]
    return AgentsTeam(id=response["id"], version=response["version"], name=name, graph=graph)


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_an_unreferenced_agent_reports_no_teams(client: ApiClient, funded_wallet: Wallet):
    agent_id, _ = _create_agent(client, funded_wallet, "Unused")

    resp = client.agents_teams.referencing_teams(funded_wallet.address, agent_id)

    assert resp.status == 200, resp.body
    assert resp.json == []


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_every_team_using_the_agent_is_named(client: ApiClient, funded_wallet: Wallet):
    # Two teams on the agent, one on a different agent: the third must not appear, or
    # a deletion warning would over-report and destroy work it had no reason to.
    used_id, used_version = _create_agent(client, funded_wallet, "Used")
    other_id, other_version = _create_agent(client, funded_wallet, "Other")

    first = _create_team(client, funded_wallet, "first", _team_using(used_id, "Used", used_version))
    second = _create_team(client, funded_wallet, "second", _team_using(used_id, "Used", used_version))
    _create_team(client, funded_wallet, "third", _team_using(other_id, "Other", other_version))

    resp = client.agents_teams.referencing_teams(funded_wallet.address, used_id)

    assert resp.status == 200, resp.body
    named = {team["id"] for team in resp.json}
    assert named == {first.id, second.id}
    assert all(team["name"] in {"first", "second"} for team in resp.json)


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_a_reference_only_in_an_older_version_is_not_reported(client: ApiClient, funded_wallet: Wallet):
    """Only the latest version counts.

    A team whose current version no longer uses the agent is not broken going
    forward, and treating it as affected would delete work to protect a historical
    version nobody deploys. The limitation is deliberate, so it is pinned here."""
    used_id, used_version = _create_agent(client, funded_wallet, "Dropped")
    other_id, other_version = _create_agent(client, funded_wallet, "Kept")

    team = _create_team(client, funded_wallet, "moved-on", _team_using(used_id, "Dropped", used_version))

    assert {t["id"] for t in client.agents_teams.referencing_teams(funded_wallet.address, used_id).json} == {team.id}

    # Save a newer version that uses the other agent instead.
    client.agents_teams.save(
        funded_wallet,
        team.id,
        name="moved-on",
        graph=_team_using(other_id, "Kept", other_version),
    ).wait_for_sync()

    resp = client.agents_teams.referencing_teams(funded_wallet.address, used_id)

    assert resp.status == 200, resp.body
    assert resp.json == []


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_a_wallet_with_no_teams_reports_nothing(client: ApiClient, funded_wallet: Wallet):
    # Guards the empty case against erroring rather than returning [].
    agent_id, _ = _create_agent(client, funded_wallet, "Lonely")

    resp = client.agents_teams.referencing_teams(funded_wallet.address, agent_id)

    assert resp.status == 200, resp.body
    assert resp.json == []


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_the_deploy_error_names_the_agent_not_only_its_id(client: ApiClient, funded_wallet: Wallet):
    """The other half of the same problem: when a team is already broken, the failure
    has to say which node to look at."""
    # A graph naming an agent that was never created.
    graph = _team_using("no-such-agent", "Vanished", "no-such-version")
    team = _create_team(client, funded_wallet, "broken", graph)

    # A missing agent is retried 30 times at one-second intervals before the deploy
    # gives up -- the retry exists for observer lag after a write, and cannot tell
    # "not replicated yet" from "deleted". So this legitimately takes ~30s, well past
    # the client's default timeout.
    resp = client.agents_teams._client.post(  # noqa: SLF001 - prepare only
        "/ai-agents-teams/deploy/prepare",
        timeout=120,
        json={
            "type": "AgentsTeam",
            "id": team.id,
            "version": team.version,
            "address": funded_wallet.address,
            "deploy": insert_signed_deploy(SECP256k1.generate(), datetime.now(UTC), funded_wallet, version=0),
        },
    )

    assert resp.status == 500
    assert "Vanished" in resp.body, resp.body
    assert re.search(r"restore the agent", resp.body), resp.body
