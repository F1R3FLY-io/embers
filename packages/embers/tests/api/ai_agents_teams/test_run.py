from datetime import UTC, datetime

import pytest
from cryptography.hazmat.primitives.asymmetric import ec

from tests.client import ApiClient, Wallet
from tests.conftest import ECHO_TEAM, insert_signed_deploy, public_key_to_uri, wait_for_read_node_sync


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
@pytest.mark.parametrize("graph", [ECHO_TEAM])
def test_run_agents_team(client: ApiClient, funded_wallet: Wallet, graph: str):
    private_key = ec.generate_private_key(ec.SECP256K1())
    timestamp = datetime.now(UTC)
    version = 0
    deploy = insert_signed_deploy(private_key, timestamp, funded_wallet, version)

    resp = client.ai_agents_teams.deploy_graph(
        funded_wallet,
        graph=graph,
        phlo_limit=5_000_000,
        deploy=deploy,
    )
    assert resp.status == 200

    wait_for_read_node_sync()

    agents_team = public_key_to_uri(private_key.public_key())
    resp = client.ai_agents_teams.run(funded_wallet, "echo", phlo_limit=5_000_000, agents_team=agents_team)
    assert resp.status == 200
    assert resp.json == "echo"
