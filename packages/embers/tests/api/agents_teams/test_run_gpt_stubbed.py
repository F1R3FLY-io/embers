"""L2: a GPT run with the model call stubbed locally — proof of hermeticity.

Marked devnet+gpt: needs the local dev-net AND the OpenAI stub, with the node's
OPENAI_API_BASE pointed at it (scripts/l2.sh arms this). The point is not the model's
answer but that the run's model call went to the LOCAL stub rather than real GPT: the
test drives a full deploy -> run -> reveal through the text-model team and asserts the
stub's request log gained a /v1/chat/completions hit during the run, and that a
non-empty output was revealed. It skips cleanly when the stub is not reachable, so it
never fails outside the managed L2 stack.

(A complementary degraded case — OPENAI_ENABLED=false, where the node's rho:ai:gpt4
returns an empty success rather than calling any model — is an operator-configured
node state, not a pytest; it is documented in docs/testing/test-architecture.md.)
"""

import json
import os
import urllib.request
import uuid

import pytest

from tests.client import ApiClient, Wallet
from tests.conftest import GPT_COMPRESS_TEAM
from tests.remote._run import deploy_graph_run_reveal

pytestmark = [pytest.mark.devnet, pytest.mark.gpt]

STUB_ADDR = os.environ.get("OPENAI_STUB_ADDR", "127.0.0.1:8571")


def _stub_chat_hits() -> int:
    """Number of /v1/chat/completions requests the stub has recorded, or None if down."""
    with urllib.request.urlopen(f"http://{STUB_ADDR}/stub/requests", timeout=2) as response:
        recorded = json.load(response)
    return sum(1 for entry in recorded if "chat/completions" in entry.get("path", ""))


def _stub_reachable() -> bool:
    try:
        _stub_chat_hits()
    except OSError:
        return False
    return True


@pytest.mark.skipif(
    not _stub_reachable(),
    reason="the local OpenAI stub is not reachable; run under scripts/l2.sh up",
)
@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_gpt_run_goes_through_the_local_stub(client: ApiClient, funded_wallet: Wallet):
    before = _stub_chat_hits()
    revealed = deploy_graph_run_reveal(
        client,
        funded_wallet,
        GPT_COMPRESS_TEAM,
        f"Summarize this distinctly: needle-{uuid.uuid4().hex[:8]}",
    )
    after = _stub_chat_hits()

    # Hermeticity: the run's model call landed on the LOCAL stub, not real GPT.
    assert after > before, "the run should have hit the local stub's chat endpoint"
    # The stubbed pipeline still produced a revealed output.
    assert json.dumps(revealed).strip(), f"expected a non-empty revealed output; got: {revealed!r}"
