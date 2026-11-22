#!/usr/bin/env bash
# M-LIGHT A12 (RLT-IBL-006): GI that leaves a Show Sky interior reads the
# portal sky. Boots FO4 DBTechHighSchool01 (Show Sky) held open with a control
# engine and the build under test, pins the exposure over the debug server,
# shoots `indirect_only` and `traced_share` at noon, midnight and noon again
# (`time.set`), and reports each engine's mean level, the noise floor and head
# minus control at each hour. A report, not a gate
# (docs/prd/reference-lighting.md §7.2). Details: scripts/m-light/portal_sky.py.
#
# Usage: docs/smoke-tests/m-light-portal-sky.sh CONTROL_BINARY [HEAD_BINARY]
# HEAD_BINARY defaults to target/release/byroredux. Needs a Fallout 4 install
# (scripts/bench-scenes.sh), a Vulkan device and a display, and
# BYROREDUX_MITSUBA_PYTHON pointing at a Python with Mitsuba 3 and NumPy
# (used only to read the screenshots). Also needs target/release/byro-dbg.

set -euo pipefail

# SKIP != PASS (#6461): a runner without the Fallout 4 data measures nothing.
DATA="${BYROREDUX_FO4_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data}"
if [ ! -f "$DATA/Fallout4.esm" ]; then
    echo "smoke[m-light-portal-sky]: SKIP -- missing $DATA/Fallout4.esm"
    exit 77
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
control="${1:?usage: m-light-portal-sky.sh CONTROL_BINARY [HEAD_BINARY]}"
head="${2:-${ROOT}/target/release/byroredux}"
python="${BYROREDUX_MITSUBA_PYTHON:-python3}"
# A release engine binds the debug port only behind this opt-in (portal_sky.py sets it on the
# engines it launches as well).
export BYRO_DEBUG_SERVER=1
exec "${python}" -I -B "${ROOT}/scripts/m-light/portal_sky.py" --control "${control}" --head "${head}"
