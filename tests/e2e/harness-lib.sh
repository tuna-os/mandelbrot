#!/usr/bin/env bash
# Shared helpers for the MatrixRTC e2e harness scripts.
#
# Sourced by run-resilience.sh, run-restricted-sfu.sh and run-huddle.sh.
# `run-interop.sh` and `run-interop-ec.sh` predate this file and keep their
# own inlined copies so that they stay standalone.
#
# Conventions (matching run-interop-ec.sh):
#   log <msg>                  progress line on stderr-ish stdout
#   result PASS|FAIL <name> …  records an assertion; the script exits 1 if
#                              any FAIL was recorded
#   result INFO <name> …       records an observation that does not gate
#   summary                    prints the RESULT block and returns the exit
#                              code

HS=${HS:-http://127.0.0.1:8008}
JWT=${JWT:-http://127.0.0.1:6080}
LK=${LK:-http://127.0.0.1:7880}
REG_SECRET=${REG_SECRET:-test_shared_secret_for_local_dev_only}

RESULTS=()
HARNESS_NAME=${HARNESS_NAME:-harness}

log() { printf '[%s] %s\n' "$HARNESS_NAME" "$*"; }
result() { # PASS|FAIL|INFO <name> [details]
    RESULTS+=("$1 $2 ${3:-}")
    log "RESULT $1 $2 ${3:-}"
}
die() {
    log "FATAL: $*"
    exit 2
}

summary() {
    log "===== SUMMARY ($HARNESS_NAME) ====="
    local failed=0 r
    for r in "${RESULTS[@]}"; do
        log "  $r"
        [[ $r == FAIL* ]] && failed=1
    done
    [ -n "${LOG_DIR:-}" ] && log "evidence bundle: $LOG_DIR"
    return $failed
}

# --- compose ---------------------------------------------------------------

# Set COMPOSE in the environment to force a provider (e.g. COMPOSE="docker
# compose" on a runner that has podman installed but no podman backend).
detect_compose() {
    if [ -n "${COMPOSE:-}" ]; then
        log "compose provider (forced): $COMPOSE"
        return
    fi
    if command -v podman-compose >/dev/null; then
        COMPOSE="podman-compose"
    elif podman compose version >/dev/null 2>&1; then
        COMPOSE="podman compose"
    elif docker compose version >/dev/null 2>&1; then
        COMPOSE="docker compose"
    else
        die "no compose provider found"
    fi
    log "compose provider: $COMPOSE"
}

# The container engine, for the per-container operations (pause, restart,
# logs) the scenarios below need. Compose has no portable equivalent.
ENGINE=${ENGINE:-}
detect_engine() {
    if [ -n "$ENGINE" ]; then return; fi
    if command -v podman >/dev/null; then
        ENGINE=podman
    elif command -v docker >/dev/null; then
        ENGINE=docker
    else
        die "neither podman nor docker found"
    fi
}

# Resolve the container name of a compose service. Works for both
# podman-compose and docker compose naming schemes.
container_of() { # service -> container name
    detect_engine
    local name
    name=$($ENGINE ps --filter "label=com.docker.compose.service=$1" \
        --format '{{.Names}}' 2>/dev/null | head -1)
    if [ -z "$name" ]; then
        name=$($ENGINE ps --format '{{.Names}}' 2>/dev/null |
            grep -E "(^|[_-])$1([_-][0-9]+)?$" | head -1)
    fi
    [ -n "$name" ] || return 1
    printf '%s\n' "$name"
}

stack_up() {
    detect_compose
    mkdir -p synapse_tmp
    if curl -sf "$HS/_matrix/client/versions" >/dev/null; then
        log "backend stack already running"
    else
        log "starting the backend stack"
        $COMPOSE -f compose.yml up -d >/dev/null 2>&1 || die "compose up failed"
    fi
    wait_for "$HS/_matrix/client/versions" 180 synapse
    wait_for "$LK/" 60 livekit
    wait_for "$JWT/healthz" 60 lk-jwt-service
}

wait_for() { # url timeout_s name
    local i
    for i in $(seq 1 "$2"); do
        curl -s -o /dev/null "$1" && return 0
        sleep 1
    done
    die "$3 did not come up at $1"
}

stack_down() {
    if [ "${KEEP_STACK:-0}" = 1 ]; then
        log "KEEP_STACK=1: leaving the stack running"
        return
    fi
    $COMPOSE -f compose.yml down -v >/dev/null 2>&1 || true
}

# --- matrix ----------------------------------------------------------------

register() { # username -> registration JSON
    curl -sf -X POST "$HS/_matrix/client/v3/register" \
        -H 'Content-Type: application/json' \
        -d "{\"username\":\"$1\",\"password\":\"testpassword\",\"auth\":{\"type\":\"m.login.dummy\"}}"
}

create_room() { # access_token [extra_json] -> room id
    local body='{"preset":"public_chat","name":"matrixrtc harness","power_level_content_override":{"events":{"org.matrix.msc3401.call.member":0,"org.matrix.msc4075.rtc.notification":0}}}'
    [ -n "${2:-}" ] && body=$(jq -c ". * $2" <<<"$body")
    curl -sf -X POST "$HS/_matrix/client/v3/createRoom" \
        -H "Authorization: Bearer $1" -H 'Content-Type: application/json' \
        -d "$body" | jq -r .room_id
}

join_room() { # access_token room_id
    curl -sf -X POST "$HS/_matrix/client/v3/join/$2" \
        -H "Authorization: Bearer $1" -H 'Content-Type: application/json' \
        -d '{}' >/dev/null
}

member_events() { # access_token room_id -> array of m.call.member state events
    curl -sf "$HS/_matrix/client/v3/rooms/$2/state" \
        -H "Authorization: Bearer $1" |
        jq '[.[] | select(.type == "org.matrix.msc3401.call.member")]'
}

active_memberships() { # access_token room_id -> count
    member_events "$1" "$2" | jq '[.[] | select(.content != {})] | length'
}

# Wait until the number of active memberships equals $3.
wait_memberships() { # access_token room_id count timeout_s
    local i
    for i in $(seq 1 "$4"); do
        [ "$(active_memberships "$1" "$2")" = "$3" ] && return 0
        sleep 1
    done
    return 1
}

# --- clients ---------------------------------------------------------------

# Wait until `pattern` appears in `logfile`.
wait_log() { # logfile pattern timeout_s
    local i
    for i in $(seq 1 "$3"); do
        grep -qE "$2" "$1" 2>/dev/null && return 0
        sleep 1
    done
    return 1
}

# The number of lines matching `pattern` in `logfile`.
count_log() { # logfile pattern -> count
    grep -cE "$2" "$1" 2>/dev/null || true
}
