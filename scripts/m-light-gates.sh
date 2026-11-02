#!/usr/bin/env bash
# M-LIGHT reference gates (docs/prd/reference-lighting.md, RLT-OPS-001).
#
# Runs the Cornell reference rungs through the engine, renders their
# converged references in Mitsuba 3 (cached under the output directory), and
# writes manifest.tsv with every metric, its noise floor and its pass state.
# See scripts/m-light/gates.py for the gate definitions.
#
# Mitsuba 3 and NumPy are developer tools, never engine dependencies. Point
# BYROREDUX_MITSUBA_PYTHON at an interpreter that has them, e.g. a venv made
# with `python3 -m venv ~/mitsuba && ~/mitsuba/bin/pip install mitsuba numpy`.
#
#   scripts/m-light-gates.sh                 # A15 + A16, warm cache
#   scripts/m-light-gates.sh --cold          # empty the reference cache first (A18)
#   scripts/m-light-gates.sh --gates A16 --out target/m-light-closure

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python="${BYROREDUX_MITSUBA_PYTHON:-python3}"

if ! "${python}" -I -c 'import mitsuba, numpy' >/dev/null 2>&1; then
    echo "m-light-gates: ${python} cannot import mitsuba and numpy;" \
         "set BYROREDUX_MITSUBA_PYTHON to a Mitsuba 3 interpreter" >&2
    exit 2
fi
if [[ -z "${DISPLAY:-}" && -z "${WAYLAND_DISPLAY:-}" ]]; then
    echo "m-light-gates: the engine needs a display (or run under xvfb-run)" >&2
    exit 2
fi

exec "${python}" -I -B "${repo_root}/scripts/m-light/gates.py" "$@"
