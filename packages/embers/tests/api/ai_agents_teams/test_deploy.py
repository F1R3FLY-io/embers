from datetime import UTC, datetime

import pytest
from cryptography.hazmat.primitives.asymmetric import ec

from tests.client import ApiClient, Wallet
from tests.conftest import COMPRESS_TEAM, ECHO_TEAM, GPT_COMPRESS_TEAM, insert_signed_deploy


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
@pytest.mark.parametrize("graph", [ECHO_TEAM, COMPRESS_TEAM, GPT_COMPRESS_TEAM])
def test_deploy_agents_team(client: ApiClient, funded_wallet: Wallet, graph: str):
    deploy = insert_signed_deploy(
        ec.generate_private_key(ec.SECP256K1()),
        datetime.now(UTC),
        funded_wallet,
        version=0,
    )

    resp = client.ai_agents_teams.deploy_graph(
        funded_wallet,
        graph=graph,
        phlo_limit=5_000_000,
        deploy=deploy,
    )
    assert resp.status == 200
