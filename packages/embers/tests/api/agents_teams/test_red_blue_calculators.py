"""Actual Rholang decoder, task arithmetic and review ledger on the local node."""

import importlib.util
import json
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


def execute(name, value, source=None):
    source = source or (DEMO / "agents" / f"{name}.rho").read_text()
    term = "new ret, main in { " + source + ' | main!({"input":' + json.dumps(value, ensure_ascii=False) + "}, *ret) }"
    response = requests.post(
        os.environ.get("EMBERS_TEST_READ_HTTP", "http://localhost:40413") + "/api/explore-deploy",
        json={"term": term},
        timeout=120,
    )
    body = response.json()
    assert response.ok and body.get("expr"), body
    return decode(body["expr"][0])["output"]


def state(limit="100", quantity="4", amount="20", unit="USD/month", basis="estimate"):
    return {
        "brief": "Budget 100 USD/month",
        "proposal": "Deploy workers",
        "constraints": [["C", "C1", "max", "USD/month", limit, "provided", "Budget 100 USD/month"]],
        "terms": [["T", "C1", unit, quantity, amount, basis, "estimated cost"]],
        "responses": {},
        "findings": {},
        "round": 0,
        "history": [],
        "historyText": "",
        "planText": "",
        "responsesText": "",
        "reviewsText": "",
        "error": "",
    }


@pytest.mark.parametrize(
    "limit,quantity,amount,status,total,headroom",
    [
        ("100", "4", "20", "pass", 80000, 20000),
        ("60", "4", "20", "violated", 80000, -20000),
        ("100", "0", "20", "pass", 0, 100000),
        ("1.250", "2", "0.625", "pass", 1250, 0),
        ("100", "?", "20", "unknown", None, None),
    ],
)
def test_task_constraint_arithmetic(limit, quantity, amount, status, total, headroom):
    output = execute("ConstraintCheck", state(limit, quantity, amount))
    assert output["round"] == 1
    assert output["checks"][0]["status"] == status
    assert output["checks"][0].get("actualMilli") == total
    assert output["checks"][0].get("headroomMilli") == headroom
    assert output["checks"][0]["estimated"] is True
    assert "${" not in output["constraintSummary"]


@pytest.mark.parametrize("limit,status,shortfall", [("10", "pass", -2000), ("15", "violated", 3000)])
def test_minimum_constraint_uses_a_shortfall(limit, status, shortfall):
    value = state(limit=limit, quantity="3", amount="4")
    value["constraints"][0][2] = "min"
    output = execute("ConstraintCheck", value)
    assert output["checks"][0]["status"] == status
    assert output["checks"][0]["actualMilli"] == 12000
    assert output["checks"][0]["headroomMilli"] == shortfall


def test_migration_constraint_sums_all_task_estimates():
    value = state(limit="40", unit="engineer-hours")
    value["brief"] = "Migrate within 40 engineer-hours, estimating preparation at 12, migration at 8 and validation at 8."
    value["constraints"][0][3] = "engineer-hours"
    value["constraints"][0][6] = "within 40 engineer-hours"
    value["terms"] = [
        ["T", "C1", "engineer-hours", "1", amount, "estimate", phase]
        for phase, amount in [("preparation", "12"), ("migration", "8"), ("validation", "8")]
    ]
    output = execute("ConstraintCheck", value)
    assert output["checks"][0]["actualMilli"] == 28000
    assert output["checks"][0]["headroomMilli"] == 12000
    assert output["checks"][0]["status"] == "pass"
    assert output["checks"][0]["estimated"] is True


@pytest.mark.parametrize(
    "change", [{"terms": []}, {"terms": [["T", "C1", "minutes", "4", "20", "provided", "unit mismatch"]]}]
)
def test_missing_or_incompatible_terms_are_unknown(change):
    output = execute("ConstraintCheck", {**state(), **change})
    assert output["checks"][0]["status"] == "unknown"


@pytest.mark.parametrize("amount", ["-1", "1e3", "1.0001", "1000001"])
def test_invalid_numbers_are_incomplete(amount):
    assert execute("ConstraintCheck", state(amount=amount))["error"]


def review_state(findings=None, previous=None, responses=None, round=1):
    base = {
        **state(),
        "round": round,
        "unmetChecks": 0,
        "checks": [],
        "constraintSummary": "Checks pass",
        "findings": previous or {},
        "responses": responses or {},
    }
    return {
        name: {**base, "reviewFindings": findings if name == "redLogic" else {}, "reviewText": ""}
        for name in ["redLogic", "redEvidence", "redOperations"]
    }


def finding(status="open", severity="blocking"):
    return {"L1": {"id": "L1", "status": status, "severity": severity, "explanation": "Review finding"}}


def test_empty_findings_converge():
    result = execute("ReviewCoverage", review_state({}))
    assert result["outcome"] == "converged"
    assert len(result["history"]) == 1


def test_response_alone_does_not_resolve_blocker():
    response = {"L1": ["A", "L1", "CHANGE", "Changed the plan", "?", "?"]}
    result = execute("ReviewCoverage", review_state(finding(), finding(), response, 2))
    assert result["outcome"] == "revise"
    assert result["openBlockers"] == 1


def test_originating_review_confirms_resolution():
    response = {"L1": ["A", "L1", "CHANGE", "Changed the plan", "?", "?"]}
    result = execute("ReviewCoverage", review_state(finding("resolved"), finding(), response, 2))
    assert result["outcome"] == "converged"
    assert result["addressed"] == 1


@pytest.mark.parametrize("current", [{}, finding("resolved"), finding(severity="nonblocking")])
def test_omission_or_unsubstantiated_resolution_is_incomplete(current):
    result = execute("ReviewCoverage", review_state(current, finding(), {}, 2))
    assert result["outcome"] == "incomplete"
    assert result["findings"]["L1"]["status"] == "open"


def test_documented_nonblocking_risk_is_preserved():
    response = {"L1": ["A", "L1", "ACCEPT_RISK", "Accept during pilot", "SRE lead", "Review after seven days"]}
    result = execute(
        "ReviewCoverage", review_state(finding(severity="nonblocking"), finding(severity="nonblocking"), response, 2)
    )
    assert result["outcome"] == "converged"
    assert "SRE lead" in result["acceptedRisks"]


def test_mismatched_round_cannot_join():
    value = review_state({})
    value["redEvidence"]["round"] = 2
    assert execute("ReviewCoverage", value)["outcome"] == "incomplete"


@pytest.mark.parametrize(
    "name", ["BluePlanner", "BlueResponder", "RedLogic", "RedEvidence", "RedOperations", "DecisionWriter"]
)
def test_authoritative_model_wrapper_parses_and_returns(name):
    fixtures = json.loads((DEMO / "fixtures/deterministic.json").read_text())
    value = (
        fixtures["cases"][0]["brief"]
        if name == "BluePlanner"
        else {**state(), "round": 1, "unmetChecks": 0, "constraintSummary": "Pass", "outcome": "converged"}
    )
    output = execute(name, value, fixtures["agents"][name])
    if name == "DecisionWriter":
        assert "Rounds: 1/3" in output
    else:
        assert output["error"] == ""


def test_sub_thousandth_product_cannot_round_into_a_pass():
    assert execute("ConstraintCheck", state(limit="0", quantity="0.001", amount="0.001"))["error"]


def test_response_to_unknown_finding_is_incomplete():
    result = execute(
        "ReviewCoverage", review_state({}, {}, {"L9": ["A", "L9", "CHANGE", "No corresponding finding", "?", "?"]})
    )
    assert result["outcome"] == "incomplete"


def test_blocker_cannot_be_accepted_even_if_model_calls_it_resolved():
    responses = {"L1": ["A", "L1", "ACCEPT_RISK", "Accept", "Owner", "Tomorrow"]}
    result = execute("ReviewCoverage", review_state(finding("resolved"), finding(), responses, 2))
    assert result["outcome"] == "incomplete"


def test_sources_runbook_and_fixtures_do_not_drift():
    spec = importlib.util.spec_from_file_location("rbfixtures", DEMO / "fixtures/build.py")
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    assert json.loads((DEMO / "fixtures/deterministic.json").read_text()) == builder.fixtures()
    runbook = (DEMO.parents[1] / "docs/agent-teams/usage/red-blue-demo-runbook.md").read_text()
    for name, code in builder.module.sources().items():
        assert (DEMO / "agents" / f"{name}.rho").read_text() == code
        assert code in runbook


def test_reviewer_record_padding_is_not_a_malformed_review():
    spec = importlib.util.spec_from_file_location("rbfixtures_padding", DEMO / "fixtures/build.py")
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    source = builder.mock_source(
        builder.module.sources()["RedLogic"], 'out!("  RB1; F|L1|blocking|open|Check rollback; END;  ")'
    )
    output = execute("RedLogic", {**state(), "constraintSummary": "Checks pass"}, source)
    assert output["error"] == ""
    assert output["reviewFindings"]["L1"]["status"] == "open"


@pytest.mark.parametrize("limit", ["100", "200"])
def test_responder_cannot_replace_or_echo_original_constraint_records(limit):
    spec = importlib.util.spec_from_file_location("rbfixtures_constraint_revision", DEMO / "fixtures/build.py")
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    answer = f"RB1;P|Revised pilot;C|C1|max|USD/month|{limit}|provided|Budget 100 USD/month;END;"
    source = builder.mock_source(builder.module.sources()["BlueResponder"], f"out!({json.dumps(answer)})")
    original = state()
    output = execute("BlueResponder", {**original, "constraintSummary": "Checks pass"}, source)
    assert output["error"] == "Invalid, duplicate or revised constraint record"
    assert output["constraints"] == original["constraints"]


@pytest.mark.parametrize("name,prefix", [("RedLogic", "L"), ("RedEvidence", "E"), ("RedOperations", "O")])
def test_reviewer_prompt_uses_own_history_without_discarding_shared_ledger(name, prefix):
    spec = importlib.util.spec_from_file_location("rbfixtures_review_scope", DEMO / "fixtures/build.py")
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    own = prefix + "1"
    other = ("E" if prefix != "E" else "L") + "1"
    value = {
        **state(), "round": 2, "constraintSummary": "Checks pass",
        "reviewsText": "foreign-review-transcript",
        "responsesText": "foreign-response-transcript",
        "findings": {
            key: {"id": key, "severity": "blocking", "status": "open", "explanation": text}
            for key, text in [(own, "own-finding-evidence"), (other, "foreign-finding-evidence")]
        },
        "responses": {
            key: ["A", key, "CHANGE", text, "Owner", "Before launch"]
            for key, text in [(own, "own-response-evidence"), (other, "foreign-response-evidence")]
        },
    }
    source = builder.mock_source(builder.module.sources()[name], "out!(prompt)")
    output = execute(name, value, source)
    prompt = output["reviewText"]
    assert "own-finding-evidence" in prompt
    assert "own-response-evidence" in prompt
    assert "foreign-finding-evidence" not in prompt
    assert "foreign-response-evidence" not in prompt
    assert "foreign-review-transcript" not in prompt
    assert "foreign-response-transcript" not in prompt
    assert output["findings"] == value["findings"]
    assert output["responses"] == value["responses"]


@pytest.mark.parametrize("name", ["BluePlanner", "BlueResponder", "RedLogic", "RedEvidence", "RedOperations"])
@pytest.mark.parametrize("mode", ["valid", "repaired", "invalid"])
def test_model_record_correction_is_bounded_and_restarts_validation(name, mode):
    spec = importlib.util.spec_from_file_location("rbfixtures_format_repair", DEMO / "fixtures/build.py")
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    prefix = {"RedLogic": "L", "RedEvidence": "E", "RedOperations": "O"}.get(name)
    valid = (
        f"RB1;F|{prefix}1|blocking|open|Check rollback;END;"
        if prefix
        else "RB1;P|Use the revised pilot with rollback;T|C1|USD/month|4|20|estimate|Cost per worker;END;"
    )
    if name == "BluePlanner":
        valid = valid.replace("RB1;", "RB1;C|C1|max|USD/month|100|provided|Budget 100 USD/month;")
    # A partially parsed first attempt must not pollute the corrected output.
    invalid = (
        f"RB1;F|{prefix}2|blocking|open|Discard this partial finding;invalid;END;"
        if prefix
        else "RB1;P|Discard this partial proposal;invalid;END;"
    )
    suffix = "Return only the corrected RB1 record."
    if mode == "repaired":
        body = (
            f'if (prompt.slice(prompt.length() - {len(suffix)}, prompt.length()) == {json.dumps(suffix)}) '
            f'{{ out!({json.dumps(valid)}) }} else {{ out!({json.dumps(invalid)}) }}'
        )
    else:
        body = f"out!({json.dumps(valid if mode == 'valid' else invalid)})"
    source = builder.mock_source(builder.module.sources()[name], body)
    value = "Budget 100 USD/month" if name == "BluePlanner" else {**state(), "constraintSummary": "Checks pass"}
    output = execute(name, value, source)
    assert output["modelFormatAttempts"] == (1 if mode == "valid" else 2)
    if mode == "invalid":
        assert output["error"]
    else:
        assert output["error"] == ""
        if prefix:
            assert set(output["reviewFindings"]) == {prefix + "1"}
            assert output["reviewText"] == valid
        else:
            assert output["proposal"] == "Use the revised pilot with rollback"
            assert len(output["terms"]) == 1
    if name == "BlueResponder":
        assert output["constraints"] == value["constraints"]
        assert output["round"] == value["round"]


@pytest.mark.parametrize("explanation", ["The owner’s rollback isn’t tested", "Vérifier le coût estimé", "確認回復手順 🙂"])
def test_model_records_preserve_unicode_prose_without_panicking(explanation):
    spec = importlib.util.spec_from_file_location("rbfixtures_unicode", DEMO / "fixtures/build.py")
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    answer = f"RB1;F|L1|blocking|open|{explanation};END;"
    source = builder.mock_source(builder.module.sources()["RedLogic"], f"out!({json.dumps(answer, ensure_ascii=False)})")
    output = execute("RedLogic", {**state(), "round": 1, "constraintSummary": "Checks pass"}, source)
    assert output["error"] == ""
    assert output["modelFormatAttempts"] == 1
    assert output["reviewFindings"]["L1"]["explanation"] == explanation


def test_non_ascii_decimal_is_rejected_without_an_interpreter_panic():
    assert execute("ConstraintCheck", state(amount="２０"))["error"] == "Invalid numeric records: Invalid decimal digit or range"


def repaired_wrapper(name, first, corrected):
    spec = importlib.util.spec_from_file_location("rb_boundary_repair", DEMO / "fixtures/build.py")
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    suffix = "Return only the corrected RB1 record."
    body = (
        f'if (prompt.slice(prompt.length() - {len(suffix)}, prompt.length()) == {json.dumps(suffix)}) '
        f'{{ out!({json.dumps(corrected)}) }} else {{ out!({json.dumps(first)}) }}'
    )
    return builder.mock_source(builder.module.sources()[name], body)


@pytest.mark.parametrize("name,prefix", [("RedLogic", "L"), ("RedEvidence", "E"), ("RedOperations", "O")])
@pytest.mark.parametrize("failure", ["omitted", "severity", "unrepaired"])
def test_reviewer_ledger_errors_use_the_existing_bounded_correction(name, prefix, failure):
    key = prefix + "4"
    prior = {key: {"id": key, "severity": "nonblocking", "status": "open", "explanation": "Missing rollout owner"}}
    value = {**state(), "round": 2, "constraintSummary": "Checks pass", "findings": prior,
             "responses": {key: ["A", key, "CHANGE", "Assign service owner to rollout", "Service owner", "Before rollout"]}}
    corrected = f"RB1;F|{key}|nonblocking|resolved|The revised proposal assigns a rollout owner;END;"
    first = (f"RB1;F|{key}|resolved|resolved|The revised proposal assigns a rollout owner;END;"
             if failure == "severity" else "RB1;END;")
    source = repaired_wrapper(name, first, first if failure == "unrepaired" else corrected)
    output = execute(name, value, source)
    assert output["modelFormatAttempts"] == 2
    assert output["findings"] == prior
    if failure == "unrepaired":
        assert f"omitted prior finding {key}" in output["error"]
    else:
        assert output["error"] == ""
        assert output["reviewFindings"][key]["severity"] == "nonblocking"
        assert output["reviewFindings"][key]["status"] == "resolved"


def test_missing_blue_response_is_corrected_before_the_next_round():
    value = {**state(), "findings": finding(), "constraintSummary": "Checks pass"}
    first = "RB1;P|Add a rollback checkpoint;T|C1|USD/month|4|20|estimate|Cost per worker;END;"
    corrected = first.replace("END;", "A|L1|CHANGE|Add an owned rollback checkpoint|Service owner|Before rollout;END;")
    output = execute("BlueResponder", value, repaired_wrapper("BlueResponder", first, corrected))
    assert output["error"] == ""
    assert output["modelFormatAttempts"] == 2
    assert output["responses"]["L1"][2] == "CHANGE"
    assert output["constraints"] == value["constraints"]


@pytest.mark.parametrize("name", ["BluePlanner", "BlueResponder"])
def test_interval_unit_mismatch_is_corrected_without_inventing_measurements(name):
    value = state(limit="240", unit="reviewer-minutes/day")
    value["constraintSummary"] = "C1: unknown because the proposal unit omitted the daily interval"
    value["constraints"][0][3] = "reviewer-minutes/day"
    value["brief"] = "Six daily requests need two independent thirty-minute reviews, within 240 reviewer-minutes/day."
    ledger = "C|C1|max|reviewer-minutes/day|240|provided|within 240 reviewer-minutes/day;" if name == "BluePlanner" else ""
    first = "RB1;P|Use two independent reviews;" + ledger + (
        "T|C1|reviewer-minutes|6|30|estimate|six daily requests and estimated thirty minutes per review;" * 2
    ) + "END;"
    corrected = first.replace("T|C1|reviewer-minutes|", "T|C1|reviewer-minutes/day|")
    output = execute(name, value["brief"] if name == "BluePlanner" else value,
                     repaired_wrapper(name, first, corrected))
    assert output["error"] == ""
    assert output["modelFormatAttempts"] == 2
    check = execute("ConstraintCheck", output)["checks"][0]
    assert check["actualMilli"] == 360000
    assert check["headroomMilli"] == -120000
    assert check["status"] == "violated"
    assert check["estimated"] is True
    output["terms"] = output["terms"][:1]
    revised = execute("ConstraintCheck", output)["checks"][0]
    assert revised["actualMilli"] == 180000
    assert revised["headroomMilli"] == 60000
    assert revised["status"] == "pass"
