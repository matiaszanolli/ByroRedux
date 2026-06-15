# Regression Verification Audit — 2026-06-14

**Scope:** Dynamically discovered closed `bug` issues (GitHub `--limit 50`,
window 2026-06-02 → 2026-06-14) + the unconditional Step 4 fragile-area
contract checks (NIFAL boundary, typed particle emitters, collision-shape
coverage, Disney BSDF attribution, `#[repr(C)]` GPU struct sizes).

**Method:** For each closed fix, located the fix commit (`git log --grep`),
re-read the live code at the named symbol (not stale line numbers — files were
split into submodules under Session 34/35), confirmed the fix is still present,
located the guard test, and ran the guard where cheap.

## Headline

**Zero regressions.** All 50 closed fixes in the discovery window are PRESENT
and correct in the live tree. All Step 4 fragile-area contracts hold. The only
findings are **hardening gaps** (fix present, no guard test) surfaced as
`PARTIAL`, reported below at LOW severity — none is a regression.

- **CRITICAL: 0  ·  HIGH: 0  ·  MEDIUM: 0  ·  LOW: 7  ·  TOTAL: 7**
- PASS (fix + guard): 43 · PARTIAL (fix, no dedicated guard): 7 · FAIL: 0 · UNVERIFIABLE: 0

The highest-severity correctness fix in the window — **#1520** (HIGH,
cell-unload Rapier body/collider leak) — is fully present with 5 running guard
tests (all GREEN). The render-origin rebase family (#1486/#1487/#1488/#1490/#1498)
and the NIFAL resolve-once contract (#1480) are each pinned by running guard
tests.

## Step 4 — Unconditional fragile-area checks (all PASS)

| Contract | Site | Status |
|----------|------|--------|
| Single material boundary; `Material.metalness/roughness` plain `f32` (no `Option<f32>`) | `byroredux/src/material_translate.rs` (`translate_material`); `crates/core/src/ecs/components/material.rs:217,223` (`resolve_pbr`, `classify_pbr_keyword`) | PASS |
| Typed particle emitters dispatched (not opaque `NiPSysBlock`) | `crates/nif/src/blocks/mod.rs` — `NiPSysEmitterCtlr`/`NiPSysEmitterCtlrData`/`NiPSysGrowFadeModifier` + box/cylinder/sphere/mesh/array emitters all `particle::parse_*` arms | PASS |
| Collision-shape coverage (`BhkMultiSphereShape` + `BhkConvexListShape` → `CollisionShape`) | `crates/nif/src/import/collision.rs:500,618` | PASS |
| Disney/Burley lobe + GLSL-PathTracer MIT attribution + `NUM_RESERVOIRS = 16` | `crates/renderer/shaders/triangle.frag:12-29,3104` | PASS |
| `GpuInstance` = 112 B, `GpuCamera` = 336 B + all shader-contract pins | `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs` | PASS — `cargo test -p byroredux-renderer gpu_` → 22 passed / 0 failed |

The `gpu_` guard run also exercises the closed render-origin fixes directly:
`composite_screen_to_world_dir_subtracts_camera_pos` (#1490),
`triangle_vert_skinned_branch_rebases_render_origin` (#1486),
`skinned_tlas_instance_uses_identity_transform` (#1487),
`caustic_writers_rebase_render_origin_before_reprojection` (#1488),
`water_shaders_must_not_acquire_material_buffer_binding` (#1498) — all GREEN.

## Findings (hardening gaps — LOW)

Each is `PARTIAL`: the fix is confirmed present, but it has no dedicated guard
test, so a future edit to that exact site would not be caught by `cargo test`.
Several were explicitly marked "TESTS: N/A" in their own issue body (shader-math
or Drop-ordering paths that cargo can't observe) — those are noted as
intentional.

### REG-01: #1520 cell-unload Rapier release — *(this one PASSES; listed for the record, not a finding)*
Confirmed PASS with running guards (`cell_loader::rapier_release_tests`, 5 tests
GREEN). Not counted in the LOW total. See summary table.

### REG-02: #1448 screenshot extent capture has no guard test
- **Severity**: LOW
- **Dimension**: Regression — renderer / sync
- **Location**: `crates/renderer/src/vulkan/context/screenshot.rs` (`screenshot_pending_readback`)
- **Status**: PARTIAL (fix present, no guard)
- **Description**: Extent is now captured at record time (`vk::Extent2D`) and the
  readback reads the captured value, surviving a same-frame swapchain resize.
  No automated test (readback path is `byro-dbg`-driven; the issue marked TESTS a gap).
- **Impact**: A revert to live-extent readback would silently re-arm SYNC-01.
- **Suggested Fix**: Add a unit test asserting the readback struct snapshots the
  recording-time extent, or accept as RenderDoc/manual-gated and leave a tracking comment.

### REG-03: #1459 water-caustic sun-direction sign has no guard test
- **Severity**: LOW
- **Dimension**: Regression — renderer / shader math
- **Location**: `crates/renderer/shaders/water.frag` (directional case); `crates/renderer/shaders/caustic_splat.comp`
- **Status**: PARTIAL (fix present, no guard)
- **Description**: Sun-direction sign corrected in both the water shadow/refract
  path and the caustic splat directional branch. Issue states "shader math not
  unit-testable; RenderDoc capture is the regression gate."
- **Impact**: Sign flip would suppress caustics for overhead sun again; invisible to cargo.
- **Suggested Fix**: Intentional manual gate — acceptable; no action required beyond the existing comment.

### REG-04: #1478 host_query_reset capability decouple has no DeviceCapabilities unit test
- **Severity**: LOW
- **Dimension**: Regression — renderer / vulkan feature gating
- **Location**: `crates/renderer/src/vulkan/device.rs` (`host_query_reset_supported`); `crates/renderer/src/vulkan/gpu_timers.rs`
- **Status**: PARTIAL (fix present, no guard)
- **Description**: `hostQueryReset` now probed independently of ray-query
  support; GPU-timer reset gates on `timestamp_supported && host_query_reset_supported`.
  No `DeviceCapabilities` unit test added (issue listed it as a checkbox).
- **Impact**: Regression would re-arm a host `vkResetQueryPool` without the feature on RT-absent devices (validation error).
- **Suggested Fix**: Add a small unit test on the capability struct's gating logic.

### REG-05: #1491 egui render-pass balance on cmd_draw error has no dedicated test
- **Severity**: LOW
- **Dimension**: Regression — renderer / vulkan (render-pass balance)
- **Location**: `crates/renderer/src/vulkan/egui_pass.rs` (~`:204-209`)
- **Status**: PARTIAL (fix present, no guard)
- **Description**: `cmd_draw` result is captured, `cmd_end_render_pass` runs
  unconditionally, then the error propagates — leaving no open render pass in a
  submitted command buffer. Covered by the renderer suite compiling/running, but
  no test exercises the error branch.
- **Impact**: A revert would re-arm an unbalanced render pass on the egui error path (HIGH if it regressed — Vulkan spec violation). Gap is LOW.
- **Suggested Fix**: Hard to unit-test (needs a forced draw failure); leave a tracking comment naming the balance invariant.

### REG-06: #1483 GPU-timer pool destroy outside allocator Drop guard has no test
- **Severity**: LOW
- **Dimension**: Regression — renderer / resource cleanup (Drop ordering)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs` (Drop; gpu_timers destroy hoisted out of the allocator-`Some` guard)
- **Status**: PARTIAL (fix present, no guard)
- **Description**: Query-pool destroys moved out of the `allocator.is_some()`
  guard so they run on the allocator-`None` Drop path. Structurally correct;
  issue marked TESTS N/A (Drop ordering).
- **Impact**: A revert leaks query pools on the allocator-None teardown.
- **Suggested Fix**: Drop-ordering is awkward to test; acceptable as structural, leave the explanatory comment.

### REG-07: #1481 SVGF firefly-clamp hoist has no test
- **Severity**: LOW
- **Dimension**: Regression — renderer / shader (denoiser)
- **Location**: `crates/renderer/shaders/svgf_temporal.comp` (firefly clamp before `hasHistory`)
- **Status**: PARTIAL (fix present, no guard)
- **Description**: Spatial firefly clamp hoisted ahead of the `hasHistory`
  branch so the disocclusion frame is clamped. Issue marked TESTS N/A (visual).
- **Impact**: Revert re-arms a 1-frame un-clamped firefly on disocclusion.
- **Suggested Fix**: Manual/RenderDoc gate — acceptable.

### REG-08: #1477 App-Drop field order has no test
- **Severity**: LOW
- **Dimension**: Regression — renderer / resource cleanup (Drop ordering)
- **Location**: `byroredux/src/main.rs` (`impl Drop for App`)
- **Status**: PARTIAL (fix present, no guard)
- **Description**: `AllocatorResource` is removed before `renderer.take()` /
  `VulkanContext` drop on every teardown, including panic-unwind — closing the
  #1406 allocator-teardown hazard re-arm. Structural; issue listed TESTS as a wishlist.
- **Impact**: Revert re-arms the allocator-outlives-VulkanContext hazard on panic.
- **Suggested Fix**: Drop-order tests are brittle; acceptable as structural with the existing comment.

## Summary table

| Issue | Title (abbrev) | Status | Fix Present | Guard |
|-------|----------------|--------|-------------|-------|
| #1525 | DoF look_at degenerate focus_dist→0 | PASS | Yes | `zero_focus_dist_falls_back_to_pinhole_and_stays_finite` |
| #1520 | Cell-unload Rapier body/collider leak (HIGH) | PASS | Yes | `cell_loader::rapier_release_tests` (5) |
| #1516 | BSTriShape/SSE-recon bitangent sign | PASS | Yes | `types::tests::bitangent_sign_*` + `tangent_convention_tests` |
| #1513 | Particle overlays via one shared helper | PASS | Yes | `apply_emitter_overlays_*` (2) |
| #1512 | Per-game completeness floors recalibrated | PASS | Yes | `cross_game_translation_completeness` |
| #1510 | Starfield BSLightingShaderProperty over-read (1036 NiUnknown) | PASS | Yes | `starfield.tsv` baseline + `unknown_ceiling_starfield` |
| #1509 | NiGeomMorpherController/NiMorphData v10.2 drift | PASS | Yes | `nigeommorpher_v10_2_bsver9_skips_trailing_unknown_ints` |
| #1508 | NiBlendInterpolator v10.1.0.x bands | PASS | Yes | `parse_blend_transform_interpolator_legacy_10_1_0_106` |
| #1507 | NiPSysData/emitter v10.2 trailing fields | PASS | Yes | `parse_particles_data` + emitter life-span tests |
| #1506 | NiInterpController Manager-Controlled bool | PASS | Yes | `parse_single_interp_controller_reads_manager_controlled_on_old_gamebryo` |
| #1504 | BGSM_AUTHORED doc | PASS | Yes (doc) | n/a (doc-only) |
| #1503 | water.frag time push-constant doc | PASS | Yes (doc) | n/a (doc-only) |
| #1502 | water-noise precision bound comment | PASS | Yes (doc) | n/a (doc-only) |
| #1501 | DBG_* bits value-pin via shared `DBG_BITS` | PASS | Yes | `generated_header_contains_all_defines` + `triangle_frag_dbg_bits_not_redeclared` |
| #1498 | water.vert in GpuInstance lockstep guard | PASS | Yes | `every_shader_struct_gpu_instance_names_material_kind_slot` |
| #1497 | TAA alpha floored for moving pixels parked cam | PASS | Yes | `taa_comp_floors_alpha_for_moving_pixels_under_parked_camera` |
| #1494 | RENDER_ORIGIN_SNAP shared constant | PASS | Yes | `render_origin_snap_is_exterior_cell_edge` |
| #1493 | volumetrics UBO block-size pin | PASS | Yes | `volumetrics_ubo_sizes_match_host_structs_in_every_shader` |
| #1492 | (= #1525/#1526 commit) GpuCamera doc | PASS | Yes | shared with #1525 |
| #1491 | egui render-pass balance on cmd_draw error | PARTIAL | Yes | none (REG-05) |
| #1490 | composite screen_to_world_dir camera offset | PASS | Yes | `composite_screen_to_world_dir_subtracts_camera_pos` |
| #1489 | origin-jump prev_view_proj rebased | PASS | Yes | `prev_view_proj_origin_tests` (2) |
| #1488 | caustic deposit reprojection rebased | PASS | Yes | `caustic_writers_rebase_render_origin_before_reprojection` |
| #1487 | skinned TLAS identity transform | PASS | Yes | `skinned_tlas_instance_uses_identity_transform` |
| #1486 | skinned raster rebased by render origin | PASS | Yes | `triangle_vert_skinned_branch_rebases_render_origin` |
| #1483 | GPU timer pools destroyed outside alloc guard | PARTIAL | Yes | none (REG-06) |
| #1481 | SVGF firefly clamp hoist | PARTIAL | Yes | none (REG-07) |
| #1480 | per-draw roughness re-classify removed (NIFAL) | PASS | Yes | `alpha_normal_seeds_smooth_roughness_from_glossiness` + 3 |
| #1479 | TAA luma-clamp skip gated on per-pixel motion | PASS | Yes | `taa_comp_floors_alpha_for_moving_pixels_under_parked_camera` |
| #1478 | hostQueryReset decoupled from ray-query | PARTIAL | Yes | none (REG-04) |
| #1477 | App Drop field order (alloc before context) | PARTIAL | Yes | none (REG-08) |
| #1463 | volumetric UBO single-buffered (doc) | PASS | Yes (doc) | n/a (doc-only) |
| #1462 | volumetric depth-convention mismatch (doc) | PASS | Yes (doc) | n/a (doc-only) |
| #1459 | water caustic sun-direction sign | PARTIAL | Yes | none (REG-03) |
| #1457 | parse_rate_fo4_all_meshes floor calibrated | PASS | Yes | `parse_rate_fo4_all_meshes` |
| #1456 | BGEM merge comment corrected | PASS | Yes (comment) | n/a (comment-only) |
| #1455 | BGSM grayscale_to_palette_scale forwarded | PASS | Yes | `bgsm_merge_forwards_scalars_child_first` |
| #1454 | BGSM fresnel_power forwarded | PASS | Yes | `bgsm_merge_forwards_scalars_child_first` |
| #1453 | BGEM grayscale_texture forwarded | PASS | Yes | `bgem_merge_forwards_grayscale_texture_as_lut_path` |
| #1452 | nif_stats parse-rate gate comment | PASS | Yes (comment) | n/a (comment-only) |
| #1450 | submersion hysteresis band | PASS | Yes | `water::tests::hysteresis_band_*` (4) |
| #1449 | evict_unused_blas multi-batch invariant | PASS | Yes | `const_assert!(MIN_IDLE_FRAMES > MAX_FRAMES_IN_FLIGHT)` |
| #1448 | screenshot extent at record time | PARTIAL | Yes | none (REG-02) |
| #1447 | volumetrics .spv recompile / CameraUBO size | PASS | Yes | `camera_ubo_size_matches_gpu_camera_in_every_shader` |
| #1444 | NiPSysPartSpawnModifier byte-exact dispatch | PASS | Yes | `parse_part_spawn_modifier_consumes_base_plus_three_fields` |
| #1443 | finite/FLT_MAX guard on keyframe converters | PASS | Yes | `sanitize_keyframe_streams::*` |
| #1442 | NiKeyframeController KF-sequence alias | PASS | Yes | `import_sequence_dispatches_keyframe_controller_alias` |
| #1441 | KeyType::Constant honored as stepped hold | PASS | Yes | `const_keytype_holds_start_value_across_segment` |
| #1440 | inline transform controllers in embedded path | PASS | Yes | `import_embedded_animations_captures_inline_transform_controller` |
| #1439 | read_pod_vec all-bit-patterns-valid bound | PASS | Yes | `pod_marker_covers_every_instantiated_type` |

## Notes (not regressions)

- **#1510 root cause** was deeper than the issue title: the `shader_type`
  (`BSShaderType155`) field was read unconditionally for `bsver ≥ 155`, shifting
  every later field 4 B on Starfield (`bsver ≥ 172`) and cascade-truncating all
  full-body shader blocks. Now gated `bsver < STARFIELD` with a Starfield
  empty-name stub discriminator. Runtime ceiling test is `#[ignore]` (needs game
  data); the checked-in `starfield.tsv` baseline (`unknown_blocks 0`) is the guard.
- **#1444** is implemented stronger than the issue proposed (byte-exact base + 3
  trailing fields, not the sketched base-only arm which would under-read 12 B).
- **#1453/#1454/#1455** use unconditional first-wins forwarding (functionally
  equivalent to the issue's epsilon-gated sketch); guard tests pin the precedence.
- **#1450** was implemented despite the issue's "don't fix speculatively" note;
  the hysteresis band is correct and guarded.
- **#1516** hardening gap: the inline/SSE packed-vertex *call sites* aren't
  exercised by a packed-vertex fixture, but the shared `bitangent_sign` helper
  they call is unit-pinned, so convention drift is guarded. Not raised as a
  separate finding (the shared-helper refactor makes operand-order drift far
  less likely than a missing test implies).

## Next step

This audit found no regressions and no actionable defects beyond LOW hardening
gaps. If publishing the LOW findings:

```
/audit-publish docs/audits/AUDIT_REGRESSION_2026-06-14.md
```
