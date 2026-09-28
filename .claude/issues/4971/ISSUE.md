# #4971: REN-D10-2026-09-27-02: Froxel local-light budget is spent in cluster-list order, and the directional light consumes a slot

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4971
- **Labels**: low,renderer,shaders,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D10-2026-09-27-02**._

- **Severity**: LOW
- **Dimension**: Volumetrics (cross-dimension with Dim 13; the root cause is cluster-list composition)
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp`: `uint lightLoopCount = min(cluster.count, adaptiveLightCap);` (L3054), then `if (lightType > 1.5) { continue; }` (L3061). The list comes from `cluster_cull.comp` `main()` (atomic append, directional `intersects = true` at L259). The cap comes from `AdaptiveRayBudget::settings_for_tier` (`scene_buffer/ray_budget.rs`: `volumetric_light_cap` = 2/4/6/8 for tiers 0–3).
- **Status**: NEW. Pre-existing since `5798e4672` (2026-08-09); no issue found.
- **Description**: The froxel loop takes the first `adaptiveLightCap` entries of the fragment cluster's list and then skips the directional among them. Two problems follow:
  1. The directional is in every cluster's list, so whenever `cluster.count > cap` and the directional lands in the first `cap` entries, one of the local-light slots is spent on a light the loop discards. At tier 0 (cap 2) that halves the local in-scatter budget.
  2. The list order is `atomicAdd` order across the 32 lanes of `cluster_cull`, which is implementation-defined. The retained subset is therefore not priority-based. In practice it tends to follow light index, i.e. the `gi_priority_score` order, which makes the CPU sort silently load-bearing for fog, but no contract says so.

  `caustic_splat.comp` already solves the same problem correctly: its loop charges `budgetedLights < maxLights` only for lights that pass its rejection (L420).
- **Evidence**: see Location.
- **Impact**: In dense-light clusters, lamp in-scatter in fog, smoke and fire media uses fewer local lights than the tier grants, and which lights it uses is arbitrary. Visual only.
- **Related**: Dim 13 volumetrics; PERF-D5 #4807 (density caps; different).
- **Suggested Fix**: Iterate `cluster.count` and count only accepted local lights against `adaptiveLightCap`, as `caustic_splat` does. If the subset should be deterministic, state that cluster lists are unordered and select by score inside the loop.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
