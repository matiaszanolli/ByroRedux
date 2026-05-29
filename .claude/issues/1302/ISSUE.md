# #1302 -- OBL-D1-03: NiGeomMorpherController unconditional MorphWeight float

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_OBLIVION_2026-05-28)._

**Severity**: HIGH | **Dim 1** — NIF v20.0.0.5 Parser Correctness
**Source**: `docs/audits/AUDIT_OBLIVION_2026-05-28.md` (OBL-D1-03)

**Location**: `crates/nif/src/blocks/controller/morph.rs:37-45`

**Issue**: `NiGeomMorpherController` reads 8-byte MorphWeight elements (interpolator_ref + weight f32) unconditionally. For `v < 20.1.0.3` (which includes all mainstream Oblivion v20.0.0.4/v20.0.0.5 content) there is no per-element float weight — only the interpolator ref. The parser consumes a spurious 4-byte f32 per morph entry, misassigning interpolator refs and reading garbage weights. Affects facial morphs and animated gates.

**Suggested fix**: gate the per-element `weight = stream.read_f32_le()?` on `stream.version() >= NifVersion::V20_1_0_3` (constant exists, `version.rs:121`); else use `weight = 1.0`. Extend the existing `nigeommorpher_oblivion_*` test to pin the boundary.

## Completeness Checks
- [ ] **SIBLING**: check other NiInterpolatorWeight-reading parsers for the same version gating
- [ ] **TESTS**: test at v20.0.0.5 / v20.0.0.4 with real morph count (no per-element float on wire)
- [ ] **CANONICAL-BOUNDARY**: parse-side only
- [ ] **UNSAFE**: no unsafe involved
