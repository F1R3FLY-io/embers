#!/bin/bash
# Supervisor for the co-located F1r3drive + Embers container.
#
# Embers refuses to start unless its continuation store lives inside a FUSE
# mount whose fsname contains "f1r3drive" (verify_f1r3drive_mount in
# packages/embers/src/domain/agents_teams/continuation_store.rs), so this
# entrypoint runs both processes in one container and one mount namespace:
#
#   phase 0  wait for the node HTTP API (compose cannot order across projects)
#   phase 1  start f1r3drive (java), which mounts the FUSE filesystem
#   phase 2  wait for the fsname=f1r3drive mount and the unlocked wallet dir
#   phase 3  start embers; if either process exits, stop the other and exit
#
# The container needs devices: [/dev/fuse], cap_add: [SYS_ADMIN], and on
# AppArmor hosts security_opt: ["apparmor:unconfined"].
set -u

log() { echo "[entrypoint] $*"; }

if [[ "${F1R3DRIVE_ENABLED}" != "true" ]]; then
    log "F1R3DRIVE_ENABLED=${F1R3DRIVE_ENABLED} - starting embers without the co-located drive"
    exec /app/embers
fi

: "${F1R3DRIVE_VALIDATOR_HOST:?F1R3DRIVE_VALIDATOR_HOST is required}"
: "${F1R3DRIVE_WALLET_ADDRESS:?F1R3DRIVE_WALLET_ADDRESS is required}"
: "${F1R3DRIVE_WALLET_KEY:?F1R3DRIVE_WALLET_KEY is required}"
if [[ ! -e /dev/fuse ]]; then
    log "FATAL: /dev/fuse is missing - the embers service needs devices: [/dev/fuse], cap_add: [SYS_ADMIN] (and apparmor:unconfined on AppArmor hosts)"
    exit 70
fi

# Phase 0 - wait for the node HTTP API when a wait URL is configured. Use the
# node's compose DNS name here, never "localhost": the service may carry an
# extra_hosts remap of localhost for f1r3sky handle resolution.
if [[ -n "${NODE_HTTP_WAIT_URL:-}" ]]; then
    log "waiting for the node at ${NODE_HTTP_WAIT_URL} (max ${NODE_WAIT_TIMEOUT_SECONDS}s)"
    node_deadline=$((SECONDS + NODE_WAIT_TIMEOUT_SECONDS))
    until curl -sf -o /dev/null "${NODE_HTTP_WAIT_URL}"; do
        if ((SECONDS >= node_deadline)); then
            log "FATAL: node not reachable at ${NODE_HTTP_WAIT_URL} within ${NODE_WAIT_TIMEOUT_SECONDS}s"
            exit 71
        fi
        sleep 2
    done
    log "node is reachable"
fi

mkdir -p "${F1R3DRIVE_MOUNT}" "$(dirname "${F1R3DRIVE_KEY_FILE}")"

# Phase 1 - start f1r3drive with the argument set the native stack verified
# (embers-local-stack cmd_serve_drive). The AES cipher key file is generated
# by f1r3drive on first run; keep it on a persistent volume - losing it makes
# previously written drive content undecryptable.
drive_args=("${F1R3DRIVE_MOUNT}"
    --key-file "${F1R3DRIVE_KEY_FILE}"
    --host "${F1R3DRIVE_VALIDATOR_HOST}" --port "${F1R3DRIVE_VALIDATOR_PORT}"
    --observer-host "${F1R3DRIVE_OBSERVER_HOST:-${F1R3DRIVE_VALIDATOR_HOST}}"
    --observer-port "${F1R3DRIVE_OBSERVER_PORT}"
    --address "${F1R3DRIVE_WALLET_ADDRESS}"
    --private-key "${F1R3DRIVE_WALLET_KEY}")
if [[ "${F1R3DRIVE_MANUAL_PROPOSE}" == "true" ]]; then
    drive_args+=(--manual-propose)
fi

log "starting f1r3drive (mount ${F1R3DRIVE_MOUNT})"
java -jar "${F1R3DRIVE_JAR}" "${drive_args[@]}" &
drive_pid=$!

embers_pid=""
terminating=0
on_signal() {
    terminating=1
    if [[ -n "${embers_pid}" ]]; then kill -INT "${embers_pid}" 2>/dev/null; fi
    kill -INT "${drive_pid}" 2>/dev/null
}
trap on_signal INT TERM

# Phase 2 - wait for the FUSE mount (fsname=f1r3drive) and the unlocked
# wallet directory, mirroring the native stack's wait_drive; embers'
# verify_f1r3drive_mount re-checks the same facts from /proc/self/mountinfo.
mount_ready() {
    grep -qE "^f1r3drive ${F1R3DRIVE_MOUNT} fuse" /proc/mounts &&
        [[ -d "${F1R3DRIVE_MOUNT}/${F1R3DRIVE_WALLET_ADDRESS}" ]]
}
mount_deadline=$((SECONDS + F1R3DRIVE_START_TIMEOUT_SECONDS))
until mount_ready; do
    if ! kill -0 "${drive_pid}" 2>/dev/null; then
        log "FATAL: f1r3drive exited during startup - see its log lines above"
        exit 72
    fi
    if ((SECONDS >= mount_deadline)); then
        log "FATAL: f1r3drive did not mount and unlock ${F1R3DRIVE_MOUNT}/${F1R3DRIVE_WALLET_ADDRESS} within ${F1R3DRIVE_START_TIMEOUT_SECONDS}s"
        kill -INT "${drive_pid}" 2>/dev/null
        exit 73
    fi
    sleep 0.5
done
log "f1r3drive FUSE mount ready at ${F1R3DRIVE_MOUNT}/${F1R3DRIVE_WALLET_ADDRESS}"

# Phase 3 - start embers. The first process to exit takes the other down with
# it (the containerized equivalent of the native stack's BindsTo coupling).
/app/embers &
embers_pid=$!

wait -n "${drive_pid}" "${embers_pid}"
status=$?
if ((terminating)); then
    wait "${embers_pid}" 2>/dev/null
    wait "${drive_pid}" 2>/dev/null
    fusermount3 -u "${F1R3DRIVE_MOUNT}" 2>/dev/null || true
    exit 0
fi
if kill -0 "${embers_pid}" 2>/dev/null; then
    log "f1r3drive exited (status ${status}) - stopping embers"
    kill -INT "${embers_pid}" 2>/dev/null
    wait "${embers_pid}" 2>/dev/null
else
    log "embers exited (status ${status}) - stopping f1r3drive"
    kill -INT "${drive_pid}" 2>/dev/null
    wait "${drive_pid}" 2>/dev/null
fi
fusermount3 -u "${F1R3DRIVE_MOUNT}" 2>/dev/null || true
exit "${status}"
