from datetime import UTC, datetime

import pytest

from tests.client import AgentsTeam, ApiClient, Wallet
from tests.conftest import GPT_COMPRESS_TEAM, insert_signed_deploy, public_key_to_uri
from tests.key import SECP256k1


@pytest.mark.skip(reason="manual")
@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
@pytest.mark.parametrize("agents_team", [GPT_COMPRESS_TEAM], indirect=True)
def test_publish_and_run_graph(client: ApiClient, funded_wallet: Wallet, agents_team: AgentsTeam):
    key = SECP256k1.generate()

    deploy = insert_signed_deploy(
        key,
        datetime.now(UTC),
        funded_wallet,
        version=0,
    )

    resp = client.ai_agents_teams.deploy(
        funded_wallet,
        agents_team=agents_team,
        phlo_limit=5_000_000,
        deploy=deploy,
    ).wait_for_sync()
    assert resp.first.status == 200

    client.ai_agents_teams.publish_to_firesky(funded_wallet, agents_team.id)
    assert resp.first.status == 200

    client.ai_agents_teams.run_on_firesky(
        funded_wallet,
        prompt="describe our universe. max 300 graphemes",
        uri=public_key_to_uri(key.public_key),
    )
    assert resp.first.status == 200
