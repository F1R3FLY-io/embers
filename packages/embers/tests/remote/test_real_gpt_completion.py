"""L3: a real GPT completion through the node's rho:ai:gpt4 channel.

Gated off by default — the `remote` marker's guard in conftest requires
EMBERS_L3_CONFIRM=1 and a non-loopback EMBERS_API_URL — so this NEVER runs on this
machine. It proves the real model path end to end: deploy a text-model team, run it,
and reveal an answer that only a real model produces and that is NOT the L2 stub's
echo (the stub prefixes every completion with "[stub:...]").
"""

import json

import pytest

from tests.client import ApiClient, Wallet
from tests.conftest import GPT_COMPRESS_TEAM
from tests.remote._run import deploy_graph_run_reveal

pytestmark = [pytest.mark.remote, pytest.mark.gpt, pytest.mark.devnet]


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_real_gpt_completion(client: ApiClient, funded_wallet: Wallet):
    revealed = deploy_graph_run_reveal(
        client,
        funded_wallet,
        GPT_COMPRESS_TEAM,
        "Reply with only the capital city of France.",
    )
    text = json.dumps(revealed).lower()
    assert "[stub:" not in text, f"expected a real model completion, not the L2 stub: {text}"
    assert "paris" in text, f"a real GPT completion should name Paris; got: {text}"
