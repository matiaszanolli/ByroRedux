# EXT-D6-2026-09-27-04: `probe_lod_corpus` still reports false zeros — `TOTAL` ignores the Creation family; Starfield's LOD corpus and Skyrim `.btt` are unmatched

**Issue**: #4933
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,bug,tech-debt

**Severity**: LOW (tooling)
**Dimension**: Distant LOD and trees
**Tier Violated**: no-fabrication
**Game Affected**: Skyrim, FO4, FO76, Starfield
**Status**: NEW (incomplete fix of #4737)
**Location**:
`crates/bsa/examples/probe_lod_corpus.rs:114,180,182`
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- Every run ends `TOTAL … 0` right after counting 10,662 (Skyrim), 8,271 + 802 (FO4) or 3,056 (FO76).
- Starfield's `LODMeshes.ba2` holds 19,535 NIFs under `meshes\lod\generated\` plus 864 `lodsettings\*.lod`, and the probe prints 0.
- The per-worldspace and per-level `.btr`/`.bto` counts themselves are correct.

## Suggested Fix
- Use `total += lod + creation`.
- Add counters for `.btt`, `meshes\lod\generated\` and `lodsettings\*.lod`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **TESTS**: A regression test pins this specific fix
