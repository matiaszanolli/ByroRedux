#!/usr/bin/env bash
# One `(game, cell)` runtime capture, with the teardown and attribution
# assertions the prose version of this procedure could not enforce (#3560).
#
# The defect this exists for: the documented teardown was `kill -INT $PID` on
# the backgrounded `xvfb-run …` job. `xvfb-run` runs its command as a CHILD
# (`DISPLAY=… "$@"`, no `exec` — read /usr/bin/xvfb-run), so that signal kills
# the wrapper, its EXIT trap tears down Xvfb, and the ENGINE keeps running and
# keeps holding port 9876. The next game's capture then connected to the
# previous game's still-live engine and recorded its numbers under the new
# filename — the RT-1/#1619 mis-attribution reached through teardown failure
# rather than through parallelism, so running serially (as the skill instructs)
# does not prevent it. Live repro, 2026-08-30: an FNV run reported Oblivion's
# `Entities: 718` and Oblivion's exact 8-path `tex.missing` list, with `dbg up
# at 1s` for a cell that takes ~40 s to load as the only tell.
#
# Three assertions, in the harness rather than in prose, because prose is what
# failed:
#   1. PRE-FLIGHT   no engine process, port free — refuse to start otherwise.
#   2. REAL PID     resolve the engine's own PID after launch and kill THAT,
#                   then sweep any survivor.
#   3. CROSS-CHECK  `Entities:` from the `byro-dbg` stats stream against
#                   `entities=` on the engine's own `bench:` line. They come
#                   from two different transports; a capture that read the
#                   wrong engine disagrees wildly.
#
# Usage:
#   capture.sh --game <key> --cell <EDID> --out <dir> [--frames N]
#   capture.sh --self-test
#
# `pgrep -x`, not `pgrep -f`: the `-f` form matches this script's own command
# line (it contains the string `target/release/byroredux`) and so always
# "finds" an engine, which would make the pre-flight assertion vacuous and the
# survivor sweep target the harness.

set -euo pipefail

# Process name to look for. A variable so `--self-test` can point the PID
# resolution and sweep at a scratch process instead of a real engine.
ENGINE_PROC="${ENGINE_PROC:-byroredux}"
DEBUG_PORT="${BYRO_DEBUG_PORT:-9876}"

# Same tolerance the skill's baseline diff uses for `entities_total`. The two
# numbers are captured at different moments of the same run (the `bench:` line
# at frame N, the `stats` line once `byro-dbg` attaches), so streaming can move
# them slightly; mis-attribution moves them by orders of magnitude.
ENTITIES_TOLERANCE_PCT="${ENTITIES_TOLERANCE_PCT:-2}"

log() { printf 'capture: %s\n' "$*" >&2; }
die() {
    printf 'capture: FATAL %s\n' "$*" >&2
    exit 1
}

# --- pure helpers (self-tested) ---------------------------------------------

# `Entities: N` out of a `byro-dbg` stats stream. NOT anchored: the real line
# is one pipe-separated row prefixed by the prompt —
# `byro> FPS: 58.9 (avg 90.4) | Frame: 16.98ms | Entities: 3146 | Meshes: …`
# (an anchored version of this returned empty against a live capture, which
# the cross-check then reported as MISSING).
entities_from_stats() {
    grep --only-matching --max-count=1 -E 'Entities:[[:space:]]*[0-9]+' |
        grep --only-matching -E '[0-9]+' || true
}

# `entities=N` out of the engine log's `bench:` summary line.
entities_from_bench() {
    grep -E '^bench:' | grep --only-matching --max-count=1 -E 'entities=[0-9]+' |
        grep --only-matching -E '[0-9]+' || true
}

# Do the two transports agree about how many entities this run had? Prints a
# verdict line; exit 0 agree, 1 diverge, 2 a number is missing.
cross_check_entities() {
    local stats="$1" bench="$2"
    if [[ -z "${stats}" || -z "${bench}" ]]; then
        echo "MISSING stats='${stats}' bench='${bench}'"
        return 2
    fi
    local delta=$((stats - bench))
    [[ "${delta}" -lt 0 ]] && delta=$((-delta))
    # Percent of the bench figure, integer arithmetic, rounding up so a
    # divergence can never be tolerated by truncation.
    local budget=$(((bench * ENTITIES_TOLERANCE_PCT + 99) / 100))
    if [[ "${delta}" -le "${budget}" ]]; then
        echo "OK stats=${stats} bench=${bench} delta=${delta} budget=${budget}"
        return 0
    fi
    echo "DIVERGED stats=${stats} bench=${bench} delta=${delta} budget=${budget}"
    return 1
}

# Is the debug port already bound? Uses `ss` where present and falls back to a
# connect attempt, so this works on a box without iproute2.
port_is_bound() {
    local port="$1"
    if command -v ss >/dev/null 2>&1; then
        ss -lnt "sport = :${port}" 2>/dev/null | grep --quiet LISTEN && return 0
        return 1
    fi
    (timeout 1 bash -c "exec 3<>/dev/tcp/127.0.0.1/${port}") 2>/dev/null
}

# The engine's own PID, or empty. Waits up to `$2` seconds for it to appear —
# `xvfb-run` has to start Xvfb before it execs anything.
resolve_engine_pid() {
    local proc="$1" deadline="${2:-30}" i pid
    for ((i = 0; i < deadline; i++)); do
        pid="$(pgrep -x "${proc}" | head -1 || true)"
        [[ -n "${pid}" ]] && {
            echo "${pid}"
            return 0
        }
        sleep 1
    done
    return 1
}

# Kill everything named `$1` that is still alive, TERM then KILL. Returns 0
# once none remain.
sweep_survivors() {
    local proc="$1" i
    pgrep -x "${proc}" >/dev/null 2>&1 || return 0
    pkill -x -TERM "${proc}" 2>/dev/null || true
    for ((i = 0; i < 10; i++)); do
        pgrep -x "${proc}" >/dev/null 2>&1 || return 0
        sleep 1
    done
    pkill -x -KILL "${proc}" 2>/dev/null || true
    sleep 1
    pgrep -x "${proc}" >/dev/null 2>&1 && return 1
    return 0
}

# --- self-test ---------------------------------------------------------------

if [[ "${1:-}" == "--self-test" ]]; then
    expect() {
        local what="$1" got="$2" want="$3"
        [[ "${got}" == "${want}" ]] || die "self-test ${what}: got '${got}' want '${want}'"
    }

    # A real captured line, verbatim (fnv, 2026-09-07) — the synthetic
    # fixture this replaced was anchored at line start and passed against a
    # parser that could not read the live format.
    expect "stats parse" \
        "$(entities_from_stats <<<'byro> FPS: 58.9 (avg 90.4) | Frame: 16.98ms | Entities: 3146 | Meshes: 538/535 | Textures: 347/163 | Draws: 928 cmds')" 3146
    expect "stats parse (multi-line)" \
        "$(entities_from_stats <<<$'Connected.\nbyro> FPS: 60 | Entities: 718 | Draws: 12')" 718
    expect "stats parse (absent)" "$(entities_from_stats <<<'nothing here')" ""
    expect "bench parse" \
        "$(entities_from_bench <<<$'noise\nbench: mode=frames wall_fps=41.2 entities=2934 draws=1/2/3')" 2934
    # A `bench-hold:` line also contains `entities=`-adjacent text in some
    # builds; only the `bench:` line counts.
    expect "bench parse (wrong line)" \
        "$(entities_from_bench <<<'bench-hold: holding open, entities=99')" ""

    expect "cross-check agree" "$(cross_check_entities 2934 2934)" "OK stats=2934 bench=2934 delta=0 budget=59"
    # Inside the tolerance: streaming moved a handful of entities between the
    # bench line and the attach.
    cross_check_entities 2940 2934 >/dev/null || die "self-test: a 6-entity drift must be tolerated"
    # The live 2026-08-30 mis-attribution: Oblivion's 718 under FNV's filename.
    if cross_check_entities 718 2934 >/dev/null; then
        die "self-test: a 4x divergence must NOT pass the cross-check"
    fi
    if cross_check_entities "" 2934 >/dev/null; then
        die "self-test: a missing number must not read as agreement"
    fi

    # PID resolution and the survivor sweep, against a scratch process rather
    # than a real engine. This is the half that actually failed in the field:
    # the wrapper died, the child did not.
    scratch="$(mktemp -d)"
    # A renamed copy of `bash`, not of `sleep`: on this box `/usr/bin/sleep`
    # is a symlink into a uutils multi-call binary that dispatches on argv[0],
    # so a renamed copy exits 0 immediately instead of sleeping. The copy
    # blocks on a fifo open, which is a builtin — no child process to leak
    # when the sweep kills it, and `comm` is the scratch name, which is what
    # `pgrep -x` matches on.
    cp "$(command -v bash)" "${scratch}/bxselftest"
    mkfifo "${scratch}/block"
    # stdio to /dev/null, and swept on ANY exit path. Inheriting this
    # script's stdout would keep the write end of a caller's pipe open after
    # a failed assertion, so `capture.sh --self-test | tail` would hang
    # instead of reporting the failure — found by fault-injecting these very
    # assertions.
    "${scratch}/bxselftest" -c "read -r _ < '${scratch}/block'" \
        >/dev/null 2>&1 </dev/null &
    scratch_wrapper=$!
    trap 'pkill -x bxselftest 2>/dev/null || true; rm -rf "${scratch}"' EXIT
    found="$(resolve_engine_pid bxselftest 10 || true)"
    [[ -n "${found}" ]] || die "self-test: resolve_engine_pid found no scratch process"
    sweep_survivors bxselftest || die "self-test: sweep left a survivor"
    pgrep -x bxselftest >/dev/null 2>&1 && die "self-test: scratch process survived the sweep"
    wait "${scratch_wrapper}" 2>/dev/null || true
    trap - EXIT
    rm -rf "${scratch}"

    # `pgrep -f` self-match: the trap the issue names. This script's own
    # command line carries the engine path, so the `-f` form reports a hit
    # with no engine running at all.
    if pgrep -f 'target/release/byroredux' >/dev/null 2>&1 &&
        ! pgrep -x byroredux >/dev/null 2>&1; then
        log "self-test: confirmed pgrep -f self-matches where -x does not"
    fi

    echo "capture: self-test passed"
    exit 0
fi

# --- arguments ---------------------------------------------------------------

GAME="" CELL="" OUT="" FRAMES=240
while [[ "$#" -gt 0 ]]; do
    case "$1" in
    --game)
        GAME="$2"
        shift 2
        ;;
    --cell)
        CELL="$2"
        shift 2
        ;;
    --out)
        OUT="$2"
        shift 2
        ;;
    --frames)
        FRAMES="$2"
        shift 2
        ;;
    *) die "unknown argument '$1'" ;;
    esac
done
[[ -n "${GAME}" && -n "${OUT}" ]] || die "usage: $0 --game <key> --cell <EDID> --out <dir> [--frames N]"

ENGINE_BIN="./target/release/byroredux"
DBG_BIN="./target/release/byro-dbg"
[[ -x "${ENGINE_BIN}" ]] || die "${ENGINE_BIN} not built"
[[ -x "${DBG_BIN}" ]] || die "${DBG_BIN} not built"
mkdir -p "${OUT}"

label="${GAME}${CELL:+-${CELL}}"
engine_log="${OUT}/${label}.engine.log"
telem="${OUT}/${label}.telem.txt"

# --- 1. pre-flight ------------------------------------------------------------

if pgrep -x "${ENGINE_PROC}" >/dev/null 2>&1; then
    die "an engine is already running (pids: $(pgrep -x "${ENGINE_PROC}" | tr '\n' ' ')) \
— a capture started now would read ITS telemetry, not this game's (#3560)"
fi
if port_is_bound "${DEBUG_PORT}"; then
    die "port ${DEBUG_PORT} is already bound — the debug server does not rebind or retry, \
so this capture would attach to whatever owns it (#3560)"
fi

# --- 2. launch, and resolve the ENGINE's pid ---------------------------------

log "launching ${label}"
xvfb-run -a --server-args="-screen 0 1280x720x24" \
    "${ENGINE_BIN}" --game "${GAME}" ${CELL:+--cell "${CELL}"} \
    --bench-frames "${FRAMES}" --bench-hold \
    >"${engine_log}" 2>&1 &
wrapper_pid=$!

engine_pid="$(resolve_engine_pid "${ENGINE_PROC}" 30 || true)"
if [[ -z "${engine_pid}" ]]; then
    kill -TERM "${wrapper_pid}" 2>/dev/null || true
    die "engine process never appeared — see ${engine_log}"
fi
log "wrapper=${wrapper_pid} engine=${engine_pid}"

teardown() {
    # Kill the ENGINE first and the wrapper second. The other order leaves the
    # engine orphaned and holding the port, which is the whole defect.
    kill -TERM "${engine_pid}" 2>/dev/null || true
    sleep 2
    kill -KILL "${engine_pid}" 2>/dev/null || true
    kill -TERM "${wrapper_pid}" 2>/dev/null || true
    wait "${wrapper_pid}" 2>/dev/null || true
    sweep_survivors "${ENGINE_PROC}" ||
        log "WARNING: an engine process survived the sweep — do NOT start another capture"
}
trap teardown EXIT

# --- 3. wait for the debug server, then capture ------------------------------

up_at=""
for i in $(seq 1 90); do
    if echo "ping" | timeout 2 "${DBG_BIN}" 2>/dev/null | grep --quiet -i pong; then
        up_at="${i}"
        break
    fi
    sleep 1
done
[[ -n "${up_at}" ]] || die "debug server never came up — see ${engine_log}"
log "dbg up at ${up_at}s"

sleep 3
printf "stats\ntex.missing\nmesh.cache failed\nlight.dump\nquit\n" |
    "${DBG_BIN}" >"${telem}" 2>&1

# --- 4. cross-check the attribution ------------------------------------------

stats_entities="$(entities_from_stats <"${telem}")"
bench_entities="$(entities_from_bench <"${engine_log}")"
verdict="$(cross_check_entities "${stats_entities}" "${bench_entities}")" && status=0 || status=$?
log "entities cross-check: ${verdict}"
if [[ "${status}" -ne 0 ]]; then
    die "the byro-dbg stream and the engine's own bench line disagree about this \
run's entity count — discard this capture, it is reading a different engine \
than the one just launched (#3560). Report: ${verdict}"
fi

log "captured ${label}: ${telem}"
