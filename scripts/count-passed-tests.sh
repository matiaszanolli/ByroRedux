#!/usr/bin/env bash
# Print how many tests passed in a `cargo test` log, summed over every test
# binary's `test result: ok. N passed` summary line.
#
#   scripts/count-passed-tests.sh target/real-data-gates/pins-foo.log
#
# The real-data lanes use this to refuse a green no-op. Counting
# `test NAME ... ok` lines instead is wrong under `--nocapture`: a test that
# prints writes its output between `test NAME ... ` and `ok`, so the line never
# matches and a passing pin reads as "matched nothing" (#6319). One helper for
# `.github/workflows/real-data-gates.yml` and `scripts/run-real-data-pins.sh`
# keeps the two from drifting again.
set -euo pipefail

[ "$#" -eq 1 ] || { echo "usage: $0 <cargo-test-log>" >&2; exit 2; }
{ grep -oE 'test result: ok\. [0-9]+ passed' "$1" || true; } | awk '{n += $4} END {print n + 0}'
