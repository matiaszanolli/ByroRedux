#!/usr/bin/env bash
# #5142 — static contracts for every harness that drives the engine
# through `byro-dbg`. Both rules come from 63c0aee3b (#4752):
#
#   1. The debug server's `screenshot` command accepts only a BARE
#      filename and writes under the engine cwd's `screenshots/` dir.
#      A harness line that passes anything containing '/' is rejected
#      at runtime ("screenshot path must be a filename inside
#      screenshots/"), so the capture never lands and the pixel gate
#      downstream fails with no hint. This cost every pre-63c0aee3b
#      harness its captures at once.
#   2. A RELEASE engine binds the debug server only behind the explicit
#      `BYRO_DEBUG_SERVER=1` opt-in. A release harness that never sets
#      it launches an engine byro-dbg can never attach to — the failure
#      the 2026-09-29 tooling audit found in seven launchers.
#
# Purely static: no game data, no Vulkan, no build. Runs in CI next to
# scripts/check-playable-smoke-contracts.sh.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

python3 - "$repo" <<'PY'
import re
import sys
from pathlib import Path

repo = Path(sys.argv[1])
targets = sorted(
    list((repo / "docs" / "smoke-tests").glob("*.sh"))
    + list((repo / "scripts").glob("*.sh"))
    # The runtime-audit harness lives outside both dirs.
    + [repo / ".claude" / "commands" / "audit-runtime" / "capture.sh"]
)

violations = []
for path in targets:
    rel = path.relative_to(repo)
    text = path.read_text(errors="replace")
    for n, line in enumerate(text.splitlines(), 1):
        code = line.split("#", 1)[0]
        # Rule 1a — `screenshot <arg>`: the console command's argument
        # must be a bare filename. `--screenshot` (the engine CLI flag)
        # takes real paths and is exempt via the lookbehind.
        m = re.search(r"(?<![-\w])screenshot\s+([^\s;&|]+)", code)
        if m and "/" in m.group(1):
            violations.append(
                f"{rel}:{n}: `screenshot` arg contains '/' "
                f"(bare filename only): {line.strip()}"
            )
        # Rule 1b — `printf 'screenshot %s' "<path>"`: the argument
        # rides a format string, so check the whole line for a path.
        if re.search(r"printf\s+'screenshot\s+%s", code) and "/" in code:
            violations.append(
                f"{rel}:{n}: printf-fed `screenshot` passes a path: {line.strip()}"
            )
    # Rule 2 — a harness that drives a RELEASE binary through byro-dbg
    # must set the opt-in somewhere in the script.
    is_release = ("--release" in text) or ("target/release" in text)
    if is_release and "byro-dbg" in text and "BYRO_DEBUG_SERVER" not in text:
        violations.append(
            f"{rel}: release byro-dbg harness without BYRO_DEBUG_SERVER=1 "
            f"(the release engine never binds the debug port)"
        )

if violations:
    print("byro-dbg harness contract violations:", file=sys.stderr)
    for v in violations:
        print(f"  {v}", file=sys.stderr)
    sys.exit(1)

print(
    f"byro-dbg harness contracts: OK ({len(targets)} scripts scanned, "
    f"bare-filename screenshots + release opt-in present where required)"
)
PY
