# #588: FO4-DIM4-02: MOVS routed through MODL-only parser — no movable-static physics data captured

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/588
**Labels**: bug, medium, legacy-compat, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 4)
**Severity**: MEDIUM
**Location**: `crates/plugin/src/esm/cell.rs:521` (`b"MOVS"` in the MODL catch-all arm)

## Description

MOVS records describe movable statics (physics-driven havok objects). The match arm at `cell.rs:521` catches MOVS but forwards to `parse_modl_group`, which only extracts EDID, MODL, VMAD, LIGH DATA, ADDN DATA/DNAM. MOVS-specific data — physics kind, collision overrides, motion properties — is dropped.

No `records/movs.rs` file exists. `RecordType` constants at `crates/plugin/src/record.rs:141-146` include `PKIN` but not `MOVS` (nor `SCOL`, though SCOL is handled via raw bytes in `cell.rs`). `EsmIndex` (`records/mod.rs:60-146`) has no `movables` field.

## Impact

Low today (0 vanilla FO4 records per AUDIT_FO4_2026-04-17 H1). Non-zero when DLCs / mods ship MOVS content — those records still register a `StaticObject` with a MODL path so they render as ordinary static decoration, but never participate in physics. Given MOVS's defining feature IS physics, "rendered as static" is an actual-vs-intent mismatch.

## Suggested Fix

Defer until a physics subsystem exists. Pre-work:
1. Add `pub const MOVS: Self = Self(*b"MOVS");` to `RecordType` so the type is addressable.
2. Keep the current MODL fallback so visual placement is preserved.
3. Land `records/movs.rs` scaffold (EDID + MODL + physics placeholder) when physics ECS lands.

## Completeness Checks

- [ ] **UNSAFE**: n/a
- [ ] **SIBLING**: Similar physics-record audit for bhkRigidBody consumption paths
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: Corpus regression test — assert no FO4 DLC archive with MOVS records panics / drops silently.
