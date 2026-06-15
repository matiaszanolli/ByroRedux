# Issue #1551: OBL-D6-NEW-01: Oblivion per-block baseline TSV is stale

_Snapshot as filed via /audit-publish from docs/audits/AUDIT_OBLIVION_2026-06-14.md. GitHub is authoritative for current state._

**Severity**: LOW · **Dimension**: 6 (Real-Data) — tech-debt · **Status**: NEW

**Location**: `crates/nif/tests/per_block_baselines.rs` (Oblivion TSV baseline)

## Description
The checked-in Oblivion baseline still lists 7 formerly-unknown types that now parse (NiPSys* modifiers, NiPSysData, NiStringExtraData) and is missing 7 new clean types (BSKeyframeController, NiCamera, NiPSysEmitter/Ctlr, NiPSysGrowFadeModifier, bhkConvexSweepShape, bhkMeshShape). The test stays green because the compare is asymmetric (fails only on `unknown` growth / `parsed` shrinkage), so the *improvements* are silently tolerated.

## Evidence
`nif_stats --tsv "Oblivion - Meshes.bsa"` diff vs the committed TSV shows only improvements.

## Impact
Baseline no longer reflects ground truth; a future improvement could regress one of these types back to `unknown` and the diff would be muddier to read.

## Suggested Fix
Regenerate with `BYROREDUX_REGEN_BASELINES=1`.

## Completeness Checks
- [ ] **SIBLING**: Other per-game baseline TSVs checked for the same drift while regenerating
- [ ] **TESTS**: Regenerated baseline keeps the per_block gate green
