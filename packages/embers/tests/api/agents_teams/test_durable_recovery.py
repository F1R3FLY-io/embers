"""L2: durable Agent-Team recovery across an embers restart (single-process).

Marked devnet+recovery+slow. Deploys a signal-wait workflow, runs it to a suspended
wait, then restarts ONLY the embers systemd unit (F1r3node and F1r3drive stay up) —
gracefully and, as a chaos variant, via SIGKILL — and from a FRESH client resumes the
run and asserts the exact awaited output. The run is recovered from the F1r3drive
continuation store across the restart; its revision and wait identity are unchanged.

Skips cleanly unless EMBERS_SERVICE_UNIT names an active user unit, so it never fails
outside the managed L2 dev-net. live_workflow_recovery.py is the manual two-process
counterpart (kept for interactive debugging).
"""

import os
import subprocess
import time
import urllib.request
import uuid
from datetime import UTC, datetime

import pytest

from tests.api.agents_teams.test_workflow_v2 import SIGNAL_WAIT_WORKFLOW_V2
from tests.client import AgentsTeam, ApiClient, Wallet
from tests.conftest import insert_signed_deploy
from tests.key import SECP256k1

pytestmark = [pytest.mark.devnet, pytest.mark.recovery, pytest.mark.slow]

EMBERS_SERVICE_UNIT = os.environ.get("EMBERS_SERVICE_UNIT", "")
EMBERS_URL = os.environ.get("EMBERS_API_URL", "[::1]:8080")


def _systemctl(*args: str, check: bool = False) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(["systemctl", "--user", *args], check=check)  # noqa: S603, S607 — fixed argv, no shell


def _unit_active(unit: str) -> bool:
    return bool(unit) and _systemctl("is-active", "--quiet", unit).returncode == 0


def _restart_embers(method: str) -> None:
    if method == "kill":
        _systemctl("kill", "-s", "SIGKILL", EMBERS_SERVICE_UNIT)
        _systemctl("start", EMBERS_SERVICE_UNIT)
    else:
        _systemctl("restart", EMBERS_SERVICE_UNIT, check=True)


def _wait_embers_ready(timeout: int = 120) -> None:
    target = f"http://{EMBERS_URL}/swagger-ui/openapi.json"
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(target, timeout=2) as response:
                if response.status == 200:
                    return
        except OSError:
            pass
        time.sleep(1)
    pytest.fail(f"embers did not become ready at {EMBERS_URL} within {timeout}s")


@pytest.mark.skipif(
    not _unit_active(EMBERS_SERVICE_UNIT),
    reason="set EMBERS_SERVICE_UNIT to an active embers user unit (see scripts/l2.sh) to run recovery",
)
@pytest.mark.parametrize("restart_method", ["restart", "kill"])
@pytest.mark.parametrize("funded_wallet", [100_000_000], indirect=True)
def test_durable_recovery_across_embers_restart(client: ApiClient, funded_wallet: Wallet, restart_method: str):
    # Deploy the signal-wait workflow and run it to a suspended wait.
    name = f"recovery-{restart_method}-{uuid.uuid4()}"
    created = client.agents_teams.create(funded_wallet, name=name, definition=SIGNAL_WAIT_WORKFLOW_V2).wait_for_sync()
    team = AgentsTeam(
        id=created.first.json["response"]["id"],
        version=created.first.json["response"]["version"],
        name=name,
        definition=SIGNAL_WAIT_WORKFLOW_V2,
    )
    client.agents_teams.deploy(
        funded_wallet,
        agents_team=team,
        deploy=insert_signed_deploy(SECP256k1.generate(), datetime.now(UTC), funded_wallet, version=0),
    ).wait_for_sync()
    deployed = client.agents_teams.get(funded_wallet.address, team.id, team.version).json
    suspended = client.agents_teams.run(funded_wallet, "before-restart", agents_team=deployed["uri"]).json
    assert suspended.get("disposition") == "suspended", f"run did not suspend: {suspended!r}"
    assert len(suspended.get("waits", [])) == 1, f"expected exactly one wait: {suspended!r}"
    run_id = suspended["runId"]
    revision = int(suspended["revision"])
    wait_id = suspended["waits"][0]["id"]

    # Restart ONLY embers; the node and drive keep the durable state alive.
    _restart_embers(restart_method)
    _wait_embers_ready()

    # A FRESH client recovers the run and drives the awaited signal.
    fresh = ApiClient(EMBERS_URL)
    fresh.wallets.listen_for_deploys(funded_wallet)
    before = fresh.agents_teams.get_run(run_id).json
    assert before["status"] == "suspended", f"run not recovered as suspended: {before!r}"
    assert int(before["revision"]) == revision, "recovered run revision changed across restart"

    output = fresh.agents_teams.run_command(
        funded_wallet,
        run_id,
        expected_revision=revision,
        command_type="signal",
        wait_id=wait_id,
        name="continue",
        payload="resumed-after-restart",
    ).json
    assert output == "resumed-after-restart", f"unexpected resumed output: {output!r}"
    completed = fresh.agents_teams.get_run(run_id).json
    assert completed["status"] == "completed", f"run did not complete after restart: {completed!r}"
