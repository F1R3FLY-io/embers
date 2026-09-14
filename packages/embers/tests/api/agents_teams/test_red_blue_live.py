"""Opt-in real-model deployed rehearsal across three engineering domains."""

import os
import json

import pytest

from tests.api.agents_teams.test_red_blue_demo import DEMO, graph_module, deploy_demo
from tests.api.agents_teams.test_reveal_output import _run

pytestmark = [
    pytest.mark.slow,
    pytest.mark.skipif(
        os.environ.get("EMBERS_RED_BLUE_LIVE") != "1", reason="Set EMBERS_RED_BLUE_LIVE=1 to call the configured model"
    ),
]
BRIEFS = [
    (
        "Recommend a pilot service deployment within a budget of 100 USD/month. Four workers at an estimated 20 USD/month each are sufficient. Include ownership, monitoring and rollback; distinguish estimates from facts.",
        "100000/1000",
    ),
    (
        "Recommend a code-review policy within 240 reviewer-minutes/day. We expect 6 PR/day and estimate 30 minutes per independent review. The current proposal requires two approvals per PR. Consider one approval with CI checks, an emergency path, rollout and rollback.",
        "240000/1000",
    ),
    (
        "Plan a database migration within a 40 engineer-hour budget. We estimate preparation at 12 hours, migration at 8 and validation at 8. Include a tested backup, rollback checkpoint and a named owning role. Data size and measured restore time are unknown, so make rollout conditional on verification.",
        "40000/1000",
    ),
]


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
@pytest.mark.parametrize("review_round", [1, 2])
def test_real_reviewer_record_protocol(client, funded_wallet, review_round):
    """Exercise actual model boundaries without durable checkpoint overhead."""
    from tests.api.agents_teams.test_red_blue_calculators import state

    value = {
        **state(),
        "round": review_round,
        "constraintSummary": "C1: pass; actual=80 USD/month; max=100 USD/month; estimated=true.",
    }
    value["brief"] = BRIEFS[0][0]
    value["proposal"] = "Pilot four workers, estimated at 20 USD/month each, with monitoring and rollback."
    if review_round == 2:
        value["proposal"] = (
            "Pilot four workers at an estimated 20 USD/month each. The SRE lead owns rollout and rollback. "
            "Measure error rate and p95 latency every minute. Revert to the saved previous release if errors "
            "exceed 1 percent or p95 latency exceeds 200ms for five minutes. Test rollback in staging before launch."
        )
        for prefix in ["E", "L", "O"]:
            for number, severity, explanation in [
                (1, "nonblocking", "Pilot ownership needs a named role"),
                (2, "blocking", "Monitoring strategy has not been defined"),
                (3, "blocking", "Rollback mechanism lacks an implementation plan"),
            ]:
                identity = f"{prefix}{number}"
                value["findings"][identity] = {
                    "id": identity, "severity": severity, "status": "open", "explanation": explanation
                }
                value["responses"][identity] = [
                    "A", identity, "CHANGE", value["proposal"], "SRE lead", "Verify before launch"
                ]
        value["reviewsText"] = "RB1;" + "".join(
            f"F|{key}|{item['severity']}|open|{item['explanation']};"
            for key, item in value["findings"].items()
        ) + "END;"
        value["responsesText"] = "RB1;" + "".join(
            "|".join(record) + ";" for record in value["responses"].values()
        ) + "END;"
    names = ["RedLogic", "RedEvidence", "RedOperations"]
    sources = [
        (DEMO / "agents" / f"{name}.rho").read_text().replace("contract main(", f"contract {name}(", 1)
        for name in names
    ]
    calls = [
        f'''new ret in {{
        {name}!({{"input": {json.dumps(value)}}}, *ret) |
        for (@result <- ret) {{
            @log!("info", "{name}|" ++ result.get("output").get("error") ++ "|" ++ result.get("output").get("reviewText"))
        }}
    }}'''
        for name in names
    ]
    code = (
        "new rl(`rho:registry:lookup`), logCh, "
        + ", ".join(names)
        + " in { "
        + " | ".join(sources)
        + """ |
        rl!(`rho:id:ugzm7bbut1gxmcutdeo3minmwfd7qgxcwepeox85gdmy7xatziwrbg`, *logCh) |
        for (@(_, log) <- logCh) { """
        + " | ".join(calls)
        + " } }"
    )
    response = client.testnet.deploy(funded_wallet, code, timeout=300).json
    assert response["type"] == "Ok", response
    messages = [entry["message"] for entry in response["logs"]]
    for name, prefix in zip(names, ["L", "E", "O"]):
        message = next(message for message in messages if message.startswith(f"{name}|"))
        assert message.startswith(f"{name}||RB1;"), message
        records = {
            parts[1]: parts for record in message.split("|", 2)[2].split(";")
            if (parts := record.split("|"))[0] == "F"
        }
        if review_round == 1:
            assert all(record[3] == "open" for record in records.values()), message
        else:
            assert {f"{prefix}{number}" for number in [1, 2, 3]} <= records.keys(), message
            assert all(records[f"{prefix}{number}"][2] == "blocking" for number in [2, 3]), message


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_real_responder_record_protocol(client, funded_wallet):
    from tests.api.agents_teams.test_red_blue_calculators import state

    findings = {
        "E1": ("nonblocking", "Pilot ownership needs a named role"),
        "E2": ("blocking", "Monitoring strategy needs measurable success criteria"),
        "E3": ("nonblocking", "Rollback steps need verification"),
        "L1": ("blocking", "Rollback procedure lacks implementation details"),
        "L2": ("nonblocking", "Monitoring approach is unspecified"),
        "O1": ("blocking", "Ownership for the pilot is unassigned"),
        "O2": ("nonblocking", "Performance monitoring is unspecified"),
        "O3": ("nonblocking", "Rollback steps are undefined"),
        "O4": ("nonblocking", "Worker availability is unverified"),
    }
    value = {
        **state(),
        "brief": BRIEFS[0][0],
        "round": 1,
        "proposal": "Pilot four workers, monitor performance and roll back if needed.",
        "planText": "RB1;P|Pilot four workers;C|C1|max|USD/month|100|provided|budget 100 USD/month;T|C1|USD/month|4|20|estimate|estimated cost per worker;END;",
        "constraintSummary": "C1: pass; actual=80 USD/month; max=100 USD/month; estimated=true.",
        "reviewsText": " ".join(
            "RB1;" + "".join(
                f"F|{key}|{severity}|open|{explanation};"
                for key, (severity, explanation) in findings.items() if key.startswith(prefix)
            ) + "END;" for prefix in ["E", "L", "O"]
        ),
        "findings": {
            key: {"id": key, "severity": severity, "status": "open", "explanation": explanation}
            for key, (severity, explanation) in findings.items()
        },
    }
    source = (DEMO / "agents/BlueResponder.rho").read_text()
    code = (
        "new main, ret, rl(`rho:registry:lookup`), logCh in { " + source
        + f' | main!({{"input": {json.dumps(value)}}}, *ret) | '
        + '''rl!(`rho:id:ugzm7bbut1gxmcutdeo3minmwfd7qgxcwepeox85gdmy7xatziwrbg`, *logCh) |
        for (@result <- ret; @(_, log) <- logCh) {
            @log!("info", "BlueResponder|" ++ result.get("output").get("error") ++ "|" ++ result.get("output").get("responsesText"))
        } }'''
    )
    response = client.testnet.deploy(funded_wallet, code, timeout=300).json
    assert response["type"] == "Ok", response
    messages = [entry["message"] for entry in response["logs"]]
    message = next(message for message in messages if message.startswith("BlueResponder|"))
    assert message.startswith("BlueResponder||RB1;"), message
    records = message.split("|", 2)[2].split(";")
    assert {record.split("|")[1] for record in records if record.startswith("A|")} == set(findings), message
    assert not any(record.startswith("C|") for record in records), message


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_real_model_deployed_team_across_domains(client, funded_wallet):
    codes = {name: (DEMO / "agents" / f"{name}.rho").read_text() for name in graph_module.DEFINITION["agents"]}
    _, uri = deploy_demo(client, funded_wallet, codes)
    for brief, limit in BRIEFS:
        run_id, result = _run(client, funded_wallet, brief, uri, timeout=900)
        assert isinstance(result, str), (run_id, result)
        assert "incomplete review" not in result, (run_id, result)
        assert limit in result, (run_id, result)
        for heading in (
            "Recommendation",
            "Changes after review",
            "Remaining risks",
            "Rollout checklist",
            "Local verification:",
            "Stop reason:",
        ):
            assert heading.lower() in result.lower(), (run_id, result)
        print(f"Real-model run: {run_id}", flush=True)


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_real_model_corrects_rejected_record(client, funded_wallet):
    """Force the observed prose-semicolon failure, then call the real model for repair."""
    import importlib.util
    from tests.api.agents_teams.test_red_blue_calculators import state

    spec = importlib.util.spec_from_file_location("rbfixtures_live_repair", DEMO / "fixtures/build.py")
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    invalid = "RB1;F|L1|blocking|open|Monitoring methods are not detailed; this may affect service evaluation;END;"
    suffix = "Return only the corrected RB1 record."
    body = (
        f'if (prompt.slice(prompt.length() - {len(suffix)}, prompt.length()) == {json.dumps(suffix)}) '
        '{ new realModel(`rho:ai:gpt4`) in { realModel!(prompt, *out) } } '
        f'else {{ out!({json.dumps(invalid)}) }}'
    )
    source = builder.mock_source(builder.module.sources()["RedLogic"], body)
    value = {**state(), "round": 1, "constraintSummary": "Checks pass"}
    code = (
        "new main, ret, rl(`rho:registry:lookup`), logCh in { " + source
        + f' | main!({{"input": {json.dumps(value)}}}, *ret) | '
        + '''rl!(`rho:id:ugzm7bbut1gxmcutdeo3minmwfd7qgxcwepeox85gdmy7xatziwrbg`, *logCh) |
        for (@result <- ret; @(_, log) <- logCh) {
            @log!("info", "Attempts=${count}|" %% {"count": result.get("output").get("modelFormatAttempts")} ++ result.get("output").get("error") ++ "|" ++ result.get("output").get("reviewText"))
        } }'''
    )
    response = client.testnet.deploy(funded_wallet, code, timeout=300).json
    assert response["type"] == "Ok", response
    messages = [entry["message"] for entry in response["logs"]]
    message = next(message for message in messages if message.startswith("Attempts="))
    assert message.startswith("Attempts=2||RB1;"), message
    records = [record.split("|") for record in message.split("|", 2)[2].split(";") if record.startswith("F|")]
    assert len(records) == 1 and records[0][1:4] == ["L1", "blocking", "open"], message
    assert "monitor" in records[0][4].lower(), message
