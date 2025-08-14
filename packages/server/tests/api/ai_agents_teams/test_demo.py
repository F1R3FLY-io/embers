from tests.client import ApiClient


def test_demo(client: ApiClient):
    resp = client._http_client.post("/ai-agents-teams/deploy-demo", json={"name": "test_name"})  # noqa: SLF001
    assert resp.status == 200

    resp = client._http_client.post(  # noqa: SLF001
        "/ai-agents-teams/run-demo",
        json={"name": "test_name", "prompt": "Describe an appearance of human-like robot"},
        timeout=150,
    )
    assert resp.status == 200
    assert "gpt4Answer" in resp.json
    assert "dalle3Answer" in resp.json
    assert "textToAudioAnswer" in resp.json
