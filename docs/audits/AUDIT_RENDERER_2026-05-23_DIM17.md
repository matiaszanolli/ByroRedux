# Renderer Audit — 2026-05-23, Dim 17 focus

**Focus**: `--focus 17` (Water Rendering — M38).
**Depth**: deep.
**Trigger**: 4-day re-verification sweep against the 2026-05-19 audit baseline. Worth re-walking because the M40 streaming push (#1199, doorteleport plumbing, BSEffectShader LUT, FO4-D6-003 Phase 2b) touched adjacent surfaces; this audit confirms none of those changes regressed the water dimension.
**Prior base**: `AUDIT_RENDERER_2026-05-19_DIM17.md` (1 LOW finding + 5 INFO verifications + 1 tracking note; only finding was #1210 — water-side caustics deferred to `water.frag` are unimplemented and untracked).
**Open Dim 17 issues at audit start**: #1210 (REN-DIM17-01) — still OPEN.

## Executive Summary

| Severity | Count | Status |
|----------|-------|--------|
| CRITICAL | 0 | — |
| HIGH     | 0 | — |
| MEDIUM   | 0 | — |
| LOW      | 1 | Existing: #1210 (carry-over, re-verified live) |
| **Total** | **1** | 1 carry-over, 0 new |

**Pipeline areas affected**: none — the dimension is stable.

**Headline**: Dim 17 is healthy. Every closed F-WAT-* finding (#1014 / #1015 / #1024 / #1025 / #1036 / #1067 / #1068 / #1069 / #1070 / #1071 / #1087 / #1110 / #1129) stayed closed. The only remaining gap is the water-side caustic synthesis still deferred to `water.frag` (`caustic_splat.comp` explicitly drops underwater caustics per M38's architectural split, and the water shader is the planned home — but neither implementation nor tracking-only successor has landed). That gap is already tracked by #1210. No new findings.

## Changes since 2026-05-19 audit

Per `git log --since="2026-05-19"` for the Dim 17 file set:
- `crates/renderer/src/vulkan/water.rs` — 3 lines added in `fe22e64c` (#1147 / FO4-D6-003 Phase 2b: test-fixture `DrawCommand` initialiser gains three translucency fields). 1 line added in `7eb137b5` (#890 Stage 2c: another test-fixture field for the greyscale palette LUT). No production code change.
- `crates/renderer/shaders/water.{vert,frag}` — untouched.
- `byroredux/src/cell_loader/water.rs` — untouched.
- `byroredux/src/systems/water.rs` — untouched.
- `byroredux/src/components.rs` — touched by M40 door-teleport / #890 / #1199, but the `Water*` / `SubmersionState` components themselves were not redefined.

The water dimension's *behaviour* is byte-identical to the 2026-05-19 baseline.

## Checklist walk

| Checklist item | Status | Evidence |
|----------------|--------|----------|
| `WaterPlane` spawned from XCWT / cell water records | ✅ | [cell_loader/water.rs:215-254](../../byroredux/src/cell_loader/water.rs#L215-L254) — `WaterPlane + WaterVolume + WaterFlow` inserted in lockstep |
| Vertex displacement bounded, no NaN, no Z-fighting | ✅ | Stability clamp (#1025) at [water.frag:398-405](../../crates/renderer/shaders/water.frag#L398-L405) — full half-space projection toward `N` |
| Fresnel: Schlick with base reflectance ~0.02 (not glass IOR) | ✅ | [water.frag:407-410](../../crates/renderer/shaders/water.frag#L407-L410) — `F0 = push.misc.x`, sourced from `mat.fresnel_f0` |
| RT reflection: sky tint miss fallback | ✅ | [water.frag:415-423](../../crates/renderer/shaders/water.frag#L415-L423) — `traceWaterRay(..., skyTint.xyz, ...)`; miss → blend toward `skyTint` |
| RT refraction: deep-water tint miss fallback (#1015) | ✅ | [water.frag:438](../../crates/renderer/shaders/water.frag#L438) — `traceWaterRay(..., push.deep.rgb, ...)`; passes Beer-Lambert with `hitDist` |
| `submersion_system` no per-frame strobe at boundary | ✅ | [systems/water.rs:71-92](../../byroredux/src/systems/water.rs#L71-L92) — strict-inequality AABB, `head_submerged = depth > 0.0` (boundary is the > side); closest-depth tiebreaker for nested volumes |
| Cell unload: water entities despawn cleanly, no leaked BLAS | ✅ | [cell_loader/water.rs:166-181](../../byroredux/src/cell_loader/water.rs#L166-L181) — `rt_enabled = false` at upload → no BLAS exists to leak; generic despawn loop at [unload.rs:244-248](../../byroredux/src/cell_loader/unload.rs#L244-L248) |
| Shadow casting: water excluded from opaque shadow rays | ✅ | [acceleration/predicates.rs:362](../../crates/renderer/src/vulkan/acceleration/predicates.rs#L362) — `draw_cmd.in_tlas && !draw_cmd.is_water` |
| Two-sided: dynamic CULL_MODE (not pipeline duplicate) | ✅ | [water.rs:188-190 + :361-378](../../crates/renderer/src/vulkan/water.rs#L188-L190) — `DynamicState::CULL_MODE` declared (#1071), caller emits `cmd_set_cull_mode(NONE)` |
| Sort key: water in transparent cluster, after opaques | ✅ | [render/mod.rs:166-178](../../byroredux/src/render/mod.rs#L166-L178) — `alpha_blend` cluster `1u8`, water re-emit at [render/water.rs:60](../../byroredux/src/render/water.rs#L60) sets `is_water = true` AFTER the sort |
| Material slot: water distinct from glass | ✅ | `is_water` is a separate `DrawCommand` field, not a material_kind value; #1067 confirms no GpuMaterial collapse risk between them |
| Water-side caustic implementation status | ❌ | `water.frag` grep for `imageAtomicAdd` / `caustic` returns zero call sites. Tracked by **open #1210** (REN-DIM17-01). See finding below. |

## Findings

### LOW

#### REN-DIM17-01 (carry-over): water-side caustics deferred to water.frag are unimplemented and untracked (was untracked; now tracked by #1210)

- **Severity**: LOW (carry-over)
- **Dimension**: Water (M38) / Caustic Splat split-of-responsibility
- **Location**: [crates/renderer/shaders/water.frag](../../crates/renderer/shaders/water.frag) (entire file — no caustic synthesis); [crates/renderer/shaders/caustic_splat.comp:199-220](../../crates/renderer/shaders/caustic_splat.comp#L199-L220) (architectural split that defers to water.frag)
- **Status**: Existing: #1210 (still OPEN; re-verified live in this audit)
- **Description**: The M38 architectural split routes glass + MultiLayerParallax refractive caustics through `caustic_splat.comp` and explicitly defers water-side caustics ("the water-side caustic is the water shader's responsibility (M38)" — caustic_splat.comp:213-215). `water.frag` does not implement caustic synthesis: grep for `caustic`, `Caustic`, or `imageAtomicAdd` in the file returns zero call sites (only a single comment mention pointing back at the caustic_splat sibling). The result is that no caustic light appears under water surfaces from the sun shining onto a wavy pool's floor.
- **Impact**: Visual fidelity gap in any cell with a water plane lit by direct lighting (most exterior cells with sun, interior baths / fonts with point lights). The water surface produces correct reflection / refraction at the surface itself; what's missing is the focused-light pattern on the surface BELOW. Subtle on overcast / interior content; conspicuous on bright exteriors. Not a correctness regression — the engine simply doesn't ship this feature yet.
- **Status verification**: `gh issue view 1210` confirms OPEN; no PRs in flight as of audit date.
- **Suggested Fix**: Implement the water-side splat in `water.frag` along one of two paths:
  - **Per-fragment** (cheap, no compute pass): inside the regular water fragment shader, after refraction, fire one extra ray downward through the water column toward the sun; on hit, modulate the resulting deep-water color by a focused-light factor (similar to glass-caustic intensity).
  - **Compute pass parallel to caustic_splat.comp** (more accurate but doubles the compute cost): a dedicated water-caustic accumulator with light-side ray firing, composite added like the glass caustic.
  
  The current finding doesn't dictate which — leaves the choice to whoever picks up #1210.

## Did-not-find (negative coverage)

- **#1014 / F-WAT-01** (refract sign inversion) — still correct at [water.frag:413](../../crates/renderer/shaders/water.frag#L413) and [:429](../../crates/renderer/shaders/water.frag#L429): `reflect(-V, ...)` and `refract(-V, ...)` with consistent incident-vector sign.
- **#1015 / F-WAT-02** (refraction miss paints sky tint) — fixed and verified at [water.frag:434-438](../../crates/renderer/shaders/water.frag#L434-L438): miss-fallback is `push.deep.rgb`, NOT `skyTint`. The reflection-miss correctly stays on `skyTint.xyz`.
- **#1024 / F-WAT-03** (TLAS self-hits via water in BLAS) — `is_water` is the load-bearing TLAS gate at [predicates.rs:362](../../crates/renderer/src/vulkan/acceleration/predicates.rs#L362) and unit-tested at [acceleration/tests.rs:113-120](../../crates/renderer/src/vulkan/acceleration/tests.rs#L113-L120). Mesh upload at [cell_loader/water.rs:169](../../byroredux/src/cell_loader/water.rs#L169) passes `rt_enabled = false`, so even if the predicate ever drifted, water meshes have no BLAS to instance.
- **#1025 / F-WAT-04** (grazing-angle clamp incomplete) — full half-space projection now at [water.frag:398-405](../../crates/renderer/shaders/water.frag#L398-L405); the prior 60% mix is gone.
- **#1036 / F-WAT-08** (orphan vert→frag varyings) — `vUV` / `vInstanceIndex` removed in lockstep across `water.vert` (112 lines, no orphan locations) and `water.frag` ([:74-80](../../crates/renderer/shaders/water.frag#L74-L80) — comment documents the removal).
- **#1067 / REN-D14-NEW-07** (no static guard preventing water shaders from acquiring GpuMaterial binding) — descriptor-set 0/1 layout for water keeps the bindless texture array + camera + TLAS + instance buffer only; no MaterialBuffer binding on the water pipeline.
- **#1068 / F-WAT-06** (duplicate trig in WATR resolver) — confirmed consolidated.
- **#1069 / F-WAT-09** (WATR reflection_color tint) — `tint_reflect.rgb` field on `WaterPush` plumbed through to `traceWaterRay`'s blend mix; `tint_reflect.w` carries reflectivity (moved from `tune.w`).
- **#1070 / M38 Phase 2** + **#1110 / TD1-003** (orphaned TODO) — orphan marker removed; the deferred work it tracked is now #1210.
- **#1071 / F-WAT-11** (CULL_MODE static vs dynamic) — `DynamicState::CULL_MODE` declared at [water.rs:188-190](../../crates/renderer/src/vulkan/water.rs#L188-L190); caller emits `cmd_set_cull_mode(NONE)` before the water draw loop.
- **#1087 / REN-D3-001** (stale "112-byte" strings post-WaterPush growth) — `WaterPush` size pinned to 128 B by `const _: () = assert!` at [water.rs:91-95](../../crates/renderer/src/vulkan/water.rs#L91-L95).
- **#1129 / REN-D3-NEW-01** (redundant `cull_mode(NONE)` static + CULL_MODE dynamic) — verified single canonical path: `cull_mode(NONE)` is the baseline for pipeline-create state, `CULL_MODE` is the per-draw dynamic override.
- **#1187 / REN-D14-NEW-01** (stale Rust-struct path post-Session-34 split) — `water.vert` comments updated.
- **#1199 / cell-loader interaction** — `unload_cell` no longer wipes worldspace-scoped weather/sky resources, which has no direct Dim 17 effect (water is per-cell, not per-worldspace), but is strictly positive for water rendering in adjacent-cell streaming.
- **Material distinct from glass** — water draws use `is_water` flag, glass uses `material_kind == MATERIAL_KIND_GLASS`. They cannot collapse via R1 dedup because `MaterialTable::intern` keys on the full `GpuMaterial` byte sequence — the distinct material content (water has different shallow/deep colors, fog_near/fog_far, scroll directions) guarantees distinct hashes.
- **No-resort contract** (#1026 / F-WAT-05) — `water_commands_match_draw_slots` debug_assert at [water.rs:142-150](../../crates/renderer/src/vulkan/water.rs#L142-L150) pins the invariant.

## Prioritized Fix Order

1. **REN-DIM17-01 / #1210** (LOW, carry-over) — implement water-side caustic synthesis in `water.frag`. Defer until either (a) a dedicated render-pass refactor lands or (b) the caustic_splat sibling itself proves stable enough to extend rather than fork. Not blocking any other work.

## Methodology notes

- Walked the full Dim 17 surface: pipeline construction → shader → cell-loader spawn → submersion system → re-emit + sort → TLAS exclusion → unload.
- Cross-checked every closed F-WAT-* / REN-D17-* issue against current code; all fixes intact.
- Cross-referenced changes since 2026-05-19 — water.rs only got test-fixture additions for new `DrawCommand` fields plumbed by adjacent work (#1147 Phase 2b translucency, #890 LUT). No production code change.
- Tests: `cargo test -p byroredux water` (2 pass) + `cargo test -p byroredux-renderer water` (13 pass).
- Did not run the engine to observe water in a live frame — the read-only audit is sufficient given the unchanged code surface and intact lockstep.

## What changed (vs 2026-05-19 audit)

Nothing of Dim 17 substance. The earlier audit's structure is preserved; the only delta is that I re-verified each closed item against current `HEAD` and confirmed the test counts (water-specific tests went from 12 passing to 13 — `acceleration::tests::water_excludes_from_tlas_regardless_of_in_tlas` was added or the count was previously off by one; either way, the predicate test coverage is now denser).
