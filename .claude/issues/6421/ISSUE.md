# EXT-D6-2026-10-26-04: `docs/smoke-tests/README.md` row for `w1-water-traversal.sh` still says Skyrim declares no route

Labels: low,water,doc-rot,documentation,game:skyrim
Source: docs/audits/AUDIT_EXTERIOR_2026-10-26.md

- **Severity**: LOW / **Dimension**: 7 (docs) / **Tier Violated**: n/a
- **Game Affected**: Skyrim
- **Location**: `docs/smoke-tests/README.md:300`
- **Status**: NEW
- **Description**: `ceaf85a54` (2026-10-26) gave Skyrim a measured W1 route (`fixtures/skyrim_se.env` `W1_*`, Tamriel `(-9,-16)`) and rewrote the README paragraph at line ~141, but the table row still reads "`skyrim_se` declares no route and is SKIPped, with the measured reason in its fixture". The two README statements now contradict each other.
- **Impact**: a reader trusts the table and skips a gate that now runs.
- **Suggested Fix**: replace the clause with "`skyrim_se` runs the northern sea shore at Tamriel (-9,-16) (the river banks that wedge the controller are recorded in its fixture)".

_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-26.md` (HEAD 3e6f6b66a). Static trace; not confirmed on screen unless stated._
