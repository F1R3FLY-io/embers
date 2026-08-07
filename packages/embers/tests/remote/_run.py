"""Shared L3 helper: deploy a team graph, run it, and reveal its output.

Used only by the `remote` (L3) tests, which run exclusively against a real remote
deployment — conftest's guard requires EMBERS_L3_CONFIRM=1 and a non-loopback
EMBERS_API_URL — so they exercise the node's real rho:ai:* system channels against
real models. They never run on this machine (the campaign's loopback-only mandate).
"""

from datetime import UTC, datetime

from tests.client import ApiClient, Wallet, sing_contract
from tests.conftest import insert_signed_deploy, public_key_to_uri
from tests.key import SECP256k1

RUN_TIMEOUT = 600


def deploy_graph_run_reveal(client: ApiClient, wallet: Wallet, graph: str, prompt: str) -> object:
    """Deploy a team graph, run it with a prompt, and return the revealed output JSON."""
    deployment_key = SECP256k1.generate()
    client.agents_teams.deploy_graph(
        wallet,
        graph=graph,
        deploy=insert_signed_deploy(deployment_key, datetime.now(UTC), wallet, version=0),
        timeout=RUN_TIMEOUT,
    ).wait_for_sync()
    uri = public_key_to_uri(deployment_key.public_key)

    # The run id is minted in the prepare response (the public run() returns only the
    # send response), so drive the two phases directly against the transport.
    http = client.agents_teams._client  # noqa: SLF001 — run() hides the prepare response that carries the run id
    prepare_request = {"prompt": prompt, "agents_team": uri}
    prepared = http.post("/ai-agents-teams/run/prepare", json=prepare_request)
    assert prepared.status == 200, prepared.body
    run_id = prepared.json["response"]["run_id"]
    sent = http.post(
        "/ai-agents-teams/run/send",
        json={
            "prepare_request": prepare_request,
            "prepare_response": prepared.json["response"],
            "request": sing_contract(wallet, prepared.json["response"]["contract"]),
            "token": prepared.json["token"],
        },
        timeout=RUN_TIMEOUT,
    )
    assert sent.status == 200, sent.body
    return client.agents_teams.reveal_output(wallet, run_id, timeout=RUN_TIMEOUT).json
