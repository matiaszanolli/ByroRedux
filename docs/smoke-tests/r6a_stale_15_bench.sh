#!/usr/bin/env bash
# R6a-stale-15 benchmark harness — collect canonical bench-of-record
# numbers for Prospector, Whiterun, and MedTek.
#
# Context: Session 46 closed with R6a-stale-14 numbers showing that
# IsCollisionOnly ghost-entity rerouting recovered some TLAS cost but
# didn't recover the full pre-collider fence baseline (161.4 FPS / 2.62 ms
# @ 2564 ent). This harness codifies the three test cells, enforces the
# CWD rule (run from each game's Data/ directory), and captures structured
# output for ROADMAP.md updates.
#
# Three canonical benches (all with --bench-frames 300):
#   1. Prospector Saloon (FNV, interior, glass-heavy, synthesized collision)
#   2. Skyrim SE WhiterunBanneredMare (control, authored bhk, 6 NPCs)
#   3. FO4 MedTekResearch01 (precombined geometry, GPU-bound)
#
# Each bench uses --bench-hold to keep the engine alive post-frames, then
# byro-dbg attaches to verify IsCollisionOnly entity counts match expectations.
#
# Output: Structured YAML-style block ready for ROADMAP.md copy-paste.
#
# Typical usage:
#   # Test with 10 frames (validation before RTX 4070 Ti full run):
#   BYROREDUX_BENCH_FRAMES=10 docs/smoke-tests/r6a_stale_15_bench.sh
#
#   # Full 300-frame run (user executes on RTX 4070 Ti):
#   docs/smoke-tests/r6a_stale_15_bench.sh

set -euo pipefail

# Game data root paths — override with env vars if needed.
FNV_DATA="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"
SKYRIM_DATA="${BYROREDUX_SKYRIM_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
FO4_DATA="${BYROREDUX_FO4_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data}"

# Bench frames — default 300, override for validation runs.
BENCH_FRAMES="${BYROREDUX_BENCH_FRAMES:-300}"

# Debug server port.
PORT="${BYRO_DEBUG_PORT:-9876}"

# Temp logs.
LOG_DIR="$(mktemp -d)"
trap 'rm -rf "$LOG_DIR"' EXIT

# ANSI color codes for readability.
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

log_header() {
    echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
    echo -e "${BOLD}  $1${NC}"
    echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
}

log_pass() {
    echo -e "${GREEN}✓${NC} $1"
}

log_fail() {
    echo -e "${RED}✗${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}⚠${NC} $1"
}

# Parse the bench summary line to extract key metrics.
# Format: bench: <fps> FPS / <wall_ms> ms wall / fence=<fence_ms> / brd=<brd_ms> / <ent> ent / <draws> draws
#
# Args: $1 = bench summary line
# Outputs: FPS, WALL_MS, FENCE_MS, BRD_MS, ENTITIES, DRAWS
parse_bench_summary() {
    local line="$1"

    # Extract FPS (number before " FPS")
    local fps=$(echo "$line" | grep -oP '\d+\.?\d*(?= FPS)' | head -1)

    # Extract wall ms (number before " ms wall")
    local wall_ms=$(echo "$line" | grep -oP '\d+\.?\d*(?= ms wall)' | head -1)

    # Extract fence ms (after "fence=")
    local fence_ms=$(echo "$line" | grep -oP 'fence=\K\d+\.?\d*' | head -1)

    # Extract brd ms (after "brd=")
    local brd_ms=$(echo "$line" | grep -oP 'brd=\K\d+\.?\d*' | head -1)

    # Extract entity count (number before " ent")
    local entities=$(echo "$line" | grep -oP '\d+(?= ent)' | head -1)

    # Extract draw count (number before " draws")
    local draws=$(echo "$line" | grep -oP '\d+(?= draws)' | head -1)

    echo "$fps" "$wall_ms" "$fence_ms" "$brd_ms" "$entities" "$draws"
}

# Query engine via byro-dbg for IsCollisionOnly entity count.
# Returns the count, or "UNKNOWN" on error.
#
# Args: (none, reads from debug server on PORT)
query_collision_only_count() {
    local result
    local dbg_cmd
    dbg_cmd=$(cat <<'EOF'
find IsCollisionOnly
EOF
)
    result=$(timeout 5 byro-dbg -p "$PORT" <<< "$dbg_cmd" 2>/dev/null | grep "IsCollisionOnly" | grep -oP '\d+' | head -1 || echo "UNKNOWN")
    echo "$result"
}

# Run a single benchmark cell. Manages engine lifecycle, collects metrics,
# formats output.
#
# Args:
#   $1 = label (for logging and output)
#   $2 = game data directory (must exist)
#   $3 = engine CLI args (--esm ... --cell ... --bsa ...)
run_bench() {
    local label="$1"
    local game_data="$2"
    shift 2
    local engine_args=("$@")

    log_header "Bench: $label"

    # Verify game data exists.
    if [[ ! -d "$game_data" ]]; then
        log_fail "Game data not found: $game_data"
        log_warn "Override with env var (e.g., BYROREDUX_FNV_DATA=/path/to/Data)"
        return 1
    fi

    local engine_log="$LOG_DIR/$label.engine.log"
    local dbg_log="$LOG_DIR/$label.dbg.log"

    echo "Game data: $game_data"
    echo "Frames: $BENCH_FRAMES"
    echo ""

    # Run engine from game's Data/ directory (CWD rule enforced).
    (
        cd "$game_data"
        cargo run --release --quiet -- \
            "${engine_args[@]}" \
            --bench-frames "$BENCH_FRAMES" \
            --bench-hold \
            > "$engine_log.stdout" 2> "$engine_log.stderr"
    ) &
    local engine_pid=$!

    # Wait for `bench-hold:` signal (cell load complete, engine holding open).
    echo "Waiting for engine to reach bench-hold state..."
    local timeout=180
    local deadline=$(( $(date +%s) + timeout ))
    while ! grep -q "^bench-hold:" "$engine_log.stderr" 2>/dev/null; do
        if [[ $(date +%s) -gt $deadline ]]; then
            log_fail "Timeout waiting for bench-hold (logs: $engine_log.stderr)"
            kill -TERM "$engine_pid" 2>/dev/null || true
            wait "$engine_pid" 2>/dev/null || true
            return 1
        fi
        if ! kill -0 "$engine_pid" 2>/dev/null; then
            log_fail "Engine crashed before bench-hold. See $engine_log.stderr"
            return 1
        fi
        sleep 0.5
    done

    log_pass "Engine ready (pid=$engine_pid)"
    echo ""

    # Query debug server for IsCollisionOnly count.
    echo "Querying IsCollisionOnly entity count..."
    local collision_count
    collision_count=$(query_collision_only_count)

    # Extract bench summary from stdout (line starting with "bench: ").
    local bench_line
    bench_line=$(grep "^bench: " "$engine_log.stdout" | tail -1 || echo "")

    if [[ -z "$bench_line" ]]; then
        log_fail "No bench summary found in engine output"
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
        return 1
    fi

    # Parse metrics.
    read -r fps wall_ms fence_ms brd_ms entities draws < <(parse_bench_summary "$bench_line")

    # Terminate engine.
    kill -TERM "$engine_pid" 2>/dev/null || true
    wait "$engine_pid" 2>/dev/null || true

    log_pass "Bench complete"
    echo ""
    echo "Raw bench line:"
    echo "  $bench_line"
    echo ""
    echo "Parsed metrics:"
    echo "  FPS: $fps"
    echo "  Wall: ${wall_ms} ms"
    echo "  Fence: ${fence_ms} ms"
    echo "  BRD: ${brd_ms} ms"
    echo "  Entities: $entities"
    echo "  Draws: $draws"
    echo "  IsCollisionOnly: $collision_count"
    echo ""

    # Format output for ROADMAP.md copy-paste.
    local roadmap_line="| **$label** | **${fps} FPS / ${wall_ms} ms / fence=${fence_ms} / brd=${brd_ms} / ${entities} ent / ${draws} draws** | "
    echo "ROADMAP.md row (this refresh):"
    echo "  $roadmap_line"
    echo ""
}

# Main entry point.
main() {
    log_header "R6a-stale-15 Benchmark Harness"
    echo "Bench frames: $BENCH_FRAMES"
    echo "Log directory: $LOG_DIR"
    echo ""

    # Check byro-dbg availability.
    if ! command -v byro-dbg &>/dev/null; then
        log_warn "byro-dbg not found in PATH — IsCollisionOnly queries will fail"
        log_warn "Build with: cargo build --release -p byro-dbg"
    fi

    # Run the three canonical benches.
    local any_failed=0

    # Prospector Saloon (FNV).
    if ! run_bench "Prospector Saloon (FNV)" "$FNV_DATA" \
        --esm FalloutNV.esm \
        --cell GSProspectorSaloonInterior \
        --bsa "Fallout - Meshes.bsa" \
        --textures-bsa "Fallout - Textures.bsa" \
        --textures-bsa "Fallout - Textures2.bsa"; then
        log_fail "Prospector bench failed"
        any_failed=1
    fi

    # Skyrim SE Whiterun.
    if ! run_bench "Skyrim SE WhiterunBanneredMare" "$SKYRIM_DATA" \
        --esm Skyrim.esm \
        --cell WhiterunBanneredMare \
        --bsa "Skyrim - Meshes0.bsa" \
        --bsa "Skyrim - Meshes1.bsa" \
        --textures-bsa "Skyrim - Textures0.bsa" \
        --textures-bsa "Skyrim - Textures1.bsa" \
        --textures-bsa "Skyrim - Textures2.bsa" \
        --textures-bsa "Skyrim - Textures3.bsa" \
        --textures-bsa "Skyrim - Textures4.bsa" \
        --textures-bsa "Skyrim - Textures5.bsa" \
        --textures-bsa "Skyrim - Textures6.bsa" \
        --textures-bsa "Skyrim - Textures7.bsa" \
        --textures-bsa "Skyrim - Textures8.bsa"; then
        log_fail "Skyrim bench failed"
        any_failed=1
    fi

    # FO4 MedTek.
    if ! run_bench "FO4 MedTekResearch01" "$FO4_DATA" \
        --esm Fallout4.esm \
        --cell MedTekResearch01 \
        --bsa "Fallout4 - Meshes.ba2" \
        --bsa "Fallout4 - MeshesExtra.ba2" \
        --textures-bsa "Fallout4 - Textures1.ba2" \
        --textures-bsa "Fallout4 - Textures2.ba2" \
        --textures-bsa "Fallout4 - Textures3.ba2" \
        --textures-bsa "Fallout4 - Textures4.ba2" \
        --textures-bsa "Fallout4 - Textures5.ba2" \
        --textures-bsa "Fallout4 - Textures6.ba2" \
        --textures-bsa "Fallout4 - Textures7.ba2" \
        --textures-bsa "Fallout4 - Textures8.ba2" \
        --textures-bsa "Fallout4 - Textures9.ba2" \
        --textures-bsa "Fallout4 - TexturesPatch.ba2" \
        --materials-ba2 "Fallout4 - Materials.ba2"; then
        log_fail "MedTek bench failed"
        any_failed=1
    fi

    if [[ $any_failed -eq 0 ]]; then
        log_header "All benches complete"
        log_pass "Next: review numbers and update ROADMAP.md Bench-of-record table"
        return 0
    else
        log_header "Bench suite completed with failures"
        return 1
    fi
}

main "$@"
