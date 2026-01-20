from datetime import UTC, datetime

import pytest

from tests.client import ApiClient, Wallet
from tests.conftest import ECHO_TEAM, insert_signed_deploy, public_key_to_uri
from tests.key import SECP256k1


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
@pytest.mark.parametrize("graph", [ECHO_TEAM])
def test_run(client: ApiClient, funded_wallet: Wallet, graph: str):
    private_key = SECP256k1.generate()
    deploy = insert_signed_deploy(private_key, datetime.now(UTC), funded_wallet, version=0)

    client.agents_teams.deploy_graph(
        funded_wallet,
        graph=graph,
        phlo_limit=5_000_000,
        deploy=deploy,
    )

    agents_team = public_key_to_uri(private_key.public_key)
    resp = client.agents_teams.run(funded_wallet, "echo", phlo_limit=5_000_000, agents_team=agents_team)
    assert resp.json == "echo"
