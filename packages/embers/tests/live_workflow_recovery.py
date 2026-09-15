"""Two-process acceptance harness for durable Agent Team restart recovery.

Run ``start`` while Embers is up, restart only the Embers process while keeping
F1r3node and F1r3drive alive, and then run ``resume`` with the same state file.
The file contains public run metadata only; the local test wallet is the same
documented development wallet used by the pytest fixtures.

For the automated, single-process pytest counterpart (graceful restart + SIGKILL
chaos, skip-guarded on EMBERS_SERVICE_UNIT), see
``tests/api/agents_teams/test_durable_recovery.py``; run it via ``scripts/l2.sh
recovery``. This harness is kept for interactive debugging across two shells.
"""

from __future__ import annotations

import argparse
import contextlib
import json
import os
import sys
import tempfile
import uuid
from datetime import UTC, datetime
from pathlib import Path

from tests.api.agents_teams.test_workflow_v2 import SIGNAL_WAIT_WORKFLOW_V2
from tests.client import AgentsTeam, ApiClient, Wallet
from tests.conftest import insert_signed_deploy
from tests.key import SECP256k1

LOCAL_TEST_WALLET_KEY = "0B4E12EC24D2F42F3FC826194750E3168A5F03071F382375C29A5E801DBBE8A5"


def local_client(backend: str) -> tuple[ApiClient, Wallet]:
    client = ApiClient(backend)
    wallet = Wallet(key=SECP256k1.from_hex(LOCAL_TEST_WALLET_KEY))
    client.wallets.listen_for_deploys(wallet)
    return client, wallet


def write_state(path: Path, state: dict[str, object]) -> None:
    path = path.resolve()
    assert path.parent.is_dir(), f"state directory does not exist: {path.parent}"
    descriptor, temporary = tempfile.mkstemp(
        dir=path.parent,
        prefix=f".{path.name}.",
        suffix=".tmp",
        text=True,
    )
    try:
        os.fchmod(descriptor, 0o600)
        with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
            json.dump(state, stream, sort_keys=True)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        Path(temporary).replace(path)
    except BaseException:
        with contextlib.suppress(FileNotFoundError):
            Path(temporary).unlink()
        raise


def start(backend: str, state_file: Path) -> dict[str, object]:
    client, wallet = local_client(backend)
    name = f"workflow-v2-restart-{uuid.uuid4()}"
    created = client.agents_teams.create(
        wallet,
        name=name,
        definition=SIGNAL_WAIT_WORKFLOW_V2,
    ).wait_for_sync()
    team = AgentsTeam(
        id=created.first.json["response"]["id"],
        version=created.first.json["response"]["version"],
        name=name,
        definition=SIGNAL_WAIT_WORKFLOW_V2,
    )
    client.agents_teams.deploy(
        wallet,
        agents_team=team,
        deploy=insert_signed_deploy(
            SECP256k1.generate(),
            datetime.now(UTC),
            wallet,
            version=0,
        ),
    ).wait_for_sync()
    deployed = client.agents_teams.get(wallet.address, team.id, team.version).json
    suspended = client.agents_teams.run(
        wallet,
        "before-restart",
        agents_team=deployed["uri"],
    ).json
    assert suspended.get("disposition") == "suspended", f"run did not suspend: {suspended!r}"
    assert len(suspended.get("waits", [])) == 1, f"run did not suspend at exactly one wait: {suspended!r}"

    wait = suspended["waits"][0]
    run = client.agents_teams.get_run(suspended["runId"]).json
    state = {
        "runId": suspended["runId"],
        "revision": int(suspended["revision"]),
        "waitId": wait["id"],
        "continuationContentHash": run["continuation_content_hash"],
        "continuationCapabilityHash": run["continuation_capability_hash"],
    }
    write_state(state_file, state)
    return state


def resume(backend: str, state_file: Path) -> dict[str, object]:
    state = json.loads(state_file.read_text(encoding="utf-8"))
    client, wallet = local_client(backend)
    before = client.agents_teams.get_run(state["runId"]).json
    assert before["status"] == "suspended", f"recovered run is not suspended: {before!r}"
    assert int(before["revision"]) == state["revision"], "recovered run revision changed across restart"
    assert before["wait_ids"] == [state["waitId"]], "recovered run wait identity changed across restart"
    assert before["continuation_content_hash"] == state["continuationContentHash"], (
        "recovered continuation content hash changed across restart"
    )
    assert before["continuation_capability_hash"] == state["continuationCapabilityHash"], (
        "recovered continuation capability changed across restart"
    )

    output = client.agents_teams.run_command(
        wallet,
        state["runId"],
        expected_revision=state["revision"],
        command_type="signal",
        wait_id=state["waitId"],
        name="continue",
        payload="resumed-after-restart",
    ).json
    assert output == "resumed-after-restart", f"unexpected resumed output: {output!r}"
    completed = client.agents_teams.get_run(state["runId"]).json
    assert completed["status"] == "completed", f"run did not complete after restart: {completed!r}"
    assert not completed["wait_ids"], f"completed run retained waits: {completed!r}"
    assert completed["output_content_hash"], "completed run has no durable output content hash"
    assert completed["output_capability_hash"], "completed run has no durable output capability"
    return {
        "runId": state["runId"],
        "status": completed["status"],
        "revision": int(completed["revision"]),
        "output": output,
        "outputContentHash": completed["output_content_hash"],
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=("start", "resume"))
    parser.add_argument("--backend", default="[::1]:8080")
    parser.add_argument("--state-file", type=Path, required=True)
    args = parser.parse_args()
    result = start(args.backend, args.state_file) if args.phase == "start" else resume(args.backend, args.state_file)
    sys.stdout.write(f"{json.dumps(result, sort_keys=True)}\n")


if __name__ == "__main__":
    main()
