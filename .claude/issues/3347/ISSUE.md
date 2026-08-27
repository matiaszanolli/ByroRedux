# FNV-2026-08-26-D8-04

**Issue**: #3347
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 8 — Real-Data Validation & Bench
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `scripts/fsr-bench-matrix.sh:160-200`

**Premise verified**: the run loop's only sanity gate is "did a `bench:` line appear":

```bash
line="$(grep '^bench:' "$log" | tail -1)"
if [[ -z "$line" ]]; then
  echo "warn: $scene/$name run $run produced no bench line (see $log)" >&2
  continue
fi
```

Everything after that is parsed and appended to the TSV unconditionally. The script's
own header (lines 22-26) documents the failure mode it does not check for — "running
from elsewhere makes archives silently fail to open and the scene loads near-empty
with a spurious FPS figure" — and the harness runs with `RUST_LOG=warn`, so the #1776
`log::error!` *is* in the log but nothing greps for it. The engine still emits a
`bench:` line for a 36-entity scene, so such a run is recorded as data.

This is the same class of defect #2835 already fixed once at the framing level (the
`BenchCameraPath::Orbit` world-origin radius bug had Prospector benchmarking an empty
view against 1214 draws for multiple cycles). Entity floors are standard practice
elsewhere in the tree — `docs/smoke-tests/m47-triggers.sh:141`,
`m43-quest-runtime.sh:101`, `m-trees.sh:126`, `p0-door-interaction.sh:164`,
`m-exteriors.sh:285` all gate on one — but the bench-of-record harness has none.

**Impact**: the artifact this whole dimension treats as ground truth has no
tripwire against the exact silent-failure mode its own comments describe. Given the
tracker has now deferred a re-run 7 times (R6a-stale-20), the eventual 75-run matrix
is a single high-stakes shot with no self-check.

**Fix sketch**: after parsing `line`, fail the run when `entities` is below a
per-scene floor (e.g. 50% of the previous archive's value) and/or when the log
contains `--bsa was specified but 0 mesh archives opened`; also assert `state_hash`
is constant across the three runs of a config, which the archived TSV already is.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
