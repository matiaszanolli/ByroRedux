# FNV-D2-04

**Issue**: #3337
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 2 — NIFAL Canonical Translation
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `docs/engine/nifal.md:617-641`

**Premise verified**: §4's table records four observed `emisM` values per
source and concludes *"the three sources already share one ~1.0 scale — no
per-source normalization is required… the legacy `Material` 7.5 is an
authored bright-neon outlier"*. The full FNV census does not match the
"outlier" framing.

**Evidence** (`Fallout - Meshes.bsa`, all 5,670 `EmissiveSource::Material`
meshes; every one of them has a genuinely authored non-zero colour × mult, so
`emissive_contribution_is_authored` filters nothing out):

```
1.00: 1912   2.00: 1454   1.50: 218   1.60: 203   1.20: 161   3.00: 153
10.00: 477   15.00: 37    12.00: 40   30.00: 24   20.00: 11   25.00: 8
40.00: 8     50.00: 5     60.00: 3    100.00: 6
```

≈ **690 meshes (12 %) author `emissive_mult >= 10`**, with a clear secondary
mode at exactly 10.0 (477 meshes) — not a scattered outlier tail.

**Impact**: no *current* visual break — `triangle.frag:1420` clamps
`emissiveColor * emissiveMult * emissiveMask` to `vec3(64.0)`, so the 100×
material is bounded. But that ceiling is a render-time material decision doing
the job the canonical tier explicitly declined to do, and the documented
"resolved as no-op" conclusion would need re-deriving against an equivalent
Skyrim/FO4 census before anyone relies on it. Per the no-guessing policy I am
**not** proposing a normalization constant — the ask is to re-measure, or to
downgrade §4's claim to "sampled, not censused".

**Fix sketch**: re-run the §4 comparison as a full per-game census (the
`material_dump.rs` `emSrc` column already emits what is needed) and either
confirm the shared scale on real distributions or record the divergence.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
