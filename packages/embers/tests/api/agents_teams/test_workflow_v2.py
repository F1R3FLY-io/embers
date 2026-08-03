"""Live acceptance coverage for Workflow v2 control-flow semantics.

This test talks to a running Embers/F1r3node/F1r3drive stack.  It deliberately
uses no model activity: the observable value is transformed only by routing,
which makes failures attributable to the workflow kernel instead of an external
provider.  One definition covers a false/true first-match conditional, all-match
fan-out, correlated all-join fan-in, and a bounded back-edge whose
``iterationLimit`` disposition exits the cycle.
"""

import time

from datetime import UTC, datetime
from typing import Any

import pytest

from tests.client import AgentsTeam, ApiClient, Wallet
from tests.conftest import insert_signed_deploy
from tests.key import SECP256k1

WORKFLOW_V2 = {
    "schemaVersion": "2.0",
    "entryScopeId": "root",
    "interface": {
        "inputs": {"input": {"kind": "string"}},
        "outputs": {"output": {"kind": "legacy"}},
    },
    "nodes": {
        "input": {"id": "input", "label": "Input", "scopeId": "root", "kind": "input"},
        "choose": {"id": "choose", "label": "Choose", "scopeId": "root", "kind": "gateway"},
        "unused": {"id": "unused", "label": "Unused", "scopeId": "root", "kind": "output"},
        "fork": {"id": "fork", "label": "Fork", "scopeId": "root", "kind": "gateway"},
        "left": {"id": "left", "label": "Left", "scopeId": "root", "kind": "gateway"},
        "right": {"id": "right", "label": "Right", "scopeId": "root", "kind": "gateway"},
        "join": {
            "id": "join",
            "label": "Join",
            "scopeId": "root",
            "kind": "gateway",
            "join": {
                "mode": "all",
                "correlation": {"kind": "routeGroup", "routeGroupId": "fork"},
            },
        },
        "loop": {"id": "loop", "label": "Loop", "scopeId": "root", "kind": "gateway"},
        "output": {"id": "output", "label": "Output", "scopeId": "root", "kind": "output"},
    },
    "policies": {"defaultExecution": "durable", "visibility": "private"},
    "presentation": {"edges": {}, "nodes": {}},
    "routeGroups": {
        "start": {"id": "start", "mode": "allMatches", "noMatch": "fail"},
        "choice": {"id": "choice", "mode": "firstMatch", "noMatch": "fail"},
        "fork": {"id": "fork", "mode": "allMatches", "noMatch": "fail"},
        "left": {"id": "left", "mode": "allMatches", "noMatch": "fail"},
        "right": {"id": "right", "mode": "allMatches", "noMatch": "fail"},
        "joined": {"id": "joined", "mode": "allMatches", "noMatch": "fail"},
        "again": {"id": "again", "mode": "firstMatch", "noMatch": "fail"},
        "limit": {"id": "limit", "mode": "allMatches", "noMatch": "fail"},
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
        "input-choose": {
            "id": "input-choose",
            "sourceNodeId": "input",
            "targetNodeId": "choose",
            "trigger": "completed",
            "priority": 0,
            "routeGroupId": "start",
        },
        "choose-unused": {
            "id": "choose-unused",
            "sourceNodeId": "choose",
            "targetNodeId": "unused",
            "trigger": "completed",
            "guard": {"kind": "literal", "value": False},
            "priority": 0,
            "routeGroupId": "choice",
        },
        "choose-fork": {
            "id": "choose-fork",
            "sourceNodeId": "choose",
            "targetNodeId": "fork",
            "trigger": "completed",
            "guard": {"kind": "literal", "value": True},
            "priority": 1,
            "routeGroupId": "choice",
        },
        "fork-left": {
            "id": "fork-left",
            "sourceNodeId": "fork",
            "targetNodeId": "left",
            "trigger": "completed",
            "priority": 0,
            "routeGroupId": "fork",
        },
        "fork-right": {
            "id": "fork-right",
            "sourceNodeId": "fork",
            "targetNodeId": "right",
            "trigger": "completed",
            "priority": 1,
            "routeGroupId": "fork",
        },
        "left-join": {
            "id": "left-join",
            "sourceNodeId": "left",
            "targetNodeId": "join",
            "trigger": "completed",
            "priority": 0,
            "routeGroupId": "left",
        },
        "right-join": {
            "id": "right-join",
            "sourceNodeId": "right",
            "targetNodeId": "join",
            "trigger": "completed",
            "priority": 0,
            "routeGroupId": "right",
        },
        "join-loop": {
            "id": "join-loop",
            "sourceNodeId": "join",
            "targetNodeId": "loop",
            "trigger": "completed",
            "priority": 0,
            "routeGroupId": "joined",
        },
        "loop-again": {
            "id": "loop-again",
            "sourceNodeId": "loop",
            "targetNodeId": "loop",
            "trigger": "completed",
            "guard": {"kind": "literal", "value": True},
            "priority": 0,
            "routeGroupId": "again",
            "loop": {"maxIterations": 2},
        },
        "loop-output": {
            "id": "loop-output",
            "sourceNodeId": "loop",
            "targetNodeId": "output",
            "trigger": "iterationLimit",
            "priority": 0,
            "routeGroupId": "limit",
        },
    },
    "types": {},
}


SIGNAL_WAIT_WORKFLOW_V2 = {
    "schemaVersion": "2.0",
    "entryScopeId": "root",
    "interface": {
        "inputs": {"input": {"kind": "string"}},
        "outputs": {"output": {"kind": "legacy"}},
    },
    "nodes": {
        "input": {"id": "input", "label": "Input", "scopeId": "root", "kind": "input"},
        "wait": {
            "id": "wait",
            "label": "Wait for signal",
            "scopeId": "root",
            "kind": "wait",
            "waitFor": "signal",
            "signal": "continue",
        },
        "output": {"id": "output", "label": "Output", "scopeId": "root", "kind": "output"},
    },
    "policies": {"defaultExecution": "durable", "visibility": "private"},
    "presentation": {"edges": {}, "nodes": {}},
    "routeGroups": {
        "to-wait": {"id": "to-wait", "mode": "allMatches", "noMatch": "fail"},
        "to-output": {"id": "to-output", "mode": "allMatches", "noMatch": "fail"},
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
        "input-wait": {
            "id": "input-wait",
            "sourceNodeId": "input",
            "targetNodeId": "wait",
            "trigger": "completed",
            "priority": 0,
            "routeGroupId": "to-wait",
        },
        "wait-output": {
            "id": "wait-output",
            "sourceNodeId": "wait",
            "targetNodeId": "output",
            "trigger": "completed",
            "priority": 0,
            "routeGroupId": "to-output",
        },
    },
    "types": {},
}


def approval_workflow_v2(approver: str) -> dict[str, Any]:
    """Build a publishable approval workflow bound to one live wallet."""
    return {
        **SIGNAL_WAIT_WORKFLOW_V2,
        "nodes": {
            "input": {
                "id": "input",
                "label": "Input",
                "scopeId": "root",
                "kind": "input",
            },
            "approval": {
                "id": "approval",
                "label": "Review",
                "scopeId": "root",
                "kind": "approval",
                "policyId": "live-review",
                "quorum": 1,
                "approvers": [approver],
            },
            "approved": {
                "id": "approved",
                "label": "Approved",
                "scopeId": "root",
                "kind": "output",
            },
            "rejected": {
                "id": "rejected",
                "label": "Rejected",
                "scopeId": "root",
                "kind": "output",
            },
        },
        "routeGroups": {
            "to-approval": {
                "id": "to-approval",
                "mode": "allMatches",
                "noMatch": "fail",
            },
            "approved": {
                "id": "approved",
                "mode": "allMatches",
                "noMatch": "fail",
            },
            "rejected": {
                "id": "rejected",
                "mode": "allMatches",
                "noMatch": "fail",
            },
        },
        "transitions": {
            "input-approval": {
                "id": "input-approval",
                "sourceNodeId": "input",
                "targetNodeId": "approval",
                "trigger": "completed",
                "priority": 0,
                "routeGroupId": "to-approval",
            },
            "approval-approved": {
                "id": "approval-approved",
                "sourceNodeId": "approval",
                "targetNodeId": "approved",
                "trigger": "completed",
                "priority": 0,
                "routeGroupId": "approved",
            },
            "approval-rejected": {
                "id": "approval-rejected",
                "sourceNodeId": "approval",
                "targetNodeId": "rejected",
                "trigger": "rejected",
                "priority": 0,
                "routeGroupId": "rejected",
            },
        },
    }


def assert_json_contains(actual: Any, expected: Any) -> None:
    """Assert that canonical JSON preserves every submitted value."""
    if isinstance(expected, dict):
        assert isinstance(actual, dict)
        assert expected.keys() <= actual.keys()
        for key, value in expected.items():
            assert_json_contains(actual[key], value)
    elif isinstance(expected, list):
        assert isinstance(actual, list)
        assert len(actual) == len(expected)
        for actual_item, expected_item in zip(actual, expected, strict=True):
            assert_json_contains(actual_item, expected_item)
    else:
        assert actual == expected


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_branch_fanout_join_and_bounded_cycle_execute_durably(client: ApiClient, funded_wallet: Wallet):
    created = client.agents_teams.create(
        funded_wallet,
        name="workflow-v2-live-acceptance",
        definition=WORKFLOW_V2,
    ).wait_for_sync()
    team = AgentsTeam(
        id=created.first.json["response"]["id"],
        version=created.first.json["response"]["version"],
        name="workflow-v2-live-acceptance",
        definition=WORKFLOW_V2,
    )

    deployment_key = SECP256k1.generate()
    client.agents_teams.deploy(
        funded_wallet,
        agents_team=team,
        deploy=insert_signed_deploy(
            deployment_key,
            datetime.now(UTC),
            funded_wallet,
            version=0,
        ),
    ).wait_for_sync()

    deployed = client.agents_teams.get(funded_wallet.address, team.id, team.version).json
    assert_json_contains(deployed["definition"], WORKFLOW_V2)
    assert deployed["uri"]

    response = client.agents_teams.run(
        funded_wallet,
        "live-branch-cycle",
        agents_team=deployed["uri"],
    )
    assert response.json == {
        "Left": "live-branch-cycle",
        "Right": "live-branch-cycle",
    }


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_authenticated_signal_resumes_durable_wait(client: ApiClient, funded_wallet: Wallet):
    created = client.agents_teams.create(
        funded_wallet,
        name="workflow-v2-signal-wait-acceptance",
        definition=SIGNAL_WAIT_WORKFLOW_V2,
    ).wait_for_sync()
    team = AgentsTeam(
        id=created.first.json["response"]["id"],
        version=created.first.json["response"]["version"],
        name="workflow-v2-signal-wait-acceptance",
        definition=SIGNAL_WAIT_WORKFLOW_V2,
    )

    client.agents_teams.deploy(
        funded_wallet,
        agents_team=team,
        deploy=insert_signed_deploy(
            SECP256k1.generate(),
            datetime.now(UTC),
            funded_wallet,
            version=0,
        ),
    ).wait_for_sync()
    deployed = client.agents_teams.get(funded_wallet.address, team.id, team.version).json

    suspended = client.agents_teams.run(
        funded_wallet,
        "before-signal",
        agents_team=deployed["uri"],
    ).json
    assert suspended["disposition"] == "suspended"
    assert len(suspended["waits"]) == 1

    result = client.agents_teams.run_command(
        funded_wallet,
        suspended["runId"],
        expected_revision=suspended["revision"],
        command_type="signal",
        wait_id=suspended["waits"][0]["id"],
        name="continue",
        payload="resumed-by-signal",
    )
    assert result.json == "resumed-by-signal"

    completed = client.agents_teams.get_run(suspended["runId"]).json
    assert completed["status"] == "completed"
    assert completed["wait_ids"] == []


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_authenticated_approval_and_rejection_route_end_to_end(
    client: ApiClient,
    funded_wallet: Wallet,
):
    workflow = approval_workflow_v2(funded_wallet.address)
    created = client.agents_teams.create(
        funded_wallet,
        name="workflow-v2-approval-acceptance",
        definition=workflow,
    ).wait_for_sync()
    team = AgentsTeam(
        id=created.first.json["response"]["id"],
        version=created.first.json["response"]["version"],
        name="workflow-v2-approval-acceptance",
        definition=workflow,
    )

    client.agents_teams.deploy(
        funded_wallet,
        agents_team=team,
        deploy=insert_signed_deploy(
            SECP256k1.generate(),
            datetime.now(UTC),
            funded_wallet,
            version=0,
        ),
    ).wait_for_sync()
    deployed = client.agents_teams.get(funded_wallet.address, team.id, team.version).json

    approved = client.agents_teams.run(
        funded_wallet,
        "await-approval",
        agents_team=deployed["uri"],
    ).json
    assert approved["disposition"] == "suspended"
    assert len(approved["waits"]) == 1
    invalid_result = client.agents_teams.run_command(
        funded_wallet,
        approved["runId"],
        expected_revision=approved["revision"],
        command_type="signal",
        wait_id=approved["waits"][0]["id"],
        name="not-an-approval",
        payload={"decision": "invalid"},
        expected_status=500,
    )
    assert "approval" in invalid_result.body.lower()
    after_invalid = client.agents_teams.get_run(approved["runId"]).json
    assert after_invalid["status"] == "suspended"
    assert [event["event_type"] for event in after_invalid["events"]].count("runSuspended") == 1

    approved_result = client.agents_teams.run_command(
        funded_wallet,
        approved["runId"],
        expected_revision=approved["revision"],
        command_type="approve",
        wait_id=approved["waits"][0]["id"],
        decision_id="live-decision-approve",
        payload={"decision": "approved"},
    )
    assert approved_result.json == {"decision": "approved"}
    approved_run = client.agents_teams.get_run(approved["runId"]).json
    assert approved_run["status"] == "completed"
    assert any(event["event_type"] == "approvalRecorded" for event in approved_run["events"])
    approved_event_types = [event["event_type"] for event in approved_run["events"]]
    cursor = -1
    for expected_event in [
        "commandClaimed",
        "commandRejected",
        "commandClaimed",
        "approvalRecorded",
        "runCompleted",
    ]:
        cursor = approved_event_types.index(expected_event, cursor + 1)

    rejected = client.agents_teams.run(
        funded_wallet,
        "await-rejection",
        agents_team=deployed["uri"],
    ).json
    assert rejected["disposition"] == "suspended"
    assert len(rejected["waits"]) == 1
    rejected_result = client.agents_teams.run_command(
        funded_wallet,
        rejected["runId"],
        expected_revision=rejected["revision"],
        command_type="reject",
        wait_id=rejected["waits"][0]["id"],
        payload={"decision": "rejected"},
    )
    assert rejected_result.json == {"decision": "rejected"}
    rejected_run = client.agents_teams.get_run(rejected["runId"]).json
    assert rejected_run["status"] == "completed"
    assert rejected_run["wait_ids"] == []


def poll_run_until(
    client: ApiClient,
    run_id: str,
    statuses: set[str],
    timeout_seconds: float = 180.0,
) -> dict[str, Any]:
    """Poll the run inspector until the run reaches one of ``statuses``."""
    deadline = time.monotonic() + timeout_seconds
    run = client.agents_teams.get_run(run_id).json
    while run["status"] not in statuses:
        assert time.monotonic() < deadline, (
            f"run {run_id} stayed {run['status']!r}; events: "
            f"{[event['event_type'] for event in run['events']]}"
        )
        time.sleep(2.0)
        run = client.agents_teams.get_run(run_id).json
    return run


DURABLE_YIELD_WORKFLOW_V2 = {
    **SIGNAL_WAIT_WORKFLOW_V2,
    "nodes": {
        "input": {"id": "input", "label": "Input", "scopeId": "root", "kind": "input"},
        "gate": {"id": "gate", "label": "Gate", "scopeId": "root", "kind": "gateway"},
        "output": {"id": "output", "label": "Output", "scopeId": "root", "kind": "output"},
    },
    "routeGroups": {
        "to-gate": {"id": "to-gate", "mode": "allMatches", "noMatch": "fail"},
        "to-output": {"id": "to-output", "mode": "allMatches", "noMatch": "fail"},
    },
    "transitions": {
        "input-gate": {
            "id": "input-gate",
            "sourceNodeId": "input",
            "targetNodeId": "gate",
            "trigger": "completed",
            "priority": 0,
            "routeGroupId": "to-gate",
        },
        "gate-output": {
            "id": "gate-output",
            "sourceNodeId": "gate",
            "targetNodeId": "output",
            "trigger": "completed",
            "priority": 0,
            "routeGroupId": "to-output",
            "loop": {"durableYield": True},
        },
    },
}


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_durable_yield_auto_resumes_via_scheduler(client: ApiClient, funded_wallet: Wallet):
    created = client.agents_teams.create(
        funded_wallet,
        name="workflow-v2-durable-yield-acceptance",
        definition=DURABLE_YIELD_WORKFLOW_V2,
    ).wait_for_sync()
    team = AgentsTeam(
        id=created.first.json["response"]["id"],
        version=created.first.json["response"]["version"],
        name="workflow-v2-durable-yield-acceptance",
        definition=DURABLE_YIELD_WORKFLOW_V2,
    )

    client.agents_teams.deploy(
        funded_wallet,
        agents_team=team,
        deploy=insert_signed_deploy(
            SECP256k1.generate(),
            datetime.now(UTC),
            funded_wallet,
            version=0,
        ),
    ).wait_for_sync()
    deployed = client.agents_teams.get(funded_wallet.address, team.id, team.version).json

    suspended = client.agents_teams.run(
        funded_wallet,
        "yield-then-continue",
        agents_team=deployed["uri"],
    ).json
    assert suspended["disposition"] == "suspended"
    assert len(suspended["waits"]) == 1
    assert suspended["waits"][0]["id"].startswith("yield:")

    parked = client.agents_teams.get_run(suspended["runId"]).json
    assert parked["due_at"] is not None, "a durable yield publishes its resume deadline"

    completed = poll_run_until(client, suspended["runId"], {"completed", "failed", "cancelled"})
    assert completed["status"] == "completed", (
        f"scheduler auto-resume should complete the run; events: "
        f"{[event['event_type'] for event in completed['events']]}"
    )
    assert completed["wait_ids"] == []
    event_types = [event["event_type"] for event in completed["events"]]
    cursor = -1
    for expected_event in ["durableYield", "waitSatisfied", "runResumed", "runCompleted"]:
        cursor = event_types.index(expected_event, cursor + 1)


def approval_timeout_workflow_v2(approver: str) -> dict[str, Any]:
    """An approval workflow whose deadline must fire the timedOut route."""
    workflow = approval_workflow_v2(approver)
    workflow["nodes"]["approval"]["policy"] = {"timeout": {"afterMillis": 5000}}
    workflow["nodes"]["timed-out"] = {
        "id": "timed-out",
        "label": "Timed out",
        "scopeId": "root",
        "kind": "output",
    }
    workflow["routeGroups"]["timed-out"] = {
        "id": "timed-out",
        "mode": "allMatches",
        "noMatch": "fail",
    }
    workflow["transitions"]["approval-timed-out"] = {
        "id": "approval-timed-out",
        "sourceNodeId": "approval",
        "targetNodeId": "timed-out",
        "trigger": "timedOut",
        "priority": 0,
        "routeGroupId": "timed-out",
    }
    return workflow


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_approval_deadline_times_out_and_rejects_late_approval(
    client: ApiClient,
    funded_wallet: Wallet,
):
    workflow = approval_timeout_workflow_v2(funded_wallet.address)
    created = client.agents_teams.create(
        funded_wallet,
        name="workflow-v2-approval-timeout-acceptance",
        definition=workflow,
    ).wait_for_sync()
    team = AgentsTeam(
        id=created.first.json["response"]["id"],
        version=created.first.json["response"]["version"],
        name="workflow-v2-approval-timeout-acceptance",
        definition=workflow,
    )

    client.agents_teams.deploy(
        funded_wallet,
        agents_team=team,
        deploy=insert_signed_deploy(
            SECP256k1.generate(),
            datetime.now(UTC),
            funded_wallet,
            version=0,
        ),
    ).wait_for_sync()
    deployed = client.agents_teams.get(funded_wallet.address, team.id, team.version).json

    suspended = client.agents_teams.run(
        funded_wallet,
        "await-expiry",
        agents_team=deployed["uri"],
    ).json
    assert suspended["disposition"] == "suspended"
    assert len(suspended["waits"]) == 1

    timed_out = poll_run_until(client, suspended["runId"], {"completed", "failed", "cancelled"})
    assert timed_out["status"] == "completed", (
        f"the expired deadline should route timedOut to completion; events: "
        f"{[event['event_type'] for event in timed_out['events']]}"
    )
    event_types = [event["event_type"] for event in timed_out["events"]]
    assert "approvalRecorded" not in event_types, "a timeout is not an approval decision"
    cursor = -1
    for expected_event in ["approvalRequired", "waitSatisfied", "runResumed", "runCompleted"]:
        cursor = event_types.index(expected_event, cursor + 1)

    late = client.agents_teams.run_command(
        funded_wallet,
        suspended["runId"],
        expected_revision=timed_out["revision"],
        command_type="approve",
        wait_id=suspended["waits"][0]["id"],
        decision_id="live-decision-late",
        payload={"decision": "too-late"},
        expected_status=500,
    )
    assert "not" in late.body.lower() and "pending" in late.body.lower()
