# #3879: TD7-2026-09-05-01: Five of the six `GpuRayBudget` policy ceilings are hand-retyped as shader loop bounds — only `glass_ray_limit` was ever derived

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD7-2026-09-05-01) via `/audit-publish`, 2026-09-05. Labels: `low,renderer,shaders,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3879 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD7-2026-09-05-01), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 7 — Magic Numbers & Hardcoded Constants
- **Location**: `crates/renderer/src/vulkan/scene_buffer/ray_budget.rs::AdaptiveRayBudget::settings_for_tier` · `crates/renderer/shaders/triangle.frag` (the `MAX_SHADOW_RAYS`, `MAX_PATH_SEGMENTS`, `MAX_SHADED_HITS`, `MAX_REFRACT_PASSTHRUS` declarations) · `crates/renderer/shaders/volumetrics_inject.comp` (the `MAX_FROXEL_LIGHTS` declaration)
- **Status**: NEW
- **Effort**: small (≤2 h)
- **Description**: `GpuRayBudget` is a seven-field CPU→GPU quality contract. `settings_for_tier` picks the per-tier values on the Rust side; each consuming shader then needs a *compile-time* ceiling for its bounded loop, which must be ≥ the tier-3 value. Today that ceiling is an independently hand-typed GLSL literal for every field except `glass_ray_limit`, which #2686 / SAFE-D7-01 fixed by deriving all four tiers from `GLASS_RAY_BUDGET`. The sibling fields never got the same treatment, so the tier table and its shader-side ceilings are five pairs of literals that only happen to match.

  Worse, the runtime clamp *repeats* the ceiling a second time in the same expression (`clamp(rayBudget.directShadowSamples, 1u, 8u)` sits one line below `const int MAX_SHADOW_RAYS = 8;`), so each field is written out two or three times.

- **Evidence**: current tier-3 (`_ =>`) arm vs the shader literals that must track it:

  | `GpuRayBudget` field | tier-3 value (`ray_budget.rs`) | shader-side ceiling | second copy (runtime clamp) |
  |---|---|---|---|
  | `glass_ray_limit` | `GLASS_RAY_BUDGET` | *derived* — #2686 | n/a |
  | `direct_shadow_samples` | `8` | `triangle.frag`: `const int MAX_SHADOW_RAYS = 8;` | `clamp(rayBudget.directShadowSamples, 1u, 8u)` |
  | `max_path_segments` | `6` | `triangle.frag`: `const int MAX_PATH_SEGMENTS = 6;` | `clamp(rayBudget.maxPathSegments, 1u, 6u)` |
  | `max_shaded_hits` | `2` | `triangle.frag`: `const int MAX_SHADED_HITS = 2;` | `clamp(rayBudget.maxShadedHits, 1u, 2u)` |
  | `volumetric_light_cap` | `8` | `volumetrics_inject.comp`: `const uint MAX_FROXEL_LIGHTS = 8u;` | `clamp(…, 1.0, float(MAX_FROXEL_LIGHTS))` |
  | `quality_tier` (max) | `3` (`observe`'s `self.tier < 3`, the `_ =>` arm) | `triangle.frag`: `min(rayBudget.qualityTier, 3u)`, and `const int MAX_REFRACT_PASSTHRUS = 8;` — which is `2 + 3 * 2`, i.e. the tier count baked into a *second* independent literal |

  `volumetric_light_cap` reaches the shader as a float through `fog_reference[2]` in `context/post_passes.rs`, which reads `current_ray_budget(...).volumetric_light_cap as f32`. No file is `#include`ing a shared definition for any of these.

  The existing guards do not close the loop. `shader_contract_tests.rs::bounded_path_preserves_the_accepted_segment_and_diffuse_budgets` and `gi_zero_budget_is_a_true_no_ray_floor` assert the *shader source text* (`frag.contains("const int MAX_PATH_SEGMENTS = 6;")`, `frag.contains("clamp(rayBudget.maxPathSegments, 1u, 6u)")`) — neither ever names `AdaptiveRayBudget::settings_for_tier`. `MAX_SHADOW_RAYS` and `MAX_FROXEL_LIGHTS` have no test at all.

- **Impact**: raising a tier-3 value in `ray_budget.rs` is a **silent no-op**: the shader clamps the uploaded value back down to its own hardcoded ceiling and the whole suite stays green (the string-matching tests only fire if the *shader* changes). The reverse edit is equally invisible. No memory-safety consequence — every loop is `for (…; i < CEILING; …) { if (i >= runtime_limit) break; }` with no array indexed by the counter, so this is **not** the HIGH over/underflow trigger. Blast radius is a tuning pass that appears to do nothing on a GPU-adaptive quality ladder, which is exactly the failure #2686 was filed for.
- **Related**: #2686 / SAFE-D7-01 (the one field that was fixed) · #2265 / TD7-001 (same defect class, three GLSL files) · #2045 / TD7-101 · #3745 / TD7-2026-08-30-01 · TD7-2026-09-05-02 below (the gate that should have caught this cannot see function-local `const`s)
- **Suggested Fix**: add `MAX_DIRECT_SHADOW_SAMPLES`, `MAX_PATH_SEGMENTS`, `MAX_SHADED_HITS`, `MAX_FROXEL_LIGHTS` and `MAX_RAY_QUALITY_TIER` to `crates/renderer/src/shader_constants_data.rs`; have `settings_for_tier`'s `_ =>` arm use them (exactly as it already uses `GLASS_RAY_BUDGET`) and have `triangle.frag` / `volumetrics_inject.comp` take them from the `#include`d header instead of declaring locals. Then replace the string-matching assertions with the relationship that actually matters — `settings_for_tier(3).max_path_segments == MAX_PATH_SEGMENTS`, etc.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
