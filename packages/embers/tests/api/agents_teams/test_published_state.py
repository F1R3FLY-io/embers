"""Cover for reconciling a team's publication against the PDS that holds it.

Two systems record one fact. The publish record lives on chain; the account it names
lives on a PDS. Nothing compared them, so wiping a development PDS left the interface
presenting a team as published while its handle resolved to nothing -- a divergence
that produced no error anywhere and was diagnosed only by hand.

Hermetic: the team is an echo graph with no `rho:ai:gpt4` node, so publishing and
reading back makes no model call. Publishing does create a real account on the local
PDS, which is what the feature does; each run uses a fresh handle so repeated runs on
the same dev network do not collide.
"""

from datetime import UTC, datetime

import pytest

from tests.client import AgentsTeam, ApiClient, Wallet
from tests.conftest import ECHO_TEAM, insert_signed_deploy
from tests.key import SECP256k1


def _deploy(client: ApiClient, wallet: Wallet, team: AgentsTeam) -> None:
    client.agents_teams.deploy(
        wallet,
        agents_team=team,
        phlo_limit=5_000_000,
        deploy=insert_signed_deploy(SECP256k1.generate(), datetime.now(UTC), wallet, version=0),
    ).wait_for_sync()


def _unique_handle(wallet: Wallet) -> str:
    # The wallet address is fresh per test, so its tail is a handle nobody has taken.
    # Lowercase because atproto normalises handles and the PDS rejects a mismatch.
    return f"team{wallet.address[-12:].lower()}.test"


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
@pytest.mark.parametrize("agents_team", [ECHO_TEAM], indirect=True)
def test_an_unpublished_team_reports_nothing(
    client: ApiClient, funded_wallet: Wallet, agents_team: AgentsTeam
):
    # 404 rather than a body saying "not published": absence is the natural encoding,
    # and it keeps the caller from mistaking an empty record for a real one.
    _deploy(client, funded_wallet, agents_team)

    resp = client.agents_teams.published_state(funded_wallet.address, agents_team.id)

    assert resp.status == 404


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
@pytest.mark.parametrize("agents_team", [ECHO_TEAM], indirect=True)
def test_a_published_team_reports_the_handle_and_that_the_account_exists(
    client: ApiClient, funded_wallet: Wallet, agents_team: AgentsTeam
):
    handle = _unique_handle(funded_wallet)
    _deploy(client, funded_wallet, agents_team)

    client.agents_teams.publish_to_firesky(
        funded_wallet,
        agents_team.id,
        handle=handle,
        email=f"{handle}@test.invalid",
    ).wait_for_sync()

    resp = client.agents_teams.published_state(funded_wallet.address, agents_team.id)

    assert resp.status == 200, resp.body
    state = resp.json

    # The handle is what makes the publication describable at all; before it was
    # recorded, nothing could say which account a team belonged to.
    assert state["handle"] == handle
    assert state["did"]
    # Reconciled against the PDS, which is the whole point: `true` means the account
    # the chain names was actually found.
    assert state["account_exists"] is True
    # Only one deployment exists, so the published one is necessarily the newest.
    assert state["is_latest_deployment"] is True
    assert state["uri"]


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
@pytest.mark.parametrize("agents_team", [ECHO_TEAM], indirect=True)
def test_re_deploying_marks_the_publication_stale(
    client: ApiClient, funded_wallet: Wallet, agents_team: AgentsTeam
):
    """A deploy mints a new registry uri, but the published config record keeps
    naming the old one -- so mentions keep running the older build. That was silent,
    and is the mechanism behind "I re-deployed and nothing changed"."""
    handle = _unique_handle(funded_wallet)
    _deploy(client, funded_wallet, agents_team)
    client.agents_teams.publish_to_firesky(
        funded_wallet,
        agents_team.id,
        handle=handle,
        email=f"{handle}@test.invalid",
    ).wait_for_sync()

    before = client.agents_teams.published_state(funded_wallet.address, agents_team.id).json
    assert before["is_latest_deployment"] is True

    # Save a new version and deploy it: the publication now points at the previous one.
    saved = client.agents_teams.save(
        funded_wallet,
        agents_team.id,
        name=agents_team.name,
        graph=ECHO_TEAM,
    ).wait_for_sync()
    _deploy(
        client,
        funded_wallet,
        AgentsTeam(
            id=agents_team.id,
            version=saved.first.json["response"]["version"],
            name=agents_team.name,
            graph=ECHO_TEAM,
        ),
    )

    after = client.agents_teams.published_state(funded_wallet.address, agents_team.id).json

    # Still published, still the same account -- but no longer the newest build.
    assert after["handle"] == handle
    assert after["account_exists"] is True
    assert after["is_latest_deployment"] is False
    assert after["uri"] == before["uri"]
