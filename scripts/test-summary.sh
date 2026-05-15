#!/usr/bin/env bash
# scripts/test-summary.sh — print a categorised test count summary.
#
# Outputs a table of:
#   PASSED   — tests that ran and succeeded
#   IGNORED  — tests skipped (game-data-gated, device-gated, etc.)
#   FAILED   — tests that ran and failed (non-zero exit if any)
#
# Also prints the IGNORED count by category prefix so contributors can
# see how many tests are waiting on each class of external dependency.
# Pins the ignored count so CI can detect silent test deletion (a
# shrinking ignore count doesn't necessarily mean tests were fixed;
# it might mean they were removed). See TD6-010 / #1050.
#
# Usage:
#   scripts/test-summary.sh              # full workspace
#   scripts/test-summary.sh -p foo       # single crate
#
# Pass extra cargo test flags after '--':
#   scripts/test-summary.sh -- --release
#
# Exit code: 0 if all ran tests pass, 1 if any failed.

set -euo pipefail

EXTRA_ARGS=()
CARGO_ARGS=()

# Split args on '--' so flags before go to the script, flags after go
# to cargo test.
parsing_extra=0
for arg in "$@"; do
    if [[ "$arg" == "--" ]]; then
        parsing_extra=1
    elif [[ $parsing_extra -eq 1 ]]; then
        EXTRA_ARGS+=("$arg")
    else
        CARGO_ARGS+=("$arg")
    fi
done

# Run the tests and capture output. Preserve exit code.
TEST_OUTPUT=$(cargo test "${CARGO_ARGS[@]}" "${EXTRA_ARGS[@]}" 2>&1 || true)
EXIT_CODE=$(cargo test "${CARGO_ARGS[@]}" "${EXTRA_ARGS[@]}" >/dev/null 2>&1; echo $?)

# --- Parse totals from the "test result: ok. N passed; M failed; K ignored" lines ---
TOTAL_PASSED=0
TOTAL_FAILED=0
TOTAL_IGNORED=0

while IFS= read -r line; do
    if [[ "$line" =~ ^test\ result:.*([0-9]+)\ passed ]]; then
        p="${BASH_REMATCH[1]}"
        TOTAL_PASSED=$((TOTAL_PASSED + p))
    fi
    if [[ "$line" =~ ([0-9]+)\ failed ]]; then
        f="${BASH_REMATCH[1]}"
        TOTAL_FAILED=$((TOTAL_FAILED + f))
    fi
    if [[ "$line" =~ ([0-9]+)\ ignored ]]; then
        i="${BASH_REMATCH[1]}"
        TOTAL_IGNORED=$((TOTAL_IGNORED + i))
    fi
done <<< "$TEST_OUTPUT"

echo ""
echo "╔══════════════════════════════════╗"
echo "║        ByroRedux Test Summary    ║"
echo "╠══════════════════════════════════╣"
printf "║  %-8s %22s  ║\n" "PASSED"  "$TOTAL_PASSED"
printf "║  %-8s %22s  ║\n" "IGNORED" "$TOTAL_IGNORED"
printf "║  %-8s %22s  ║\n" "FAILED"  "$TOTAL_FAILED"
echo "╚══════════════════════════════════╝"
echo ""

# --- Category breakdown of ignored tests ---
# Extract the list of ignored test names by running with --list.
IGNORED_LIST=$(cargo test "${CARGO_ARGS[@]}" -- --list --ignored 2>/dev/null | grep "^test " | sed 's/^test //;s/: .*$//' || true)

if [[ -n "$IGNORED_LIST" ]]; then
    echo "Ignored by category (prefix):"
    echo "$IGNORED_LIST" | sed 's/::.*//' | sort | uniq -c | sort -rn | \
        while read -r count prefix; do
            printf "  %-40s %3d\n" "$prefix" "$count"
        done
    echo ""
fi

# --- Sentinel: warn if ignored count drops (silent test deletion) ---
SENTINEL_FILE=".test-ignore-count"
if [[ -f "$SENTINEL_FILE" ]]; then
    PREV=$(cat "$SENTINEL_FILE")
    if (( TOTAL_IGNORED < PREV )); then
        echo "WARNING: ignored test count dropped $PREV → $TOTAL_IGNORED."
        echo "  If tests were fixed, update $SENTINEL_FILE: echo $TOTAL_IGNORED > $SENTINEL_FILE"
        echo "  If tests were deleted, investigate before lowering the sentinel."
    fi
fi

# Update sentinel after each successful run.
echo "$TOTAL_IGNORED" > "$SENTINEL_FILE"

if (( TOTAL_FAILED > 0 )); then
    echo "FAILED: $TOTAL_FAILED test(s) failed — see output above."
    exit 1
fi

exit 0
