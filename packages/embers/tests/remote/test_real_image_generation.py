"""L3: a real image generation through the node's rho:ai:dalle3 channel.

Gated off by default (see the module guard); never runs on this machine. Derives an
image team from the text-model team by retyping the model node to the image kind
("tti-model", the compiler's image node type in graphl_parsing.rs), deploys and runs
it, and asserts the revealed output is a real REMOTE image URL — not the L2 stub's
loopback stub-assets URL.
"""

import json

import pytest

from tests.client import ApiClient, Wallet
from tests.conftest import GPT_COMPRESS_TEAM
from tests.remote._run import deploy_graph_run_reveal

pytestmark = [pytest.mark.remote, pytest.mark.gpt, pytest.mark.devnet]

# The text-model team with its model node retyped to the image (text-to-image) kind.
TTI_COMPRESS_TEAM = GPT_COMPRESS_TEAM.replace("text-model", "tti-model")


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_real_image_generation(client: ApiClient, funded_wallet: Wallet):
    revealed = deploy_graph_run_reveal(
        client,
        funded_wallet,
        TTI_COMPRESS_TEAM,
        "A single red circle centered on a white background.",
    )
    text = json.dumps(revealed)
    assert "[stub:" not in text, f"expected a real image URL, not the L2 stub: {text}"
    assert "127.0.0.1" not in text, f"expected a real remote image URL, not the loopback stub: {text}"
    assert "stub-assets" not in text, f"expected a real remote image URL, not the loopback stub: {text}"
    assert "http" in text, f"expected an image URL in the revealed output; got: {text}"
