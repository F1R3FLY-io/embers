import pytest

from tests.client import ApiClient, Wallet


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_demo(client: ApiClient, funded_wallet: Wallet):
    resp = client.ai_agents_teams.deploy(funded_wallet, graph="", phlo_limit=500_000)
    assert resp.status == 200

    resp = client._http_client.post(  # noqa: SLF001
        "/ai-agents-teams/run-demo",
        json={"name": "demo_agents_team", "prompt": "Describe an appearance of human-like robot"},
        timeout=150,
    )
    assert resp.status == 200
    assert "gpt4Answer" in resp.json
    assert "dalle3Answer" in resp.json
    assert "textToAudioAnswer" in resp.json
