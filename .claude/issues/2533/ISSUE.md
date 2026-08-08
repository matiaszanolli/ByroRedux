# NIFAL-D8-NEW-01: BGEM v21+/v22 glass-overlay texture paths have no MaterialTextureSet role -- undocumented in nifal.md's texture-roles section

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2533
**Finding ID**: NIFAL-D8-NEW-01

**Severity**: LOW
**Dimension**: Shader-flags/Effects · **Tier Violated**: no-leak (doc-completeness only — the code-level gap is already deliberately deferred, not a live bug)
**Game Affected**: FO76/Starfield-era BGEM content (mod-added; `bgem_uses_glass_behavior` gate)
**Location**: `byroredux/src/asset_provider/material.rs:1271-1282`; `crates/bgsm/src/bgem.rs:32-43`; missing from `docs/engine/nifal.md`'s "Shader flags / texture sets / effect shaders" section
**Status**: Existing: **#2109** (CLOSED, code-comment-documented) — this finding is narrower: the code-site comment is accurate and complete, but `nifal.md`'s dedicated deferred/parked-passthrough tables have no entry for it, even though two of the six BGEM fields (`glass_roughness_scratch`, `glass_dirt_overlay`) are texture paths, not scalars, and belong conceptually next to the `MaterialTextureSet<T>` role inventory this dimension audits.

## Description
`BGEM` (v21+/v22) decodes `glass_fresnel_color`, `glass_refraction_scale_base`, `glass_blur_scale_base`, `glass_blur_scale_factor`, `glass_roughness_scratch` (String texture path), `glass_dirt_overlay` (String texture path), and `environment_mapping_mask_scale`. All six decode correctly but none reach `ImportedMesh`/`ImportedMaterial`/`MaterialTextureSet<T>` — no 19th/20th named role exists for them the way `tint`/`inner_layer`/`reflectance` were added in a prior texture-role unification. The asset-provider comment is honest about this being deferred, but the spec doc's own texture-role inventory doesn't mention the gap.

## Evidence
Confirmed directly: `bgem.rs:32-33,132-133` decode both texture-path fields; `material.rs:1271-1282` carries an honest deferred comment; `docs/engine/nifal.md`'s `MaterialTextureSet<T>` role-inventory section (line ~328) has no entry naming either field.

## Impact
None beyond documentation completeness — intentionally deferred per `#2109`'s own resolution, reachability on real content already flagged as low/unmeasured there.

## Suggested Fix
Add a one-line entry to `nifal.md`'s texture-roles/Passthroughs table naming `glass_roughness_scratch`/`glass_dirt_overlay` as parsed-but-unrouted BGEM texture paths, blocked on a renderer glass-overlay consumer — mirroring the existing `bs_lod_cutoffs`/`BSInvMarker` table-row format. Doc-only.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
