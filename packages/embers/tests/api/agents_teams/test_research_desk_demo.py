"""Acceptance for the Research Desk customer demo.

The team under test is the demo shipped in ``embers-frontend/demo/research-desk``:

    Question ──▶ Draft (Researcher) ──▶ Critique (Critic) ──▶ Decide (firstMatch)
                     ▲                                           │ accept: score ≥ 80 ──▶ Answer
                     └───── revise (bounded, maxIterations 3) ───┤
                                                                 │ iterationLimit ──▶ Approval gate
                                                                 │    ├─ completed ──▶ Archive (Archivist) ──▶ Answer
                                                                 │    └─ rejected ──▶ Refused

Policy: tests never make network calls outside localhost. The local chain,
the embers backend, the ChromaDB server on localhost:8000, and the node's
in-process SBERT embedder are all fair game; the OpenAI-backed ``rho:ai:gpt4``
channel is not exercised by any test -- model-touching rehearsal is a
deliberate operator action described in the runbook, never a pytest. The real
Archivist agent (read from the demo assets) runs in BOTH tests: its only
possible side effect is an upsert into the localhost Chroma, and the
choreography test's legs all short-circuit before that branch (the sentinel
and non-map payloads never reach the upsert), so the hermetic test stays
Chroma-free even with the real agent wired in.

``test_research_desk_demo_choreography``: canned Researcher/Critic plus the
real Archivist drive the demo team's full routing choreography
deterministically: structured-v1 compilation (the quoted node-id rendering),
per-iteration effect keys in the durable orchestrator, accept-at-threshold
(bypassing the Archivist by topology), the bounded revise cycle exhausting
into iterationLimit, the approval wait, a null-payload approve releasing the
preserved value with ``archived: false``, and a non-record approve payload
passing through the Archivist byte-identical.

``test_research_desk_demo_local_rag``: the embers ⇄ f1r3node ⇄ ChromaDB
integration end to end, all on localhost: the corpus is seeded on chain
(node-side SBERT embedding into the local Chroma), and the team's Researcher
retrieves it back through ``rho:chroma:collection:entries:query`` inside its
activity deploy, folding the retrieved documents into the answer itself --
so the accepted answer literally containing corpus text proves retrieval
flowed through the whole pipeline. A second, knowledge-loop team then proves
the Archivist end to end: an unanswerable question escalates, a human record
replaces the value at the gate (``archived: true`` in the approve response),
and RE-ASKING the same question retrieves the archived Q&A document back
through node-side SBERT -- the re-ask IS the upsert verification.
"""

import json
import uuid
from datetime import UTC, datetime
from pathlib import Path

import pytest

from tests.client import AgentsTeam, ApiClient, Wallet
from tests.conftest import insert_signed_deploy
from tests.key import SECP256k1

DEMO_DIR = Path(__file__).resolve().parents[5].parent / "embers-frontend" / "demo" / "research-desk"

IN_CORPUS_QUESTION = "What is the progress boundary rule for cycles?"
# Unanswerable from BOTH the corpus and model pretraining (a fictional
# platform internal): generally known questions leak confident answers past
# any rubric under revise pressure.
TRAP_QUESTION = "What is the exact rotation schedule of the Zephyr-9 attestation committee?"
RUN_TIMEOUT = 600

# Canned bodies for the hermetic choreography test. They mirror the real
# agents' record protocol exactly -- including the INSUFFICIENT CONTEXT
# sentinel and the Critic's mechanical short-circuit -- without any system
# process besides the return channel, so no network of any kind is touched.
HERMETIC_RESEARCHER = (
    'contract main(@invocation, return) = {\n'
    '  match invocation.getOrElse("input", Nil) {\n'
    '    question /\\ String => {\n'
    f'      if (question == "{IN_CORPUS_QUESTION}") {{\n'
    '        return!({"output": {"question": question, "answer": "Every hot cycle must cross a wait, a bounded transition, or a durable yield."}, "disposition": "completed"})\n'
    '      } else {\n'
    '        return!({"output": {"question": question, "answer": "INSUFFICIENT CONTEXT"}, "disposition": "completed"})\n'
    '      }\n'
    '    }\n'
    '    revision => {\n'
    '      return!({"output": {"question": revision.getOrElse("question", ""), "answer": "INSUFFICIENT CONTEXT"}, "disposition": "completed"})\n'
    '    }\n'
    '  }\n'
    '}'
)

HERMETIC_CRITIC = (
    'contract main(@invocation, return) = {\n'
    '  match invocation.getOrElse("input", {}) {\n'
    '    draft => {\n'
    '      match (draft.getOrElse("question", ""), draft.getOrElse("answer", "")) {\n'
    '        (question, answer) => {\n'
    '          if (answer.length() >= 20 and answer.slice(0, 20) == "INSUFFICIENT CONTEXT") {\n'
    '            return!({"output": {"question": question, "answer": answer, "feedback": "POOR - no grounding", "score": 5}, "disposition": "completed"})\n'
    '          } else {\n'
    '            return!({"output": {"question": question, "answer": answer, "feedback": "GOOD - grounded", "score": 85}, "disposition": "completed"})\n'
    '          }\n'
    '        }\n'
    '      }\n'
    '    }\n'
    '  }\n'
    '}'
)


# Localhost-RAG researcher: real retrieval, no model. It queries the demo
# collection through the chroma system channel and folds the retrieved
# document texts into the answer (this Rholang has no general toString, hence
# the recursive fold over map.toList()). Grounding is therefore literal: the
# answer contains the corpus text that retrieval returned.
RETRIEVAL_RESEARCHER = (
    'contract main(@invocation, return) = {\n'
    '  new chromaQuery(`rho:chroma:collection:entries:query`),\n'
    '      buildContext, resultsCh, contextCh in {\n'
    '    contract buildContext(@entries, @index, @acc, ret) = {\n'
    '      if (index < entries.length()) {\n'
    '        match entries.nth(index) {\n'
    '          (_, (text, _)) => { buildContext!(entries, index + 1, acc ++ " " ++ text, *ret) }\n'
    '          _ => { buildContext!(entries, index + 1, acc, *ret) }\n'
    '        }\n'
    '      } else { ret!(acc) }\n'
    '    } |\n'
    '    match invocation.getOrElse("input", Nil) {\n'
    '      question /\\ String => {\n'
    '        chromaQuery!("embers-demo-docs", [question], *resultsCh) |\n'
    '        for (@results <- resultsCh) {\n'
    '          match results.nth(0) {\n'
    '            resultMap => { buildContext!(resultMap.toList(), 0, "", *contextCh) }\n'
    '          }\n'
    '        } |\n'
    '        for (@context <- contextCh) {\n'
    '          if (context.length() >= 20) {\n'
    '            return!({"output": {"question": question, "answer": "GROUNDED:" ++ context}, "disposition": "completed"})\n'
    '          } else {\n'
    '            return!({"output": {"question": question, "answer": "INSUFFICIENT CONTEXT"}, "disposition": "completed"})\n'
    '          }\n'
    '        }\n'
    '      }\n'
    '      revision => {\n'
    '        return!({"output": {"question": revision.getOrElse("question", ""), "answer": "INSUFFICIENT CONTEXT"}, "disposition": "completed"})\n'
    '      }\n'
    '    }\n'
    '  }\n'
    '}'
)


# Human-authored answer for the knowledge-loop leg (integers only in the
# record: floats are not portable across the activity boundary).
HUMAN_ANSWER = (
    "The Zephyr-9 attestation committee rotates every 400 blocks by "
    "stake-weighted round robin."
)

# Knowledge-loop researcher: real retrieval plus a mechanical discriminator
# for ARCHIVED answers specifically. Only the Archivist writes documents with
# the exact prefix "Q: " ++ question, so finding one proves the human answer
# from the escalation gate landed in Chroma and came back through node-side
# SBERT similarity. Everything else -- including the 13 corpus documents the
# query also returns -- yields the sentinel, which escalates.
KNOWLEDGE_LOOP_RESEARCHER = (
    'contract main(@invocation, return) = {\n'
    '  new chromaQuery(`rho:chroma:collection:entries:query`),\n'
    '      findQa, resultsCh, answerCh in {\n'
    '    contract findQa(@entries, @index, @needle, ret) = {\n'
    '      if (index < entries.length()) {\n'
    '        match entries.nth(index) {\n'
    '          (_, (text, _)) => {\n'
    '            if (text.length() >= needle.length() and text.slice(0, needle.length()) == needle) {\n'
    '              ret!(text.slice(needle.length(), text.length()))\n'
    '            } else { findQa!(entries, index + 1, needle, *ret) }\n'
    '          }\n'
    '          _ => { findQa!(entries, index + 1, needle, *ret) }\n'
    '        }\n'
    '      } else { ret!("") }\n'
    '    } |\n'
    '    match invocation.getOrElse("input", Nil) {\n'
    '      question /\\ String => {\n'
    '        chromaQuery!("embers-demo-docs", [question], *resultsCh) |\n'
    '        for (@results <- resultsCh) {\n'
    '          match results.nth(0) {\n'
    '            resultMap => { findQa!(resultMap.toList(), 0, "Q: " ++ question, *answerCh) }\n'
    '          }\n'
    '        } |\n'
    '        for (@qa <- answerCh) {\n'
    '          if (qa.length() > 0) {\n'
    '            return!({"output": {"question": question, "answer": "FROM THE ARCHIVE:" ++ qa}, "disposition": "completed"})\n'
    '          } else {\n'
    '            return!({"output": {"question": question, "answer": "INSUFFICIENT CONTEXT"}, "disposition": "completed"})\n'
    '          }\n'
    '        }\n'
    '      }\n'
    '      revision => {\n'
    '        return!({"output": {"question": revision.getOrElse("question", ""), "answer": "INSUFFICIENT CONTEXT"}, "disposition": "completed"})\n'
    '      }\n'
    '    }\n'
    '  }\n'
    '}'
)


def _archivist_code() -> str:
    return (DEMO_DIR / "agents" / "archivist.rho").read_text()


# Both cases drive the durable kernel end to end (RUN_TIMEOUT 600), so the module
# is `slow`; the RAG demo additionally needs Chroma (marked per-test below). The
# demo assets live in the sibling embers-frontend repo, hence the skip when absent.
# `devnet` is applied to every tests/api/** case by conftest.
pytestmark = [
    pytest.mark.slow,
    pytest.mark.skipif(not DEMO_DIR.is_dir(), reason=f"demo assets not found at {DEMO_DIR}"),
]


def _demo_definition(
    researcher: tuple[str, str], critic: tuple[str, str], archivist: tuple[str, str]
) -> dict:
    template = (DEMO_DIR / "team" / "research-desk.workflow.json").read_text()
    return json.loads(
        template.replace("__RESEARCHER_AGENT_ID__", researcher[0])
        .replace("__RESEARCHER_AGENT_VERSION__", researcher[1])
        .replace("__CRITIC_AGENT_ID__", critic[0])
        .replace("__CRITIC_AGENT_VERSION__", critic[1])
        .replace("__ARCHIVIST_AGENT_ID__", archivist[0])
        .replace("__ARCHIVIST_AGENT_VERSION__", archivist[1])
    )


def _create_agent(client: ApiClient, wallet: Wallet, name: str, code: str) -> tuple[str, str]:
    resp = client.agents.create(wallet, name=name, code=code, timeout=RUN_TIMEOUT).wait_for_sync()
    return resp.first.json["response"]["id"], resp.first.json["response"]["version"]


def _deploy_team(
    client: ApiClient,
    wallet: Wallet,
    researcher_code: str,
    critic_code: str,
    archivist_code: str,
    team_name: str,
) -> str:
    researcher = _create_agent(client, wallet, f"{team_name}-researcher", researcher_code)
    critic = _create_agent(client, wallet, f"{team_name}-critic", critic_code)
    archivist = _create_agent(client, wallet, f"{team_name}-archivist", archivist_code)
    definition = _demo_definition(researcher, critic, archivist)

    created = client.agents_teams.create(
        wallet, name=team_name, definition=definition, timeout=RUN_TIMEOUT
    ).wait_for_sync()
    team = AgentsTeam(
        id=created.first.json["response"]["id"],
        version=created.first.json["response"]["version"],
        name=team_name,
        definition=definition,
    )
    client.agents_teams.deploy(
        wallet,
        agents_team=team,
        deploy=insert_signed_deploy(SECP256k1.generate(), datetime.now(UTC), wallet, version=0),
        timeout=RUN_TIMEOUT,
    ).wait_for_sync()

    uri = client.agents_teams.get(wallet.address, team.id, team.version).json["uri"]
    assert uri, "deploying the demo team should record its registry uri"
    return uri


def _assert_escalation_releases_preserved_value(
    client: ApiClient, wallet: Wallet, uri: str, question: str
) -> None:
    suspended = client.agents_teams.run(
        wallet, question, agents_team=uri, timeout=RUN_TIMEOUT
    ).json
    assert isinstance(suspended, dict) and suspended.get("disposition") == "suspended", (
        f"trap question should escalate to the approval gate, got: {suspended}"
    )
    assert len(suspended["waits"]) == 1

    released = client.agents_teams.run_command(
        wallet,
        suspended["runId"],
        expected_revision=suspended["revision"],
        command_type="approve",
        wait_id=suspended["waits"][0]["id"],
        decision_id=f"research-desk-approve-{suspended['runId']}",
        payload=None,
        timeout=RUN_TIMEOUT,
    ).json
    assert released["question"] == question
    assert released["score"] < 80, "the gate releases the best-effort answer, not an accepted one"
    assert released["answer"]
    assert released["archived"] is False, "the desk must not learn its own refusal"

    completed = client.agents_teams.get_run(suspended["runId"]).json
    assert completed["status"] == "completed"
    event_types = [event["event_type"] for event in completed["events"]]
    assert "approvalRecorded" in event_types
    assert event_types.count("runSuspended") == 1


@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_research_desk_demo_choreography(client: ApiClient, funded_wallet: Wallet):
    """Hermetic: the demo team's full routing choreography, canned model agents.

    No gpt4, no Chroma calls, no corpus seeding -- nothing leaves the chain
    (the real Archivist is wired in, but every leg here short-circuits before
    its upsert branch). What it pins: structured-v1 activity compilation,
    distinct effect keys across the bounded cycle's iterations,
    accept-at-threshold bypassing the Archivist by topology, three revise
    traversals exhausting into iterationLimit, the approval wait, a
    null-payload approve releasing the preserved value with archived false,
    and a non-record approve payload passing through the Archivist
    byte-identical.
    """
    uri = _deploy_team(
        client,
        funded_wallet,
        HERMETIC_RESEARCHER,
        HERMETIC_CRITIC,
        _archivist_code(),
        "research-desk-choreography",
    )

    accepted = client.agents_teams.run(
        funded_wallet, IN_CORPUS_QUESTION, agents_team=uri, timeout=RUN_TIMEOUT
    ).json
    assert accepted["question"] == IN_CORPUS_QUESTION
    assert accepted["score"] >= 80, f"grounded answer should be accepted, got: {accepted}"
    assert "hot cycle" in accepted["answer"]
    assert "archived" not in accepted, (
        "the accept path must bypass the Archivist by topology"
    )

    _assert_escalation_releases_preserved_value(
        client, funded_wallet, uri, TRAP_QUESTION
    )

    # A non-record approve payload (here a JSON string) replaces the released
    # value and must pass through the Archivist byte-identical: it is not the
    # {question, answer} shape, so nothing is filed and no flag is added --
    # malformed human input can neither poison retrieval nor error the deploy.
    suspended = client.agents_teams.run(
        funded_wallet, TRAP_QUESTION, agents_team=uri, timeout=RUN_TIMEOUT
    ).json
    assert isinstance(suspended, dict) and suspended.get("disposition") == "suspended"
    operator_note = "operator note, wrong shape on purpose"
    passed_through = client.agents_teams.run_command(
        funded_wallet,
        suspended["runId"],
        expected_revision=suspended["revision"],
        command_type="approve",
        wait_id=suspended["waits"][0]["id"],
        decision_id=f"research-desk-malformed-{suspended['runId']}",
        payload=operator_note,
        timeout=RUN_TIMEOUT,
    ).json
    assert passed_through == operator_note
    assert client.agents_teams.get_run(suspended["runId"]).json["status"] == "completed"


@pytest.mark.chroma
@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_research_desk_demo_local_rag(client: ApiClient, funded_wallet: Wallet):
    """embers ⇄ f1r3node ⇄ ChromaDB end to end, entirely on localhost."""
    collection = f"embers-test-research-desk-{uuid.uuid4().hex}"

    def isolate(code: str) -> str:
        return code.replace("embers-demo-docs", collection)

    seeded = client.testnet.deploy(
        funded_wallet,
        isolate((DEMO_DIR / "seed" / "seed-corpus.rho").read_text()),
        timeout=RUN_TIMEOUT,
    ).json
    assert seeded["type"] == "Ok", seeded
    assert any("EMBERS-DEMO-SEED-OK" in log["message"] for log in seeded["logs"]), seeded["logs"]

    uri = _deploy_team(
        client,
        funded_wallet,
        isolate(RETRIEVAL_RESEARCHER),
        HERMETIC_CRITIC,
        isolate(_archivist_code()),
        "research-desk-local-rag",
    )

    accepted = client.agents_teams.run(
        funded_wallet, IN_CORPUS_QUESTION, agents_team=uri, timeout=RUN_TIMEOUT
    ).json
    assert accepted["question"] == IN_CORPUS_QUESTION
    assert accepted["score"] >= 80, f"retrieval-grounded answer should be accepted, got: {accepted}"
    assert accepted["answer"].startswith("GROUNDED:")
    # The fold put the retrieved corpus documents into the answer verbatim, so
    # finding the progress-boundary sentence proves the upsert (node-side
    # embedding into local Chroma) and the in-activity similarity query both
    # worked through the full team pipeline.
    assert "progress boundary" in accepted["answer"]
    assert "archived" not in accepted, (
        "the accept path must bypass the Archivist by topology"
    )

    # Keep test answers in a separate collection. Similarity search can expose
    # a fictional answer to live agents even when its question is unique.
    loop_uri = _deploy_team(
        client,
        funded_wallet,
        isolate(KNOWLEDGE_LOOP_RESEARCHER),
        HERMETIC_CRITIC,
        isolate(_archivist_code()),
        "research-desk-knowledge-loop",
    )
    question = (
        "What is the exact rotation schedule of the "
        f"{uuid.uuid4().hex[:8]} attestation committee?"
    )

    suspended = client.agents_teams.run(
        funded_wallet, question, agents_team=loop_uri, timeout=RUN_TIMEOUT
    ).json
    assert isinstance(suspended, dict) and suspended.get("disposition") == "suspended", (
        f"the unanswerable question should escalate to the gate, got: {suspended}"
    )

    # The human answers THROUGH the gate: a non-null approve payload replaces
    # the released value, and the Archivist files it (integers only -- a float
    # anywhere in this record would fail the run before any deploy).
    human_record = {
        "question": question,
        "answer": HUMAN_ANSWER,
        "feedback": "human-authored at the escalation gate",
        "score": 100,
    }
    archived = client.agents_teams.run_command(
        funded_wallet,
        suspended["runId"],
        expected_revision=suspended["revision"],
        command_type="approve",
        wait_id=suspended["waits"][0]["id"],
        decision_id=f"research-desk-knowledge-{suspended['runId']}",
        payload=human_record,
        timeout=RUN_TIMEOUT,
    ).json
    assert archived["question"] == question
    assert archived["answer"] == HUMAN_ANSWER
    assert archived["feedback"] == human_record["feedback"]
    assert archived["score"] == 100
    assert archived["archived"] is True, (
        f"the human answer should be filed into the corpus, got: {archived}"
    )
    first_run = client.agents_teams.get_run(suspended["runId"]).json
    assert first_run["status"] == "completed"
    assert [e["event_type"] for e in first_run["events"]].count("runSuspended") == 1

    # Re-asking the same question is the upsert verification: retrieval must
    # now surface the archived Q&A document (round-tripped through node-side
    # SBERT), and the desk answers without escalating.
    relearned = client.agents_teams.run(
        funded_wallet, question, agents_team=loop_uri, timeout=RUN_TIMEOUT
    ).json
    assert relearned["score"] >= 80, (
        f"the archived answer should be retrievable and accepted, got: {relearned}"
    )
    assert relearned["answer"].startswith("FROM THE ARCHIVE:")
    assert HUMAN_ANSWER in relearned["answer"]
    assert "archived" not in relearned, (
        "the machine-accepted re-ask must bypass the Archivist by topology"
    )
