"""Live local-PDS acceptance for the durable Workflow v2 mention path.

The team is a hermetic input-to-output Workflow v2 definition, so this exercises
manifest discovery, durable execution, account publication, thread identity,
and the bounded reply without depending on an external model provider.
"""

from datetime import UTC, datetime

import pytest
import requests

from tests.client import AgentsTeam, ApiClient, Wallet
from tests.conftest import insert_signed_deploy, public_key_to_uri
from tests.key import SECP256k1

PDS_URL = "http://localhost:2583"
POST_COLLECTION = "app.bsky.feed.post"
TERMINAL_REPLY_LIMIT = 280

ECHO_WORKFLOW_V2 = {
    "schemaVersion": "2.0",
    "entryScopeId": "root",
    "interface": {
        "inputs": {"input": {"kind": "string"}},
        "outputs": {"output": {"kind": "string"}},
    },
    "nodes": {
        "input": {"id": "input", "label": "Input", "scopeId": "root", "kind": "input"},
        "output": {"id": "output", "label": "Output", "scopeId": "root", "kind": "output"},
    },
    "policies": {"defaultExecution": "durable", "visibility": "private"},
    "presentation": {"edges": {}, "nodes": {}},
    "routeGroups": {
        "echo": {"id": "echo", "mode": "allMatches", "noMatch": "fail"},
    },
    "scopes": {
        "root": {
            "id": "root",
            "label": "Root",
            "kind": "root",
            "initialNodeIds": ["input"],
            "entryActivities": [],
            "exitActivities": [],
            "history": "none",
        }
    },
    "transitions": {
        "echo": {
            "id": "echo",
            "sourceNodeId": "input",
            "targetNodeId": "output",
            "trigger": "completed",
            "priority": 0,
            "routeGroupId": "echo",
        }
    },
    "types": {},
}


def xrpc_post(method: str, body: dict, token: str | None = None) -> dict:
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    response = requests.post(
        f"{PDS_URL}/xrpc/{method}",
        headers=headers,
        json=body,
        timeout=30,
    )
    assert response.status_code == 200, response.text
    return response.json()


def create_mention_post(bot_handle: str, bot_did: str) -> dict:
    session = xrpc_post(
        "com.atproto.server.createSession",
        {"identifier": "user1.test", "password": "password123"},
    )
    mention = f"@{bot_handle}"
    text = f"{mention} customer-demo-echo"
    return xrpc_post(
        "com.atproto.repo.createRecord",
        {
            "repo": session["did"],
            "collection": POST_COLLECTION,
            "record": {
                "$type": POST_COLLECTION,
                "text": text,
                "createdAt": datetime.now(UTC).isoformat().replace("+00:00", "Z"),
                "facets": [
                    {
                        "index": {
                            "byteStart": 0,
                            "byteEnd": len(mention.encode("utf-8")),
                        },
                        "features": [
                            {
                                "$type": "app.bsky.richtext.facet#mention",
                                "did": bot_did,
                            }
                        ],
                    }
                ],
            },
        },
        session["accessJwt"],
    )


def replies_by(bot_did: str) -> list[dict]:
    response = requests.get(
        f"{PDS_URL}/xrpc/com.atproto.repo.listRecords",
        params={"repo": bot_did, "collection": POST_COLLECTION, "limit": 100},
        timeout=30,
    )
    assert response.status_code == 200, response.text
    return response.json()["records"]


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_at_mention_executes_team_and_posts_bounded_thread_reply(
    client: ApiClient,
    funded_wallet: Wallet,
):
    created = client.agents_teams.create(
        funded_wallet,
        name="workflow-v2-firesky-echo",
        definition=ECHO_WORKFLOW_V2,
    ).wait_for_sync()
    agents_team = AgentsTeam(
        id=created.first.json["response"]["id"],
        version=created.first.json["response"]["version"],
        name="workflow-v2-firesky-echo",
        definition=ECHO_WORKFLOW_V2,
    )
    deployment_key = SECP256k1.generate()
    client.agents_teams.deploy(
        funded_wallet,
        agents_team=agents_team,
        deploy=insert_signed_deploy(
            deployment_key,
            datetime.now(UTC),
            funded_wallet,
            version=0,
        ),
    ).wait_for_sync()
    uri = public_key_to_uri(deployment_key.public_key)
    handle = f"team{funded_wallet.address[-12:].lower()}.test"
    client.agents_teams.publish_to_firesky(
        funded_wallet,
        agents_team.id,
        handle=handle,
        email=f"{handle}@test.invalid",
    ).wait_for_sync()
    published = client.agents_teams.published_state(funded_wallet.address, agents_team.id).json
    assert published["active"] is True
    assert published["uri"] == uri

    source_post = create_mention_post(handle, published["did"])
    reply_to = {
        "parent": {"cid": source_post["cid"], "uri": source_post["uri"]},
        "root": {"cid": source_post["cid"], "uri": source_post["uri"]},
    }
    client.agents_teams.run_on_firesky(
        funded_wallet,
        prompt="customer-demo-echo",
        uri=uri,
        reply_to=reply_to,
    )

    replies = [
        record
        for record in replies_by(published["did"])
        if record["value"].get("reply", {}).get("parent", {}).get("uri") == source_post["uri"]
    ]
    assert len(replies) == 1, replies
    reply = replies[0]["value"]
    assert reply["reply"]["root"]["uri"] == source_post["uri"]
    assert "customer-demo-echo" in reply["text"]
    assert "Run: http://localhost:5173/runs/" in reply["text"]
    assert len(reply["text"]) <= TERMINAL_REPLY_LIMIT
