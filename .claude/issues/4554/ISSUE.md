# NIFAL-D2-2026-09-21-02: ImportedMesh.tangents doc still describes the Starfield UDEC3 unpack as unbuilt

**Labels**: low, nifal, nif-parser, documentation, doc-rot, game:starfield

**Severity**: LOW · **Dimension**: Geometry/Transform · **Tier Violated**: — (doc rot) · **Game Affected**: Starfield (doc only)
**Location**: `crates/nif/src/import/types.rs:950-953` vs `crates/nif/src/import/mesh/bs_geometry.rs:296-334`
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
The `tangents` field doc ends "`[f32; 4]` is a follow-up to this issue." The unpack has been implemented since #1086/#1232 and is pinned by `bs_geometry_tangent_tests.rs` (counts, values, exact ±1 signs). Only the prose is stale.

### Evidence
`bs_geometry.rs:296-334` produces `Vec<[f32;4]>` from `tangents_raw` via `unpack_udec3_xyzw` + `clamp_sign` (#2246) or `synthesize_tangents_yup` (#1232).

### Impact
A reader concludes Starfield tangents are untranslated and may "fix" a non-gap.

### Related
#1086, #1232, #2246

### Suggested Fix
Replace the stale sentence with the current reality (UDEC3 unpack + `clamp_sign` at `bs_geometry.rs`, synthesis fallback).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **TESTS**: A regression test pins this specific fix
