"""Cover for stopping a published team without destroying its account.

Deleting a team removes its entry from the wallet's index and nothing else. `run.rho`
resolves a team by registry uri and calls the deployed contract directly, never
consulting that index, so a published team that had been deleted carried on answering
`@mentions` indefinitely -- from a dashboard where it no longer appeared. Two systems
held one fact and deletion reached only one of them. Unpublishing is the operation
that reaches the other.

**What it does and does not remove.** It deletes the `com.f1r3sky.agentsteam.config`
record that makes f1r3sky treat the handle as a team. The account and handle survive,
so publishing again is possible; the destructive alternative -- deleting the atproto
account -- frees the handle irreversibly and is far more than "stop responding" should
mean. The stored credential on chain is not removed either, because it cannot be: the
env contract keeps firesky tokens in a stack exposing only `push` and `toList`. A
publication is therefore permanent history, and what actually varies is whether the
config record is present, which is what `published_state` reports as `active`.

Hermetic: the team is an echo graph with no `rho:ai:gpt4` node, so nothing here
reaches a model. Publishing does create a real account on the local PDS, which is what
the feature does; each run uses a fresh handle so repeated runs on the same dev
network do not collide.
"""

from datetime import UTC, datetime

import pytest
import requests

from tests.client import AgentsTeam, ApiClient, Wallet
from tests.conftest import ECHO_TEAM, insert_signed_deploy
from tests.key import SECP256k1

PDS_URL = "http://localhost:2583"
CONFIG_COLLECTION = "com.f1r3sky.agentsteam.config"


def _deploy(client: ApiClient, wallet: Wallet, team: AgentsTeam) -> None:
    client.agents_teams.deploy(
        wallet,
        agents_team=team,
        deploy=insert_signed_deploy(SECP256k1.generate(), datetime.now(UTC), wallet, version=0),
    ).wait_for_sync()


def _unique_handle(wallet: Wallet) -> str:
    # The wallet address is fresh per test, so its tail is a handle nobody has taken.
    # Lowercase because atproto normalises handles and the PDS rejects a mismatch.
    # A readable purpose prefix plus a wallet-derived suffix: humans browsing the
    # local feed can tell what the account is, while the suffix keeps parallel
    # test wallets from colliding on the handle.
    return f"echo-team-{wallet.address[-8:].lower()}.test"


def _publish(client: ApiClient, wallet: Wallet, team: AgentsTeam) -> str:
    handle = _unique_handle(wallet)
    _deploy(client, wallet, team)
    client.agents_teams.publish_to_firesky(
        wallet,
        team.id,
        handle=handle,
        email=f"{handle}@test.invalid",
    ).wait_for_sync()
    return handle


def _handle_resolves(handle: str) -> bool:
    resp = requests.get(
        f"{PDS_URL}/xrpc/com.atproto.identity.resolveHandle",
        params={"handle": handle},
        timeout=30,
    )
    return resp.status_code == 200


def _config_record_present(did: str) -> bool:
    resp = requests.get(
        f"{PDS_URL}/xrpc/com.atproto.repo.getRecord",
        params={"collection": CONFIG_COLLECTION, "repo": did, "rkey": "self"},
        timeout=30,
    )
    return resp.status_code == 200


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
@pytest.mark.parametrize("agents_team", [ECHO_TEAM], indirect=True)
def test_unpublishing_stops_the_mentions_and_keeps_the_account(
    client: ApiClient, funded_wallet: Wallet, agents_team: AgentsTeam
):
    handle = _publish(client, funded_wallet, agents_team)

    before = client.agents_teams.published_state(funded_wallet.address, agents_team.id).json
    assert before["active"] is True, before
    assert before["account_exists"] is True
    did = before["did"]
    assert _config_record_present(did)

    resp = client.agents_teams.unpublish(funded_wallet.address, agents_team.id)
    assert resp.status == 200, resp.body

    after = client.agents_teams.published_state(funded_wallet.address, agents_team.id).json

    # The record that makes a mention fire is gone...
    assert after["active"] is False, after
    assert not _config_record_present(did)

    # ...and everything that makes republishing possible is not. The account still
    # exists, the handle still resolves, and both are still what the chain recorded.
    assert after["account_exists"] is True
    assert after["handle"] == handle
    assert after["did"] == did
    assert _handle_resolves(handle)


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
@pytest.mark.parametrize("agents_team", [ECHO_TEAM], indirect=True)
def test_the_deployment_survives_unpublishing(client: ApiClient, funded_wallet: Wallet, agents_team: AgentsTeam):
    """Unpublishing stops the mention path, not the contract.

    The deployed team is still there at its registry uri and still callable by anyone
    who holds it -- `run.rho` looks a team up by uri and never consults the wallet's
    index. That is expected rather than a gap: unpublishing is about the handle, and
    a chain deployment cannot be withdrawn. It is asserted here so the limit is
    stated rather than assumed.
    """
    _publish(client, funded_wallet, agents_team)

    before = client.agents_teams.published_state(funded_wallet.address, agents_team.id).json
    client.agents_teams.unpublish(funded_wallet.address, agents_team.id)
    after = client.agents_teams.published_state(funded_wallet.address, agents_team.id).json

    # Same deployment, same version: nothing about the contract changed.
    assert after["uri"] == before["uri"]
    assert after["published_version"] == before["published_version"]
    assert after["is_latest_deployment"] == before["is_latest_deployment"]


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
@pytest.mark.parametrize("agents_team", [ECHO_TEAM], indirect=True)
def test_unpublishing_a_team_that_was_never_published_fails(
    client: ApiClient, funded_wallet: Wallet, agents_team: AgentsTeam
):
    # Failing loudly rather than reporting success: a caller told "unpublished" about
    # a team that was never published has learned nothing, and the same silence is
    # what a genuinely failed unpublish would produce.
    _deploy(client, funded_wallet, agents_team)

    resp = client.agents_teams.unpublish(funded_wallet.address, agents_team.id)

    assert resp.status == 500
    assert "not published" in resp.body, resp.body


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
@pytest.mark.parametrize("agents_team", [ECHO_TEAM], indirect=True)
def test_republishing_after_unpublishing_works(client: ApiClient, funded_wallet: Wallet, agents_team: AgentsTeam):
    """The reason the account is kept.

    Unpublishing has to be reversible, or it is just a slower deletion. Publishing
    again re-creates the config record on the same account, and the handle is still
    the one the user knows.
    """
    handle = _publish(client, funded_wallet, agents_team)
    did = client.agents_teams.published_state(funded_wallet.address, agents_team.id).json["did"]

    client.agents_teams.unpublish(funded_wallet.address, agents_team.id)
    assert not _config_record_present(did)

    client.agents_teams.publish_to_firesky(
        funded_wallet,
        agents_team.id,
        handle=handle,
        email=f"{handle}@test.invalid",
    ).wait_for_sync()

    state = client.agents_teams.published_state(funded_wallet.address, agents_team.id).json
    assert state["active"] is True, state
    assert state["handle"] == handle
    assert state["did"] == did
    assert _config_record_present(did)
