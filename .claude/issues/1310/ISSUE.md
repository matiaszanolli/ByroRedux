# #1310 -- OBL-D1-01: NiTriStripsData phantom has-points bool at v10.0.1.0

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_OBLIVION_2026-05-28)._

**Severity**: HIGH | **Dim 1** — NIF v20.0.0.5 Parser Correctness
**Source**: `docs/audits/AUDIT_OBLIVION_2026-05-28.md` (OBL-D1-01)

**Location**: `crates/nif/src/blocks/tri_shape/ni_tri_shape.rs:545`

**Issue**: `NiTriStripsData` reads a phantom `has_points` bool using the gate `stream.version() >= NifVersion::V10_0_1_0`. nif.xml has `Has Points` as `since="10.0.1.3"` (nifly `StripsInfo::Sync` also reads it only `>= V10_0_1_3`). At v10.0.1.0 and v10.0.1.2 (early-Gamebryo content shipped inside Oblivion's BSA) a 1-byte field that isn't on disk is consumed, shifting the stream by 1 byte. Because Oblivion has no per-block size table there is no recovery — the entire rest of the NIF truncates.

**Empirical**: patching the gate to `>= V10_0_1_3` and re-running `nif_stats` on the full `Oblivion - Meshes.bsa` reduces truncated scenes from 197 → 175 (22 files recovered, 429 fewer dropped blocks).

**Suggested fix**: change the gate at line 545 to `stream.version() >= NifVersion::V10_0_1_3` (constant already exists in `version.rs:73`). Add a regression test at v10.0.1.2 with `num_strips > 0` (no `has_points` byte on the wire) to pin the boundary.

**Note**: `OBL-D1-02` (NiTriShapeData, same wrong gate at line 483) is the companion bug — fixing both together recovers ~43 Oblivion meshes total.

## Completeness Checks
- [ ] **SIBLING**: `NiTriShapeData` has the identical bug at line 483 — fix both (OBL-D1-02)
- [ ] **TESTS**: regression test at v10.0.1.2 with num_strips>0 (no has_points on wire)
- [ ] **CANONICAL-BOUNDARY**: fix is parse-side only; no material/translate impact
- [ ] **UNSAFE**: no unsafe involved
