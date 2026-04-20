# Issue #489

FNV-5-F3: Prospector Saloon 809/784/48 FPS baseline not CI-gated; ROADMAP has conflicting 48 vs 85 FPS

---

## Severity: Low (doc + CI gate)

**Location**: `ROADMAP.md:14,27,53,403,444`; `.claude/issues/307/INVESTIGATION.md` also references the cell

## Problem

The FNV Prospector Saloon interior baseline is cited across five ROADMAP locations:
- "809 entities, 784 draws, 48 FPS" (most locations)
- "~85 FPS after lighting work" (line 444)

No automated check confirms entity count, draw count, or frametime. Numbers drift silently between renderer refactors (BLAS batching, SVGF, TAA, RIS all landed after the 48 FPS baseline).

## Impact

Can't detect perf regressions. The audit (dim 5) could not re-measure inside budget — no tooling exists to run a cell load headless and dump stats.

## Fix

Add an offline bench mode alongside the existing `--bench-frames`:

```bash
cargo run --release -- --esm FalloutNV.esm --cell GSProspectorSaloonInterior \
  --bsa 'Fallout - Meshes.bsa' --textures-bsa 'Fallout - Textures.bsa' \
  --bench-cell --bench-frames 300
```

Prints: entity count, draw count, texture count, median + p95 frametime, exits. Feed into CI as an expected-range check.

Reconcile the 48 vs 85 FPS numbers in ROADMAP.md — pick the current truth and anchor to commit hash.

## Completeness Checks

- [ ] **TESTS**: `--bench-cell` flag added, outputs stable format
- [ ] **DOCS**: ROADMAP.md Prospector numbers anchored to commit hash
- [ ] **SIBLING**: Add sweetroll (single-NIF bench) + WastelandNV exterior grid to the same harness
- [ ] **CI**: Add to CI with expected ranges (tolerant to ±20% to avoid flake)

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-5-F3)
