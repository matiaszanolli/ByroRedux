# NIFAL-D5-2026-09-21-02: Legacy NiPSysEmitterCtlrData rate tier is still a whole-scene first-match

**Labels**: low, nifal, nif-parser, bug

**Severity**: LOW · **Dimension**: Particles · **Tier Violated**: no-fabrication (misattributed authored rate, legacy tier) · **Game Affected**: pre-10.2 Gamebryo content only (measured 0 files on FNV)
**Location**: `crates/nif/src/import/walk/emitter.rs:452-458` (documented residual), `:702-708` (the scan)
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
The final tier of `extract_emitter_rate` scans the whole scene for the first `NiPSysEmitterCtlrData` and applies its first birth-rate key to *this* system — the same whole-scene first-match class #4261 eliminated and #4467 repaired, left on the deprecated legacy tier. Documented in-code as a deliberate #4261 residual, never issue-tracked.

### Evidence
`scene.blocks.iter().find_map(… downcast_ref::<NiPSysEmitterCtlrData>() …)` with no per-instance key, in contrast to the own-chain + `target_ref` fallback directly above it.

### Impact
Minimal today (block deprecated pre-10.2, census-measured absent on FNV — 0 files); live only for old non-Bethesda Gamebryo FX with multiple emitters, where rates would cross-attribute.

### Related
#4261, #4467

### Suggested Fix
If a corpus measurement ever shows target-game incidence, link the legacy block through its owning `NiParticleSystemController` → target particle system; otherwise leave with the existing comment + this issue as the tracking record.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **CANONICAL-BOUNDARY**: The fix keeps per-game logic at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
