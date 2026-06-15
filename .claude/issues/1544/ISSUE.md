# Issue #1544: OBL-D1-NEW-02: NiPSysEmitterCtlr Visibility Interpolator ref gated on >= V10_2_0_0 but nif.xml says since=10.1.0.104

_Snapshot as filed via /audit-publish from docs/audits/AUDIT_OBLIVION_2026-06-14.md. GitHub is authoritative for current state._

**Severity**: MEDIUM · **Dimension**: 1 (NIF Version Handling) · **Status**: NEW

**Location**: `crates/nif/src/blocks/particle.rs:936-938` (and the identical gate in `parse_multi_target_emitter_ctlr`, `particle.rs:947-949`)

## Description
`parse_emitter_ctlr` reads the `Visibility Interpolator` ref only when `version >= V10_2_0_0`. nif.xml `NiPSysEmitterCtlr.Visibility Interpolator` is `type="Ref" since="10.1.0.104"`. For files in [10.1.0.104, 10.2.0.0) — including the v10.1.0.106 Oblivion FX content — the 4-byte ref is wrongly skipped. (The old `Data` ref `until=10.1.0.103` is correctly not read at ≥10.1.0.104.)

## Evidence
nif.xml `NiPSysEmitterCtlr.Visibility Interpolator since="10.1.0.104"`. `arwelkydclusterfx01.nif` is v10.1.0.106 → in band. Combined with OBL-D1-NEW-01's missing bool, the emitter ctlr under-reads 1 + 4 = 5 bytes, consistent with the observed 4646-byte runaway consume.

## Impact
Compounds OBL-D1-NEW-01 on the same Oblivion FX meshes; no separate file population.

## Related
OBL-D1-NEW-01.

## Suggested Fix
Change the gate to `>= NifVersion::V10_1_0_104`. Also handle the pre-10.1.0.104 `Data` ref (`until=10.1.0.103`) arm and gate the `Interpolator` ref on `>= 10.1.0.104` the way `NiSingleInterpController::parse` already does. Fold into the OBL-D1-NEW-01 fix.

## Completeness Checks
- [ ] **SIBLING**: Both `parse_emitter_ctlr` and `parse_multi_target_emitter_ctlr` gates corrected
- [ ] **CANONICAL-BOUNDARY**: Fix stays at the NIF parser boundary
- [ ] **TESTS**: A regression test pins the v10.1.0.106 emitter-ctlr ref read
