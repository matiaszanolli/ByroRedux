# Issue #1280: Canonical material convergence — finish steps 3b/3c/4 (#1277 Workstream A)

**State**: OPEN
**Labels**: enhancement, renderer, import-pipeline, high

## Body

**Child of #1277 — Workstream A (canonical material convergence).**

Finish the in-progress canonical-material work documented at [docs/engine/material-abstraction.md](../blob/main/docs/engine/material-abstraction.md). The infrastructure pieces (#1277 Task 5 helpers, #1277 Task 6 `ShaderFlags` typed view) landed this session; the **single biggest user-visible Fallout-looks-matte-plastic cause** — the FNV `classify_pbr_keyword` collapse to `metalness=0 / roughness=0.8` — is the remaining surgery.

## The smoking gun (from material-abstraction.md §3a)

Equivalent surfaces, real engine pipeline:

| Surface | Game | source | metalness | roughness | glossiness |
|---|---|---|---:|---:|---:|
| metal (door) | FO4 | `institutemetal01.bgsm` | **0.79** | **0.04** | 100 |
| floor | FO4 | `institutefloor02d.bgsm` | 0.69 | 0.10 | 90 |
| wall | FNV | keyword | **0.00** | **0.80** | 10 |
| glass bottle | FNV | keyword | **0.00** | **0.80** | 50 |

FNV's keyword classifier collapses *every* surface to `metalness 0.00 / roughness 0.80` — metal renders as matte plastic, glass as rough plastic. FO4 BGSM gives real per-material PBR. Same shader fed these two conventions produces the "different stages of development" look the original epic is aimed at.

## Status of the convergence plan (material-abstraction.md §4)

| Step | Status |
|---|---|
| 1. Ground-truth audit | DONE (material_dump example, table in §3a) |
| 2. Canonical PBR at parse — env-arm REVERTED | DONE (regression test `classify_pbr_neutral_envmap_default_clamps_matte_not_chrome` pins) |
| 3. Parse-time glass — alpha-aware | DONE (legacy path); FO4 BGSM transparency signal still pending |
| 4. Emissive-mult scale unification | PENDING |
| 5. Ambient model unification | PENDING (optional) |

## Concrete deliverables

- [ ] **Sub-step 3b**: FO4 BGSM glass classification. A BGSM-authored glass bottle with no keyword in texture/name doesn't classify today — needs the BGSM transparency signal plumbed through `merge_bgsm_into_mesh`.
- [ ] **Sub-step 3c**: delete the now-subsumed render-side glass heuristic in `byroredux/src/render/static_meshes.rs:372` (spawn is a superset). Currently kept as a defensive fallback.
- [ ] **Step 4**: emissive-scalar unification across the three sources documented in survey §4.2 finding 9 — `BSEffect.base_color_scale` (FO4+, diffuse tint conflated as emissive), `BSLightingShaderProperty.emissive_multiple` (Skyrim+), `NiMaterialProperty.emissive_mult` (legacy). Same `info.emissive_mult` slot, three semantics. Add `MaterialInfo.emissive_source: EmissiveSource` discriminator so consumers can tell what kind they're getting.
- [ ] **Verification**: Task 8 harness `metO%` / `rghO%` fill-rates should still be 100% (they already are) but the underlying VALUES should be more varied for FNV — extend the harness with per-game `metalness_distribution` / `roughness_distribution` (e.g. count of meshes with `roughness > 0.7` per game). A FNV convergence pass should drop FNV's "matte 0.8 collapse" rate from ~90% to whatever the natural distribution of materials is.

## Open questions (material-abstraction.md §5)

- **Q1**: Glass authoritative signal for legacy — is `NiAlphaProperty.blend + low NiMaterialProperty.alpha` sufficient, or do we still need a parse-time keyword tiebreaker?
- **Q2**: `emissive_mult` scale — FO4 / legacy / Skyrim may not share a scale. Tabulate before unifying.
- **Q3**: Does BGSM-less FO4 loading (materials BA2 absent) need an explicit "PBR unavailable" path, or is keyword-fallback acceptable as a documented degraded mode?

## References

- Parent epic: #1277
- Design doc: [docs/engine/material-abstraction.md](../blob/main/docs/engine/material-abstraction.md)
- Survey identifying Leak B as the dominant Fallout-vs-Skyrim divergence: [docs/engine/per-game-translation-survey.md §4.2](../blob/main/docs/engine/per-game-translation-survey.md#42-nif-importer-cratesnifsrcimport) finding 8
- Infrastructure landed this session: #1277 Task 5 (helpers), #1277 Task 6 (`ShaderFlags` enum)
- Verification harness: #1277 Task 8
