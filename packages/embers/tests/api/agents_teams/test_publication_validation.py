"""Publication against the actual demo PDS, including bare handles and reuse."""
import pytest

from tests.api.agents_teams.test_published_state import _deploy, _unique_handle
from tests.conftest import ECHO_TEAM

pytestmark = pytest.mark.pds


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
@pytest.mark.parametrize("agents_team", [ECHO_TEAM], indirect=True)
def test_publication_defaults_validation_and_existing_account(client, funded_wallet, agents_team):
    handle = _unique_handle(funded_wallet)
    bare = handle.removesuffix(".test")
    endpoint = f"/ai-agents-teams/{funded_wallet.address}/{agents_team.id}/publish-to-firesky/prepare"
    valid = {"account_mode": "create", "pds_url": "http://localhost:2583",
             "handle": bare, "email": f"{bare}@example.com", "password": "password123"}
    for field, invalid in [("pds_url", "file:///tmp"), ("email", "invalid"),
                           ("handle", "has spaces"), ("password", "")]:
        response = client.agents_teams._client.post(endpoint, json={**valid, field: invalid})
        assert response.status == 422, response.body
        assert response.json["field_errors"][field]

    _deploy(client, funded_wallet, agents_team)
    client.agents_teams.publish_to_firesky(
        funded_wallet, agents_team.id, handle=bare, email=valid["email"],
        password=valid["password"], account_mode="create",
    ).wait_for_sync()
    state = client.agents_teams.published_state(funded_wallet.address, agents_team.id)
    assert state.json["handle"] == handle
    assert state.json["account_exists"] is True
    original_did = state.json["did"]

    wrong = client.agents_teams._client.post(endpoint, json={
        **valid, "account_mode": "existing", "email": "", "password": "wrong-password",
    })
    assert wrong.status == 422, wrong.body
    assert wrong.json["field_errors"]["password"]

    # Existing account authentication uses its handle, without an email field.
    client.agents_teams.publish_to_firesky(
        funded_wallet, agents_team.id, handle=handle, email="",
        password=valid["password"], account_mode="existing",
    ).wait_for_sync()
    state = client.agents_teams.published_state(funded_wallet.address, agents_team.id)
    assert state.json["did"] == original_did
    assert state.json["account_exists"] is True
    # Stored DID + app-password credentials must work when unpublishing too.
    unpublished = client.agents_teams.unpublish(funded_wallet.address, agents_team.id)
    assert unpublished.status == 200, unpublished.body
