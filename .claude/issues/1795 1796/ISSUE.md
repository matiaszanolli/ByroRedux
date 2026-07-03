# #1795: D2-NEW-02: Per-particle unquantized LERPed color defeats MaterialTable dedup

**Severity**: MEDIUM
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (D2-NEW-02)
**Labels**: bug, renderer, medium, performance
**State**: OPEN

## Location
`byroredux/src/render/particles.rs:74-82,138-139,208-209`; `crates/renderer/src/vulkan/context/mod.rs:509,521-523` (`material_hash` over raw `emissive_*` bits)

## Description
Each particle's color is LERPed against `t = age/life` (unquantized, `particles.rs:74`) and folded into `emissive_color`/`emissive_mult` (`:138-139`), both material-table fields post-R1. `material_hash` hashes `emissive_mult.to_bits()` + `emissive_color[..].to_bits()` — raw f32 bits, zero quantization (`context/mod.rs:521-523`) — so `intern_by_hash` (`:208`) takes the miss path per particle every frame: full `GpuMaterial` build + FxHashMap insert + table push + upload, inverting the ~97% dedup-hit rate the #781 fast path assumes. Instancing is unaffected (`material_id` is per-instance); this is the residual after #1649 fixed the depth-vs-mesh sort ordering.

## Evidence
`particles.rs:74` continuous `t`; `:138-139` emissive writes; `:208` intern; `context/mod.rs:521-523` raw-bits hash covers the emissive fields, so distinct colors never dedup.

## Impact
Scales with live particle count. FX-heavy scenes (20-30 emitters, 96-256 particle caps each) can reach ~5-8K unique materials/frame ≈ 1.5-2.3 MB/frame upload plus CPU churn, stacking toward the `MAX_MATERIALS = 16384` cap where overflow silently routes particles to neutral material id 0 (wrong color). Also permanently depresses dedup-ratio telemetry, masking real dedup regressions elsewhere.

## Related
#1649, #781, #780, #797.

## Suggested Fix
Quantize the fade parameter before the color LERP (e.g. 32 steps — imperceptible on additive billboards). Same-emitter particles then collapse to ≤32 materials.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other hot-path loops / other dirty gates)
- [ ] **TESTS**: A regression test pins this specific fix

---

# #1796: D6-02: Pose hash committed at build_render_data time — draw_frame early return freezes RT skinned pose while dirty gate reads "clean"

**Severity**: MEDIUM
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (D6-02)
**Labels**: bug, renderer, medium, vulkan, performance
**State**: OPEN

## Location
`byroredux/src/render/skinned.rs:152,180` (clear + mark = the hash commit) + `crates/renderer/src/vulkan/context/draw.rs:2118` (early return) + `draw.rs:1711-1715` (consumer skip gate)

## Description
`try_mark_pose_dirty(entity, hash)` records the new pose hash the moment `build_skinned_palettes` runs — CPU-side, in `build_render_data`, before `draw_frame`. The commit is `render/skinned.rs:180` (into `last_pose_hash` on the ECS `SkinSlotPool`, `resources.rs:949-966`); `draw.rs:1711` is the consumer (`pose_dirty.contains(entity)` skip gate), and there is no `last_pose_hash` write anywhere in `crates/renderer/src`. If `draw_frame` early-returns before the skin dispatch (swapchain out-of-date @2118, empty framebuffers), the dispatch never runs but the hash baseline has already advanced. Sequence: frame N-1 dispatches pose P1 (H1); frame N computes P2 → H2 recorded dirty; `draw_frame` N early-returns; frame N+1 the NPC stops → H2 matches stored H2 → gate reads "not dirty" → dispatch + refit skipped with `has_populated_output == true`. The slot output and skinned BLAS stay at P1 while the raster palette (recomputed every frame from `bone_world`) shows P2.

## Evidence
`skinned.rs:180` runs unconditionally in `build_render_data`; grep for `last_pose_hash` in the renderer crate = 0 hits, so nothing rolls it back when `draw_frame` fails to reach the skin section.

## Impact
RT shadows/reflections/GI of the affected NPC freeze at a pose one-plus frames stale relative to the rasterized body, persisting through the idle period after the lost frame. Self-healing on next movement; no crash, no leak.

## Related
D6-01 (same root cause), #1195.

## Suggested Fix
Same transactional shape as D6-01 — stage the frame's pose hashes and fold into `last_pose_hash` only after `draw_frame` confirms the skin section ran, or re-insert the frame's dirty set on early-return paths.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
