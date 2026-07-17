# SF2D2-01: BSGeometry block bounding-sphere scale not cross-checked against havok-scaled vertices

**Severity**: LOW (low-confidence, needs real-data spot-check)
**Labels**: low, nif-parser, legacy-compat, bug
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:233-249`
**Source audit**: `docs/audits/AUDIT_STARFIELD_2026-07-16.md` (SF2D2-01)

## Description
The block's raw `bounding_sphere` is used verbatim as the mesh's local bound whenever `radius > 0`, with no cross-check that it's expressed in the same havok-scaled units as the decoded vertices. If units diverge, the bound could be ~70x too small, causing off-axis culling pop. Cydonia renders correctly today, which is evidence against a gross mismatch — hence LOW and flagged as needs-verification rather than confirmed.

## Suggested Fix
Add a debug-only sanity check comparing the sphere radius against the actual max vertex extent (mirroring the existing `bs_geometry_hint_mismatch` pattern); spot-check one vanilla Cydonia `.mesh`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
