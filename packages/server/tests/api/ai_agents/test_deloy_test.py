from tests.client import ApiClient, Wallet


def test_delploy_test(client: ApiClient, test_wallet: Wallet):
    code = "Nil"
    resp = client.ai_agents.deploy_test(test_wallet, test=code)

    assert resp.status == 200
    assert resp.json["logs"] == []


def test_delploy_test_with_logs(client: ApiClient, test_wallet: Wallet):
    code = """
        new deployId(`rho:rchain:deployId`) in {
            @"logDebug"!(*deployId, "debug log")
        }
    """
    resp = client.ai_agents.deploy_test(test_wallet, test=code)
    assert resp.status == 200
    assert resp.json["logs"] == [{"level": "debug", "message": "debug log"}]
