"""Execute the authoritative calculator sources on the node, including edited assumptions."""

import os
from pathlib import Path

import pytest
import requests

DEMO = Path(__file__).resolve().parents[5].parent / "embers-frontend/demo/red-blue"


def decode(value):
    if isinstance(value, dict) and len(value) == 1 and next(iter(value)).startswith("Expr"):
        return decode(next(iter(value.values()))["data"])
    if isinstance(value, dict):
        return {key: decode(item) for key, item in value.items()}
    if isinstance(value, list):
        return [decode(item) for item in value]
    return value


def execute(name, replacement=None):
    source = (DEMO / "agents" / f"{name}.rho").read_text()
    if replacement:
        assert replacement[0] in source
        source = source.replace(*replacement)
    response = requests.post(
        os.environ.get("EMBERS_TEST_READ_HTTP", "http://localhost:40413") + "/api/explore-deploy",
        json={"term": 'new ret, main in { ' + source + ' | main!("scenario", *ret) }'}, timeout=60,
    )
    response.raise_for_status()
    body = response.json()
    assert body.get("expr"), body
    return decode(body["expr"][0])


@pytest.mark.parametrize("replacement,capacity,two_demand,two_percent,two_max,one_percent,one_max", [
    (None, 240, 360, 150, 4, 75, 8),
    (("calculate!(5, 1, 60, 6, 30", "calculate!(7, 2, 90, 5, 30"), 450, 300, 66, 7, 33, 15),
])
def test_review_capacity_arithmetic(replacement, capacity, two_demand, two_percent, two_max, one_percent, one_max):
    result = execute("ReviewCapacity", replacement)
    assert result["capacityMinutesPerDay"] == capacity
    assert result["proposed"]["demandMinutesPerDay"] == two_demand
    assert result["proposed"]["utilizationPercent"] == two_percent
    assert result["proposed"]["maxPullRequestsPerDay"] == two_max
    assert result["alternative"]["utilizationPercent"] == one_percent
    assert result["alternative"]["maxPullRequestsPerDay"] == one_max
    assert "${" not in result["summary"]


@pytest.mark.parametrize("reviewers,minutes,deadline,pairs,two_deficit,two_feasible,one_feasible", [
    (1, 10, 15, 0, 1, False, True),
    (3, 10, 15, 3, 0, True, True),
    (3, 20, 15, 3, 0, False, False),
])
def test_emergency_reviewer_combinations_and_deadline(reviewers, minutes, deadline, pairs, two_deficit, two_feasible, one_feasible):
    result = execute("EmergencyReviewPolicy", ("calculate!(1, 10, 15", f"calculate!({reviewers}, {minutes}, {deadline}"))
    assert result["proposed"]["distinctReviewerCombinations"] == pairs
    assert result["proposed"]["reviewerDeficit"] == two_deficit
    assert result["proposed"]["feasible"] is two_feasible
    assert result["alternative"]["feasible"] is one_feasible
    assert "${" not in result["summary"]


def test_runbook_and_deterministic_fixture_use_authoritative_calculators():
    import json
    runbook = (DEMO.parents[1] / "docs/agent-teams/usage/red-blue-demo-runbook.md").read_text()
    fixtures = json.loads((DEMO / "fixtures/deterministic.json").read_text())
    for source_path in (DEMO / "agents").glob("*.rho"):
        source = source_path.read_text()
        assert source in runbook, f"Runbook source drift: {source_path.name}"
        assert "getOrElse(\"Input\"" not in source
        assert "getOrElse(\"input-node\"" not in source
    for name in ("ReviewCapacity", "EmergencyReviewPolicy"):
        assert fixtures["agents"][name] == (DEMO / "agents" / f"{name}.rho").read_text()


def test_decision_writer_keeps_calculator_evidence_when_model_omits_it():
    source = (DEMO / "agents/DecisionWriter.rho").read_text()
    # Replace only the model boundary; run the actual writer composition on-chain.
    source = source.replace("gpt4(`rho:ai:gpt4`)", "gpt4")
    source = source.replace("answerCh in {", 'answerCh in { contract gpt4(@prompt, answer) = { answer!("Model summary") } |')
    term = 'new ret, main in { ' + source + ' | main!({"input":"brief","blueResponder":"reviewed plan","reviewCapacity":{"summary":"Capacity: 240 minutes"},"emergencyReviewPolicy":{"summary":"Two reviewers unavailable"}}, *ret) }'
    response = requests.post(os.environ.get("EMBERS_TEST_READ_HTTP", "http://localhost:40413") + "/api/explore-deploy", json={"term": term}, timeout=60)
    response.raise_for_status()
    result = decode(response.json()["expr"][0])
    assert result == "Model summary Calculator evidence (editable scenario assumptions): Capacity: 240 minutes Two reviewers unavailable"
