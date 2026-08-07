"""Acceptance for deploying an agent standalone.

`POST /ai-agents/deploy/prepare` + `/send` publish a stored agent version's
contract under its registry URI (the `Agent` request variant), or deploy raw
Rholang verbatim (the `Code` variant). This is the one agent operation the
Python client and suite did not previously cover; the two-phase deploy of a
stored agent is the path the frontend drives when a user presses "deploy".
"""

import pytest

from tests.client import Agent, ApiClient, Wallet


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_deploy_stored_agent(client: ApiClient, funded_wallet: Wallet, agent: Agent):
    resp = client.agents.deploy(funded_wallet, agent).wait_for_sync()
    assert resp.first.status == 200

    got = client.agents.get(funded_wallet.address, agent.id, agent.version)
    assert got.json["last_deploy"], "a standalone deploy must stamp last_deploy on the version"

    versions = client.agents.list_versions(funded_wallet.address, agent.id)
    assert len(versions.json["agents"]) == 1
    assert versions.json["agents"][0]["last_deploy"]

    listed = client.agents.list(funded_wallet.address)
    assert len(listed.json["agents"]) == 1
    assert listed.json["agents"][0]["last_deploy"]


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_deploy_raw_code(client: ApiClient, funded_wallet: Wallet):
    resp = client.agents.deploy_code(funded_wallet, code='@"greeting"!("hello")').wait_for_sync()
    assert resp.first.status == 200
