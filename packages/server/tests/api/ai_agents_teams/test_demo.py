from packages.server.tests.client import ApiClient


def test_demo(client: ApiClient):
    resp = client._http_client.post("/ai-agents-teams/deploy-demo", json={"name": "test_name"})
    print(resp.status)

    resp = client._http_client.post(
        "/ai-agents-teams/run-demo",
        json={"name": "test_name", "prompt": "Describe an appearance of human-like robot"},
        timeout=150,
    )
    print(resp.body)
