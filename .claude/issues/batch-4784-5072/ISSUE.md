########## issue-4784.json ##########
# 4784: PERF-D5-2026-09-23-02: When any transport emitter is active, every froxel in the grid runs the RK2 backtrace, the six-neighbour gather and the curl forcing, including the empty majority
[OPEN] labels: bug, renderer, medium, performance, shaders

- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp`:
  - `main` → `transportCombustion` (`:2594`);
  - the RK2 block `:2005-2078`;
  - the `combustionActivity < 0.08` neighbour gather `:2007-2018` → `incomingDynamicsFromNeighbors` `:1770-1828`;
  - the midpoint/source probes `:2026`, `:2039`;
  - the `dt > 0` curl/wind forcing `:2309-2337`, computed and then scaled by `activity` (0 on empty froxels).
- **Status**: NEW. This is the residual of #3131: that fix zeroes `dt` only when *nothing* in the scene transports.
- **Description**:
  - `simulationDt` is one scalar for the whole dispatch. With `dt > 0`, an empty froxel (the overwhelming majority) does all of the following:
    - samples its own history (3 fetches);
    - fails the `< 0.08` activity test the "wrong" way, so it runs the six-neighbour gather (6 × `samplePreviousTransport` = 18 fetches; `carriesCombustion` rejects each neighbour only *after* its three samples);
    - takes a midpoint and a source sample (6 fetches);
    - evaluates two `curlField`s (12 `sin`/`cos`) and 2–3 `atmosphericWindVelocity` `sin` calls, all multiplied by `activity = 0`.
  - Total: 27 trilinear RGBA16F 3D fetches and 9 reprojections (mat4·vec4 + `length` + log slice mapping) per quiet froxel, against 3 fetches at `dt = 0`.
  - A plume occupies a tiny fraction of the grid, and its transport support is bounded: `MAX_COMBUSTION_SPEED_MPS` 28 m/s × the `dt` clamp of 1/15 s is 1.87 m per step, far below one 16 m fog cluster.
- **Evidence**:
  - *est.*: +24 fetches × 921,600 froxels = +22 M trilinear fetches/frame at a 720p render extent (FSR Quality at 1080p output), and +50 M at 1080p render.
  - At the 4070 Ti's texture rate that is ≥ 0.16 ms ideal at 1080p. Latency-bound reality is plausibly 0.3–0.6 ms.
  - Bench corroboration (1280×720 output, FSR-Q, 410,880 froxels): Whiterun BanneredMare (hearth → Flame volumes; `fog.rs:595` names its `FlamesSmall03-Emitter`) is 0.83 ms (≈ 2.0 ns/froxel). Prospector is 0.155 ms (≈ 0.38 ns/froxel). The scenes differ in more than transport, so this is corroboration, not attribution. PERF-D8-01's split bracket would make it attributable.
- **Impact**:
  - A tier-invariant cost on every frame of every cell with a fire in view, which covers most Skyrim interiors and many exteriors (Markarth's braziers).
  - It scales linearly with render resolution: roughly 4× at native 4K versus 1080p.
  - The adaptive controller cannot shed it, because only `volumetric_light_cap` is tiered.
- **Related**: #3131, PERF-D5-01, REN-D8-2026-09-23-03 (the frozen residual keeps `transportedMediumActive` froxels paying `transportedCombustionTransmittance`: 8 steps × (1 + lights) fetches).
- **Suggested Fix**:
  1. Add a world-space coarse occupancy mask on the existing 16³ fog-cluster grid. Mark a cell when the inject pass writes `carriesCombustion` (atomicOr into a small SSBO read next frame, one frame behind like the moment buffer), and OR in the clusters that hold a transported source volume (the CPU already knows them).
  2. Dilate by one cell (16 m ≫ 1.87 m/step).
  3. In the shader, skip the whole `hadHistory && dt > 0` block for froxels whose cell and its 26 neighbours are empty, falling through to the `dt = 0` carry.
  4. Hoist the curl/wind forcing under `activity > 0` in any case.
  5. Measure with the PERF-D8-01 split bracket before and after, at a pinned `--rt-test-ray-quality-tier`.
- **Confidence**: High on the structure (read from GLSL). Medium on the magnitude (arithmetic plus cross-scene bench, not an A/B).

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-23.md` (`/audit-suite --preset volumetrics-deep`, HEAD `2237da9c3`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related passes (other compute passes / history buffers / call sites)
- [ ] **TESTS**: A regression test pins this specific fix

########## issue-5028.json ##########
# 5028: REN-D5-2026-09-29-02: the player's mid-life gear imports are never released — each distinct item ever equipped stays resident until shutdown
[OPEN] labels: bug, renderer, low, memory, gameplay, inventory

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-29.md`
**Severity**: LOW
**Dimension**: Memory/Lifecycle (owner overlaps `/audit-gameplay`)
**Location**: `GearImportLoader::step` (`byroredux/src/npc_spawn/loot_appearance.rs`). Its doc says: "the player has no `CellRoot` — their gear outlives cells exactly like the body it hangs from".

## Description
- Equipping an item the actor did not spawn wearing imports its worn NIF under the player body root (`0182fc5e8`).
- Unequipping hides that root (`NpcAppearanceHidden`), and re-equipping reveals it.
- The only path that despawns an `NpcEquipmentPart` root is cell teardown (`stamp_cell_root_range` → `CellRootIndex` → `unload_cell`), and the player never gets one. A failed player import likewise leaves its hidden partial range in place.

## Impact
For every distinct armour/clothing piece equipped in a session, its geometry (global and per-mesh buffers, plus its BLAS once drawn) and texture refcounts stay resident until shutdown. Dropping, selling or destroying the item frees nothing. Growth is bounded by the item catalogue and happens per user action, not per frame; not measured.

## Related
`0182fc5e8` (mid-life gear import); `/audit-gameplay` owns the equip path.

## Suggested Fix
When the imported item's form leaves the inventory, release its root through the same entity-despawn and GPU-handle release path that cell teardown uses. Alternatively, cap hidden gear roots with an LRU.

Validated at HEAD 9fcfdc3fc: `GearImportLoader` doc states player gear outlives cells; unequip only toggles `NpcAppearanceHidden`; no non-cell release path for player gear roots found.

## Completeness Checks
- [ ] **SIBLING**: NPC mid-life gear imports (cell-owned) still released via the cell range
- [ ] **DROP**: GPU handles (mesh, BLAS, textures) released through the existing deferred-destroy path, not immediately
- [ ] **TESTS**: A regression test pins release of an imported gear root when its item leaves the inventory


########## issue-5030.json ##########
# 5030: REN-D11-2026-09-29-01: a startup FSR→TAA fallback is persisted to settings.toml, so one failed FSR context permanently turns later no-flag launches into TAA
[OPEN] labels: bug, renderer, low

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-29.md`
**Severity**: LOW
**Dimension**: FSR/Presentation
**Location**:
- `byroredux/src/app_events.rs` `resumed`: `crate::record_active_upscaler(&self.world, ctx.renderer_config.upscaler, false)`.
- `record_active_upscaler` (`byroredux/src/main.rs`): `if changed || released { settings_io::save(...) }`.
- The FSR→TAA promotion in `VulkanContext::new` (`crates/renderer/src/vulkan/context/init.rs`, the #2480 arm).

## Description
Residual of CLOSED #4974 / #4975. When FSR context creation fails at boot, the renderer promotes to TAA, and `resumed` records that mode with `chosen = false`. `record_active_upscaler` saves whenever the registry value *changed*, regardless of `chosen`. With no CLI pin (the default launch), the fallback is written as `render.upscaler = "taa"`. `active_upscaler_setting_tests` codifies this: `record_active_upscaler(&world, UpscalerMode::Taa, false)` leaves the file holding `taa`. Even without that save, the registry now holds `taa`, so the next unrelated settings save would persist it (the #4974 mechanism).

## Impact
One transient FSR init failure (driver hiccup, device swap, the unexercised FP32 permutation) replaces the engine's default render path for every later launch without `--upscaler`. #4947's `source: settings.toml` log line makes it visible (hence LOW), but it re-opens the harness hazard of a persisted value changing which path "default" exercises.

## Related
#4974, #4975 (closed), #4947, #2480.

## Suggested Fix
Treat a non-`chosen` startup fallback like a CLI override: `pin_stored(UPSCALER_SETTING_ID)` before updating the in-memory registry, so neither this call nor a later unrelated save writes the fallback, and release the pin only on a `chosen` switch. Add the inverse fixture: a fallback with nothing pinned leaves the file untouched.

Validated at HEAD 9fcfdc3fc: `record_active_upscaler` computes `released = chosen && unpin_stored(...)` but saves on `changed || released`, so a `chosen = false` change still saves; `resumed` calls it with `false`.

## Completeness Checks
- [ ] **LOCK_ORDER**: the scoped persistence write lock is still released before the registry read
- [ ] **SIBLING**: other non-chosen setting writes (e.g. FSR quality fallback) checked for the same persistence path
- [ ] **TESTS**: Inverse fixture — an unpinned startup fallback leaves `settings.toml` untouched


########## issue-5053.json ##########
# 5053: PERF-D3-2026-09-29-01: TextureRegistry::has_any_view_of_path builds up to 16 keyed-path Strings per probe, called per fresh texture by the prefetch planner on the main thread
[OPEN] labels: bug, renderer, low, performance

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-29.md` — finding `PERF-D3-2026-09-29-01`

**Severity**: LOW

**Dimension**: GPU Memory Pressure (texture residency) / Streaming

**Location**: `crates/renderer/src/texture_registry/lookup.rs:67-91`; key builders `texture_registry/mod.rs:1220-1241`; caller `byroredux/src/streaming_helpers.rs:720-725` (`prefetch_import_textures` → `TextureProvider::texture_resident`)

**Status in report**: NEW (`a3632909a`)

## Description

the probe tries 4 clamp modes × {2D, cube} × {sRGB, linear}. Each combination re-runs `normalize_path` (an allocation), formats the clamp digit (another allocation), pushes suffixes, and does a SipHash `path_map` lookup. `any()` stops only on a hit, and the planner exists to find textures that are **not** resident. So the common case runs all 16 combinations, about 48 small allocations per fresh texture, on the main thread inside the budgeted apply slices the prefetch was built to relieve.

## Impact

main-thread allocator traffic proportional to unique textures × fresh models per crossing. Small per call, unmeasured. No quantitative guard exists for this site.

## Suggested Fix

normalize once and rewrite only the suffix in a reused buffer, or keep a secondary `FxHashSet` of normalized resident base paths, maintained beside `path_map`.

Validated at HEAD 9fcfdc3fc: `has_any_view_of_path` (`crates/renderer/src/texture_registry/lookup.rs`) still iterates 4 clamp modes × {D2, Cube} × {Srgb, Linear}, each building a `String` via `texture_keyed_path_with_color_space` against a std `HashMap<String, _>` `path_map`; caller `prefetch_import_textures` → `TextureProvider::texture_resident` (`byroredux/src/streaming_helpers.rs`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other `texture_keyed_path*` probe loops in `texture_registry/`)
- [ ] **TESTS**: A regression test pins this specific fix


########## issue-5055.json ##########
# 5055: PERF-D4-2026-09-29-01: GpuLight.history_id is CPU-only identity shipped in the GPU struct (+16 B per light) and mirrored in four shaders that never read it
[OPEN] labels: bug, renderer, low, performance, shaders

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-29.md` — finding `PERF-D4-2026-09-29-01`

**Severity**: LOW

**Dimension**: SSBO Sizing & Upload

**Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:362`; mirrors in `shaders/include/bindings.glsl:293`, `cluster_cull.comp:43`, `caustic_splat.comp:56`, `volumetrics_inject.comp:125`; producers `byroredux/src/render/lights.rs:183,244`

**Status in report**: NEW (`186234944`)

## Description

`LightHistory::remap` runs on the CPU and uploads its result as the header's `previous_to_current` table. No shader reads `history_id`: a grep of `crates/renderer/shaders` finds only the four struct declarations. The field still grows the light stride from 64 to 80 B, so each light upload carries up to 16 KiB of unused bytes at the 1023-light cap. The upload repeats every frame whenever any light animates, because the dirty gate hashes the whole light bytes. It also adds a fifth copy to the Shader-Struct-Sync lockstep set.

## Impact

small upload waste and maintenance surface. Shader fetch cost is essentially unchanged: member-wise SSBO loads skip the unused vec4.

## Suggested Fix

carry the identities in a CPU-side array parallel to `gpu_lights`, moved through the priority sort in the `light_sort_scratch` tuple. Then drop the field from `GpuLight` and the four GLSL mirrors.

Validated at HEAD 9fcfdc3fc: `GpuLight::history_id: [u32; 4]` is at `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs`; the only shader hits for `history_id` are the four struct declarations (`include/bindings.glsl`, `cluster_cull.comp`, `caustic_splat.comp`, `volumetrics_inject.comp`); its only reader is CPU-side `scene_buffer/light_history.rs`.

## Completeness Checks
- [ ] **SIBLING**: every GLSL mirror of `GpuLight` (bindings.glsl + 3 standalone copies) is edited in lockstep, and the layout/size pins are updated
- [ ] **TESTS**: A regression test pins this specific fix (light-history remap still survives priority reordering with identities held CPU-side)


########## issue-5057.json ##########
# 5057: PERF-D5-2026-09-29-01: Early-fragment-test eligibility admits only material_kind == 0, although lighting-shader kinds 1–16 have no discard or depth-write path
[OPEN] labels: bug, renderer, low, pipeline, performance, shaders

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-29.md` — finding `PERF-D5-2026-09-29-01`

**Severity**: LOW (an optimization gap in a landed feature; benefit unmeasured)

**Dimension**: GPU Pipeline

**Location**: `crates/renderer/src/vulkan/context/types.rs:374-384` (`DrawCommand::allows_early_fragment_tests`); kind source `crates/nif/src/import/material/dedicated_shader.rs:488` (`material_kind = shader_type`); discard sites `crates/renderer/shaders/triangle.frag:463,477,1208,1249,1290`

**Status in report**: NEW

## Description

the certificate's doc asks for other kinds to be reviewed against every discard in `triangle.frag`. That review is mechanical:
- `triangle.frag` never writes `gl_FragDepth`.
- Its discards are the alpha test (already excluded by `alpha_threshold == 0.0`), the blend-only transparent-texel cull (already excluded by `!alpha_blend`), and the `MATERIAL_KIND_EFFECT_SHADER` (101) and `MATERIAL_KIND_FIRE_REFRACTION` (103) blocks.
- `BSLightingShaderProperty` kinds 1–16 (env map, glow, parallax variants, face/skin/hair tint, eye env) only change shading.

Every such draw is still routed to the late-test pipeline.

## Impact

on Skyrim and FO4 interiors, where env-mapped, glow and skinned-actor surfaces are a large share of opaque pixels, the early-Z saving is not realized for those draws. The opaque interval is about 95% of MedTek's main pass (26b report).

## Suggested Fix

replace `== 0` with an explicit allow-list of reviewed kinds, pinned by a source-scan test that fails when a new `discard` or `gl_FragDepth` appears under a `materialKind` branch. A/B it with `BYRO_PROFILE=1` `opaque_fragment_invocations` and `main_opaque_ms` at a pinned tier, and repeat the Kendall visual gate.

Validated at HEAD 9fcfdc3fc: `DrawCommand::allows_early_fragment_tests` (`crates/renderer/src/vulkan/context/types.rs`) still requires `self.material_kind == 0`; `triangle.frag` has no `gl_FragDepth` write and its discards are alpha-test, blend-only cull, effect-shader and fire-refraction branches.

## Completeness Checks
- [ ] **SIBLING**: every `discard` / depth-write site in `triangle.frag` (the early variant `triangle_early.frag.spv` is built from it) reviewed against the allow-list
- [ ] **TESTS**: a source-scan pin (via `source_scan::production_text`) fails when a new `discard` / `gl_FragDepth` appears under an allow-listed `materialKind` branch


########## issue-5072.json ##########
# 5072: CONC-D6-2026-09-29-01: A swapchain-format-change rebuild of `EguiPass` drops the egui font atlas, and egui never re-sends it
[OPEN] labels: bug, renderer, low, vulkan

**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-09-29.md`
**Severity**: LOW
**Dimension**: Resource Lifecycle (swapchain recreate)

## Location
- `crates/renderer/src/vulkan/context/resize.rs` — the #2475 format-change arm (`pass.destroy` + `EguiPass::new`)
- `crates/renderer/src/vulkan/egui_pass.rs` — `promote_partial_deltas`, `image_mirrors`

## Description
The rebuilt pass has a fresh `egui_ash_renderer::Renderer` (empty `managed_textures`/`textures`) and an empty `image_mirrors`. The app's `egui::Context` is not reset, so egui believes the font atlas is resident and only sends partial deltas for new glyphs: (1) a partial delta arrives and, with no mirror entry, `promote_partial_deltas` passes it through unchanged; (2) the crate's `set_textures` returns `BadTexture` for the unknown id (`egui-ash-renderer-0.11.0/src/renderer/mod.rs:351`); (3) every `cmd_draw` sampling the atlas also returns `BadTexture` (`:589`). The overlay is dead for the session with an error every frame.

## Evidence
The resize arm constructs `EguiPass::new(...)` with no hand-over of textures or mirrors; `EguiPass::new` initialises `image_mirrors: FxHashMap::default()`. Dates from the #2475 full rebuild (`fd8f67e2a`), not caused by #4986.

## Impact
Only on a surface-format change (HDR toggle or display move). Debug overlay and native pause/inventory/dialogue pages (all egui) render nothing after the flip. Not visible to `cargo test`.

## Related
#2475, #2685, #4986 (whose mirror holds exactly the data a re-seed needs), REN-D5-2026-09-29-01.

## Suggested Fix
Take `image_mirrors` out of the old pass before `destroy`, then replay each mirror as a full delta into the rebuilt pass; alternatively have egui re-send its textures after a rebuild. Add a unit test that the rebuild re-seeds managed textures.

Validated at HEAD 9fcfdc3fc: format-change arm in `resize.rs` still calls `pass.destroy` then `EguiPass::new` with no mirror hand-over; `EguiPass::new` sets `image_mirrors: FxHashMap::default()`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix


