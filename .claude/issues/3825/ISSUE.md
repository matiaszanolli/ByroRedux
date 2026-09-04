# Issue #3825: REN-WD-D2-02: audit-renderer/SKILL.md Dim 2 still describes the removed shader-side GLASS_RAY_BUDGET admission gate

**Labels**: low,renderer,documentation,doc-rot
**Filed**: 2026-09-04, via /audit-publish from the water-deep audit suite

---

**Severity**: LOW
**Dimension**: SSBO/Indexing (audit-skill doc-rot)
**Location**: `.claude/commands/audit-renderer/SKILL.md` (Dimension 2, glass/IOR bullet); live code in `crates/renderer/shaders/triangle.frag`, `crates/renderer/src/vulkan/scene_buffer/ray_budget.rs`, `crates/renderer/src/shader_constants.rs`
**Source report**: `docs/audits/AUDIT_RENDERER_2026-09-04.md` (water-deep suite, Dim 2)

## Description
The audit-renderer skill's Dimension 2 checklist says "`GLASS_RAY_BUDGET` (from `shader_constants.glsl`) cap wired; the budget `atomicAdd` overshoots unconditionally by design (#1438)". In the live shader the cap is **not** wired: `GLASS_RAY_BUDGET` is emitted into `shader_constants.glsl` but has no consumer in any `.frag`/`.comp`/`.vert`. Admission is now `qualityTier`-based (`budgetTier = min(rayBudget.qualityTier, 3u)`), the `atomicAdd(rayBudget.rayBudgetCount, glassRayCost)` is telemetry only, and `crates/renderer/src/shader_constants.rs` carries a *negative* assertion (`!src.contains("old + glassRayCost <= rayBudget.glassRayLimit")`) pinning the old gate's absence. `ray_budget.rs` documents `glass_ray_limit` as "retained in the ABI for telemetry".

## Evidence
`grep -rn GLASS_RAY_BUDGET crates/renderer` → definition in `shader_constants_data.rs`, emission in `shader_constants.rs`, derivation of the per-tier `glass_ray_limit` in `ray_budget.rs`, and the `#define` in `include/shader_constants.glsl` — no shader reads it. Confirmed no `.frag`/`.comp`/`.vert` under `crates/renderer/shaders/` references `GLASS_RAY_BUDGET`.

## Impact
Audit-methodology only. An auditor following the checklist looks for a gate that was deliberately deleted and can file a false "the cap regressed" finding, or miss that the real limiter is now `qualityTier`.

## Related
#1438, #2686 (`glass_ray_limit_tiers_derive_from_*`).

## Suggested Fix
Reword the bullet to: "the glass ray budget is `qualityTier`-gated; `GLASS_RAY_BUDGET` only derives the per-tier `glass_ray_limit` telemetry value — verify the negative pin in `shader_constants.rs` still holds".

## Completeness Checks
- [ ] N/A — documentation-only edit to `.claude/commands/audit-renderer/SKILL.md`
