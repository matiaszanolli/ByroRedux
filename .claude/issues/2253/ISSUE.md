# TD3-202: feature-matrix.md's Volumetrics row still says 'content-driven density not wired' and omits MATERIAL_KIND_FIRE_REFRACTION entirely

Severity: medium
Source audit: docs/audits/AUDIT_TECH_DEBT_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2253

**Dimension**: 3 (Stale Documentation & Comments)
**Location**: `docs/feature-matrix.md:47` (Rendering table, Volumetrics row)
**Status**: NEW

**Description**: The row reads "~ Scaffold | Froxel injection + integration shaders shipped; content-driven density not wired." Session 62 (2026-07-26→08-01) wired content-driven density (extinction/chromaticity/peak radiance/coverage from CELL/WTHR) — independently confirmed the same day by `AUDIT_RENDERER_2026-08-02.md`, which recommends closing #2220 on that basis. Separately, `MATERIAL_KIND_FIRE_REFRACTION` (103) shipped in the same window with no row anywhere in the table. Third recurrence of the same feature-matrix-lags-shipped-code pattern (after TD3-101 and TD3-NEW-03 in prior cycles, both fixed).

**Evidence**: `docs/feature-matrix.md:47`; `docs/audits/AUDIT_RENDERER_2026-08-02.md`'s "Confirmed fixed" section; `crates/renderer/src/vulkan/scene_buffer/constants.rs:336: pub const MATERIAL_KIND_FIRE_REFRACTION: u32 = 103;`.

**Impact**: A reader would conclude volumetrics is an unfinished scaffold and there's no fire-refraction path at all — both wrong as of this session. This is the third recurrence of the exact same doc-lag pattern the prior two audits already fixed once each.

**Related**: TD3-101 (closed), TD3-NEW-03 (closed) — same file, third recurrence for a different feature.

**Suggested Fix**: Change the Volumetrics row to reflect fog + local volumes shipped (partial: global fog shipped, local volumes shipped, REGN-driven per-cell density still open); add a `MATERIAL_KIND_FIRE_REFRACTION` row noting its known consistency gaps (tracked #2224/#2236/#2237).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix, if applicable
