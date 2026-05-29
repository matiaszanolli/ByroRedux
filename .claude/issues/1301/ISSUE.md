# #1301 -- OBL-D1-02: NiTriShapeData phantom has-triangles bool at v10.0.1.0

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_OBLIVION_2026-05-28)._

**Severity**: HIGH | **Dim 1** — NIF v20.0.0.5 Parser Correctness
**Source**: `docs/audits/AUDIT_OBLIVION_2026-05-28.md` (OBL-D1-02)

**Location**: `crates/nif/src/blocks/tri_shape/ni_tri_shape.rs:483`

**Issue**: `NiTriShapeData` reads a phantom `has_triangles` bool using the gate `stream.version() >= NifVersion::V10_0_1_0`. nif.xml has `Triangles` as unconditional `until="10.0.1.2"` and cond-gated `since="10.0.1.3"`. OpenMW `data.cpp:182` reads the bool only `> VER_OB_OLD` (= v10.0.1.2), i.e. exactly `>= V10_0_1_3`. At v10.0.1.0/10.0.1.2 a phantom 1-byte read misaligns the triangle list and num_match_groups; with no Oblivion block-size table the NIF tail truncates.

**Empirical**: this fix alone recovers a further 21 Oblivion meshes (truncated 175 → 154; companion to OBL-D1-01 which recovers 22).

**Suggested fix**: change the gate at line 483 to `stream.version() >= NifVersion::V10_0_1_3`.

## Completeness Checks
- [ ] **SIBLING**: `NiTriStripsData` has the identical bug at line 545 — fix together (OBL-D1-01)
- [ ] **TESTS**: regression test at v10.0.1.2 / v10.0.1.0 with real triangle data (no has_triangles bool on wire)
- [ ] **CANONICAL-BOUNDARY**: parse-side only; no material/translate impact
- [ ] **UNSAFE**: no unsafe involved
