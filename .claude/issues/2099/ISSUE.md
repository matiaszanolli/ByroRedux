# SF2D2-02: Secondary UV channel (uvs1) parsed then dropped by the importer

**Severity**: LOW
**Labels**: low, nif-parser, enhancement
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:160`
**Source audit**: `docs/audits/AUDIT_STARFIELD_2026-07-16.md` (SF2D2-02)

## Description
`BSGeometryMeshData` decodes both `uvs0` and `uvs1`, but `extract_bs_geometry` only consumes `uvs0`. Starfield uses the second UV set for detail/decal and some shader layering; affected content renders with the primary UV set only. Not actionable until `Vertex`/`ImportedMesh` grow a second UV slot.

## Suggested Fix
Track as an enhancement; thread `uvs1` through once the vertex format supports a second UV channel.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
