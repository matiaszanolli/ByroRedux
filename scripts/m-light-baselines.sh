#!/usr/bin/env bash
# M-LIGHT Phase 1b global baselines (docs/prd/reference-lighting.md §9):
# settled tier and pinned-tier frame time per benchmark scene (A13), content
# anchors (A11) and the material-class report (A9). See
# scripts/m-light/baselines.py. Needs the games' Data folders under
# BYROREDUX_GAMES_ROOT and a Mitsuba 3 + NumPy interpreter in
# BYROREDUX_MITSUBA_PYTHON (EXR reading only).
#
#   scripts/m-light-baselines.sh                          # every scene, 3 pinned runs
#   scripts/m-light-baselines.sh --scenes cornell --runs 1

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python="${BYROREDUX_MITSUBA_PYTHON:-python3}"
if ! "${python}" -I -c 'import mitsuba, numpy' >/dev/null 2>&1; then
    echo "m-light-baselines: ${python} cannot import mitsuba and numpy;" \
         "set BYROREDUX_MITSUBA_PYTHON" >&2
    exit 2
fi
exec "${python}" -I -B "${repo_root}/scripts/m-light/baselines.py" "$@"
