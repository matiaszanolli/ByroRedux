# EXT-D6-2026-09-27-05: #4736 left two census citations stale

**Issue**: #4934
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,documentation,doc-rot,game:fo76

**Severity**: LOW (doc rot)
**Dimension**: Distant LOD and trees
**Tier Violated**: no-fabrication
**Game Affected**: FO76
**Status**: NEW (incomplete part of #4736)
**Location**:
`byroredux/src/cell_loader/lod_bands.rs:182-183`; `byroredux/src/cell_loader/object_lod.rs:1177`
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
The comment's first line still lists the retired level set, and the correction is only appended. The test message is byte-identical to the baseline's and still cites #4488.

## Suggested Fix
Say "L4/8/16/32 across GeneratedMeshes01/02", and point the test message at #4736.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **TESTS**: Doc text matches the code it describes (re-grep the cited symbols)
