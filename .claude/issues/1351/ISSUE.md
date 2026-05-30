# #1351 — D6-04: FO4 PreCombined CSG companion reader absent, no ROADMAP milestone

_Snapshot from AUDIT_FO4_2026-05-30. GitHub is authoritative for live state._

**Severity**: MEDIUM · **Source**: AUDIT_FO4_2026-05-30 (D6-04) · **Domain**: performance / legacy-compat

**Location**: `byroredux/src/cell_loader/precombined.rs` (module doc, lines 10–33); `ROADMAP.md` (no CSG entry)

**Description**: Vanilla FO4 `_oc.nif` precombined geometry carries no inline vertex data — the actual vertex and triangle bytes live in `Fallout4 - Geometry.csg`, keyed by `BSPackedGeomObject` filename hash + offset. The Stage A PreCombined-Mesh spawn path (`spawn_precombined_meshes`) correctly extracts and parses `_oc.nif` files, but they contain zero-vertex placeholders, yielding `spawned = 0` for every vanilla FO4 cell.

The honest `pc_spawned == 0 → empty absorbed_refs` REFR-fallback gate means FO4 cells render correctly via per-REFR rendering. But the optimized precombined-architecture path (which Bethesda uses for performance) is never exercised, so the engine always incurs the full per-REFR overhead for FO4.

**Gap**: No ROADMAP milestone tracks the CSG companion reader. It is mentioned only in `precombined.rs` module docs as "deferred (future PreCombined-Geometry milestone)."

**Suggested Fix**: File a CSG milestone. Minimum viable stub:
1. Seek `Fallout4 - Geometry.csg` from the Data BA2 set
2. Parse `BSPackedGeomObject` TLV records keyed by (filename-hash, offset) pairs extracted from the `_oc.nif` `BSGeometrySegment` / `BSPackedGeomDataCombined` blocks
3. Feed vertex/index buffers into the existing `spawn_precombined_meshes` pipeline

The `precombined.rs` module doc already has a detailed stub plan at lines 14–33.

## Completeness Checks
- [ ] **SIBLING**: The companion `_precomb.nif` collision geometry (Havok) is also unread — should be part of the same milestone
- [ ] **TESTS**: Add a golden-frame or entity-count assertion for a known FO4 cell that verifies `pc_spawned > 0` once the CSG reader lands
