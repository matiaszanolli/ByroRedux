# Renderer Audit — 2026-08-27

**Scope**: Full `/audit-renderer` run as part of an `--preset comprehensive`
audit-suite sweep. All 23 dimensions in
`.claude/commands/audit-renderer/SKILL.md` were walked; no `--focus` filter.
**Depth**: deep (data-flow tracing + invariant validation).
**Repo state**: `main`, HEAD `969d81c8`.
**Method**: single-auditor source verification (no sub-agents — nested-agent
result relay is unreliable in this project). Dedup against
`gh issue list --limit 400 --state open` and the three most recent renderer
reports (`AUDIT_RENDERER_2026-08-24.md`, `…08-20.md`, `…08-16.md`), plus
today's sibling reports (`AUDIT_SAFETY_2026-08-27.md`,
`AUDIT_PERFORMANCE_2026-08-27.md`) to avoid re-filing their findings.
**Verification performed**: full `cargo test -p byroredux-renderer --lib`
(768 passed, 0 failed) and a full 22-shader `glslangValidator -V`
recompile-and-byte-compare against every checked-in `.spv` (all 22 identical).
No Vulkan device, RenderDoc capture, or `BYRO_VALIDATION` run was available —
every barrier/layout verdict below is source-read confidence only, per the
project's standing no-speculative-Vulkan-fix policy.

## Executive Summary

**6 NEW findings**: 0 CRITICAL, 1 HIGH, 3 MEDIUM, 2 LOW.

Prior renderer coverage is extremely dense (~95 reports), and the last full
sweep was three days ago at `048a8bd8`. Value in this run is therefore
concentrated almost entirely in the 147-commit delta since then, which
contained three substantial renderer feature landings:

* **`#3298` / `#3372`** — the global geometry SSBO rebuild became a resumable,
  multi-frame state machine that keeps **two full geometry generations
  resident** while copying (`crates/renderer/src/mesh.rs`, +875 LOC).
* **BGEM v21+ glass optics + Bethesda soft/rim/back lighting response**
  (2026-08-25) — `GpuMaterial` 364 → 396 → 432 B, `triangle.frag` +192,
  `include/lighting.glsl` +62, `include/pbr.glsl` +19.
* **`#3323`** — `GpuCamera` 352 → 368 B with an appended `exterior_sky_tint`
  lane for the interior window-portal escape.

All three are structurally sound. The findings below are (1) a real
device-headroom guard that the new rebuild path routes around, (2) the fifth
recurrence of the GPU-struct documentation-drift class, (3) two no-value
sentinels in the newly-live Bethesda lighting fields that the shader's clamps
convert into extreme shading values, and (4/5) two stale load-bearing comments.

## RT Pipeline Assessment

Dimensions 1–3 (BLAS/TLAS, SSBO/ray queries, GPU-struct layout) are clean of
new CRITICAL/HIGH structural defects.

* **`GpuMaterial` = 432 B** is byte-correct, offset-pinned
  (`gpu_material_field_offsets_match_shader_contract` asserts every one of the
  new glass-optics and lighting-response offsets, 364→428), and its single GLSL
  mirror in `crates/renderer/shaders/include/bindings.glsl` matches
  **field-for-field** through `backLightingMapIndex`. `hash_gpu_material_fields`
  covers all new fields.
* **`GpuCamera` = 368 B**. All five direct `uniform CameraUBO` re-declarers
  (`include/bindings.glsl`, `triangle.vert`, `water.vert`, `cluster_cull.comp`,
  `caustic_splat.comp`) carry `exteriorSkyTint` last;
  `camera_ubo_size_matches_gpu_camera_in_every_shader` (SPIR-V reflection) is
  green, as is `every_committed_spv_is_spirv_1_0`.
* **`GpuInstance` = 160 B**, `surface_id` still at offset 108, morph fields
  appended at 128/136/144. All five `struct GpuInstance` mirror sites carry
  `surfaceId`.
* **`#3372`'s `scene_geometry_resident` predicate is correct.** The hazard I
  independently re-derived — a mesh registered *after* `plan_geometry_compaction`
  shrinks the pools gets a compacted-layout offset that can land inside the
  still-bound uncompacted buffer's extent, so the length check alone waves it
  through — is exactly what the `deferred_plan_mesh_count` arm rejects, and the
  frame driver's `is_geometry_dirty()` gate provably cannot skip that filter
  while `deferred_compaction` is `Some` (the plan is taken and applied *before*
  the dirty flag clears, in the same `advance_geometry_rebuild` call).
* Deferred BLAS destruction, deferred BLAS-scratch destruction, the
  AS-build-input `SHADER_READ` flag distinction, `render_finished`
  per-swapchain-image indexing, the two-sided-blend split predicate
  (`is_blend && b.two_sided && b.order_dependent_glass`), the thin-glass gate,
  ReSTIR-DI's normal cone + stable surface ID, and the BC1 punch-through gate
  are all intact and test-pinned.

## GPU-Struct & Memory Assessment

Rust ↔ GLSL ↔ SPIR-V lockstep is fully intact at HEAD. **All** GPU-struct
problems found this run are documentation-side (D3-01). The one substantive
memory finding is D5-01: `#3298` introduced a deliberate transient doubling of
the largest non-texture VRAM class and, in doing so, moved the `#2374`
device-headroom guard off the primary path onto a fallback that is only
reachable via a clean allocation error.

## Findings

### CRITICAL

*(none)*

### HIGH

#### REN-2026-08-27-D5-01: `#3298`'s chunked geometry rebuild attempts a second full-size device-local generation at any size, routing around the `GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES` guard `#2374` added for exactly that case

- **Severity**: HIGH
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/mesh.rs:1244-1288` (`rebuild_geometry_ssbo`),
  `crates/renderer/src/mesh.rs:1309-1336` (`try_allocate_empty_geometry_buffers`),
  `crates/renderer/src/mesh.rs:222-227` (`geometry_rebuild_needs_idle`),
  `crates/renderer/src/mesh.rs:1518-1521` (its only live caller)
- **Status**: NEW (behaviour introduced by `ae7179a3` / `#3298`, 2026-08-25 —
  after the 2026-08-24 renderer sweep)
- **Description**: `GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES` exists because,
  per its own doc, *"Large global-geometry rebuilds cannot safely keep two prior
  SSBO generations alive while allocating the replacement on mid-range GPUs.
  Above 256 MiB, prefer a one-time device-idle reclamation over a recoverable
  allocation failure escalating into `VK_ERROR_DEVICE_LOST` (FO4 boundary
  traversal, #2374)."* After `#3298`, `rebuild_geometry_ssbo` no longer consults
  that predicate at all on its primary path: whenever a prior generation exists
  it calls `try_allocate_empty_geometry_buffers` for the **full projected size**,
  unconditionally, and holds both generations for the whole multi-frame copy.
  `geometry_rebuild_needs_idle` is now reachable only from
  `rebuild_geometry_ssbo_atomic_fallback`, which itself is only reached *after*
  that allocation has already returned `Err`. The threshold's premise is that
  on a constrained device the double allocation may **succeed** (driver-managed
  residency / system-memory spill) and escalate to device loss under later
  pressure — a path a post-hoc `Err` check cannot catch.
- **Evidence**:
  ```rust
  // mesh.rs:1250-1270 — no size gate anywhere on this arm
  if has_existing_buffers {
      let rt_usage = …;
      match Self::try_allocate_empty_geometry_buffers(
          device, allocator, vertex_size, index_size, rt_usage,
      ) {
          Ok((new_vertex_buffer, new_index_buffer)) => {
              self.geometry_rebuild = Some(GeometryRebuildInProgress { … });
  ```
  versus the only site that still asks the question, inside the fallback:
  ```rust
  // mesh.rs:1520-1521
  let reclaim_before_rebuild =
      geometry_rebuild_needs_idle(projected_bytes, has_existing_buffers);
  ```
  `GeometryRebuildInProgress`'s own doc (`mesh.rs:57-77`) states the trade-off
  plainly — *"two full geometry SSBO generations are resident in device-local
  memory at once for the rebuild's duration"* — and names the `#2374` path as
  *"a fallback, not the common case"*, i.e. the inversion is intentional; what
  is missing is any size condition on it.
- **Impact**: On the 12 GB dev card this is invisible (the FO4 boundary case is
  ~800–900 MiB duplicated against ~9 GB of headroom, and the audit environment
  has no GPU to measure on). On a 6 GB card — the documented RT minimum in
  `feedback_vram_baseline.md` — an FO4/Skyrim boundary crossing now transiently
  doubles the single largest non-texture allocation class on top of a ~1.7 GB
  steady state, in exactly the scenario `#2374` was filed for. The blast radius
  if it lands is `VK_ERROR_DEVICE_LOST`, which is unrecoverable. Note also that
  a *second* rebuild starting while the previous generation is still in the
  `DEFAULT_COUNTDOWN` deferred-destroy queue can put three generations in flight.
- **Related**: `#2374` (CLOSED — the guard this bypasses), `#3298` (CLOSED — the
  landing), `#3372` (CLOSED — the sibling correctness fix on the same feature),
  `feedback_vram_baseline.md`. Distinct from `SAFE-2026-08-27-01`, which covered
  the compacted-offset publish, not headroom.
- **Suggested Fix**: Gate the chunked path on
  `!geometry_rebuild_needs_idle(projected_bytes, has_existing_buffers)` so
  rebuilds at or above 256 MiB take the atomic idle-reclaim route `#2374`
  specified, and only sub-threshold rebuilds duplicate. If the intent is that
  the chunked path should supersede the threshold outright, that reversal needs
  its own evidence on a memory-constrained device, and
  `GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES`' doc comment should be rewritten
  rather than left asserting a rule the primary path no longer follows.

### MEDIUM

#### REN-2026-08-27-D3-01: the fifth recurrence of GPU-struct doc drift — `shader-pipeline.md` still documents `GpuInstance` at 128 B and `GpuCamera` at 352 B, and `memory-budget.md` understates the Instance SSBO by 25%

- **Severity**: MEDIUM
- **Dimension**: GPU-Struct Layout (doc lockstep)
- **Location**: `docs/engine/shader-pipeline.md:193` (`GpuCamera` heading),
  `docs/engine/shader-pipeline.md:211` (table's last row is `render_debug` at
  offset 336; no `exterior_sky_tint` row at 352),
  `docs/engine/shader-pipeline.md:248` (`GpuInstance` heading),
  `docs/engine/shader-pipeline.md:268` (table's last row is `_reserved` at
  offset 120; no morph rows at 128/136/144),
  `docs/engine/shader-pipeline.md:427`;
  `docs/engine/memory-budget.md:31` (Instance SSBO row),
  `docs/engine/memory-budget.md:37` (Camera UBO row);
  `docs/engine/renderer.md:271`, `:531`, `:582`;
  `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:335` (`"**352 bytes**"`),
  `:342` (growth history stops at 352), `:131`, `:137` (`"current 128 B"`);
  `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:50`
  (`"must stay 352 B … plus ten"` — it is eleven now), `:130`
  (`"struct still exactly 128 B"`);
  `crates/renderer/src/vulkan/context/mod.rs:810`;
  `.claude/commands/audit-renderer/SKILL.md:115`
- **Status**: Regression of `#3201` / `#2483` (both CLOSED) — same defect class,
  new growths. `GpuInstance` 128 → 160 B landed 2026-08-23 (`#3231`) and was
  **not** caught by the 2026-08-24 sweep's own `D3-01`, which examined only
  `GpuMaterial`; `GpuCamera` 352 → 368 B landed with `#3323` after that sweep.
- **Description**: The Rust structs, their GLSL mirrors and all layout-pin tests
  are correct. The documentation the project's own audit protocol designates
  authoritative for these byte layouts is not. `docs/engine/renderer.md:582`
  additionally names two tests that no longer exist
  (*gpu_instance_is_128_bytes_std430_compatible*, *gpu_camera_is_352_bytes*);
  the live pins are `gpu_instance_is_160_bytes_std430_compatible` and
  `gpu_camera_is_368_bytes`.
- **Evidence**: `size_of::<GpuInstance>() == 160` and
  `size_of::<GpuCamera>() == 368` are asserted at
  `gpu_instance_layout_tests.rs:30` and `:66`, both green. Against that,
  `docs/engine/memory-budget.md:31` reads
  `| Instance SSBO | MAX_INSTANCES = 262 144 | 262 144 | 128 B (#2219) | 33.6 MB | **67.1 MB** |`
  — the correct figures at 160 B are 41.9 MB / 83.9 MB, so the row understates
  the second-largest scene-buffer allocation by ~16.8 MB. `:36` reads
  `| Camera UBO | — | 1 | 352 B | 352 B | **704 B** |`. And
  `gpu_types.rs:335` still opens the struct with *"GPU-side camera data
  (**352 bytes**, std140-compatible)"* two lines above a doc line that
  correctly names `gpu_camera_is_368_bytes`.
- **Impact**: No runtime effect — every allocation sizes from
  `size_of::<T>()`. The damage is that an auditor or implementer following
  `_audit-common.md`'s instruction to *"audit against those docs"* is handed
  wrong numbers, and that the VRAM budget doc under-reports the real ceiling.
  This is the fifth iteration of the same manual-fix cycle
  (`#2222`→`#2308`→`#2415`→`#2483`/`#3201`→ here).
- **Related**: `#3201`, `#2483`, `#3197`, `feedback_shader_struct_sync.md`
- **Suggested Fix**: Update the sites, add the three `GpuInstance` morph rows
  and the `GpuCamera.exterior_sky_tint` row, and rename the two dead test
  citations. Given the recurrence count, the durable fix is the one `#3197`
  already argued for: a doc-glob regression check that greps
  `docs/engine/*.md` + `.claude/commands/*.md` for size literals and dead
  `gpu_*_is_*_bytes` test names and diffs them against `size_of::<T>()`.

#### REN-2026-08-27-D17-01: the newly-live Bethesda rim-light lobe turns a "no value authored" `0.0` into the clamp floor 0.25, producing a near-full-surface albedo add — the exact hazard `#2589` fixed for the two sibling fields in the same struct literal

- **Severity**: MEDIUM
- **Dimension**: Disney BSDF / PBR gating (Bethesda lighting response)
- **Location**: `crates/renderer/shaders/include/lighting.glsl:110-116`
  (`bethesdaRimFactor`); the `0.0` no-value sites it falls back onto:
  `crates/nif/src/blocks/shader.rs:827` (`material_reference_stub`),
  `crates/nif/src/blocks/shader.rs:1117` (`parse_fo4`),
  `crates/nif/src/blocks/shader.rs:1314` (`parse_fo76_plus`),
  `crates/nif/src/import/material/mod.rs:1153` (`MaterialInfo::default`),
  `crates/nif/src/import/types.rs:654` (`ImportedMaterial` default),
  `crates/core/src/ecs/components/material.rs:519` (`Material` default)
- **Status**: NEW (consumer landed 2026-08-25, `b80313f6`/`ceb69d24`; the fields
  were inert before that)
- **Description**: `bethesdaRimFactor` resolves its exponent as
  `rimlightPower > 0.0 ? rimlightPower : lightingEffect2`, then
  `clamp(exponent, 0.25, 16.0)`. When **both** lanes are zero — which is the
  state every no-value site above installs — the exponent becomes the clamp
  *floor*, 0.25, i.e. the broadest possible rim rather than a neutral or
  disabled one. Its two siblings in the same file handle their zero case
  deliberately and correctly: `bethesdaDiffuseLightFactor` degenerates to plain
  `max(N·L, 0)` at `width == 0`, and `bethesdaBackFactor` explicitly documents
  *"zero there therefore means the Skyrim unit-strength convention rather than
  disabling a feature whose flag is already set"* and substitutes `1.0`. Rim is
  the one lobe with no such treatment.
- **Evidence**:
  ```glsl
  // lighting.glsl:110-116
  float bethesdaRimFactor(GpuMaterial mat, float NdotV, float frontNdotL) {
      if ((mat.materialFlags & MAT_FLAG_RIM_LIGHTING) == 0u) return 0.0;
      float exponent = mat.rimlightPower > 0.0
          ? mat.rimlightPower : mat.lightingEffect2;
      exponent = clamp(exponent, 0.25, 16.0);
      return pow(clamp(1.0 - NdotV, 0.0, 1.0), exponent) * frontNdotL;
  }
  ```
  and the contribution it feeds (`lighting.glsl:244-248`):
  `brdfResult += albedo * clamp(lightingMask,0,1) * rim * (1.0 - metalness)`.
  At exponent 0.25 the rim weight is `0.56` even head-on (`NdotV = 0.9`) and
  `0.84` at `NdotV = 0.5` — more than half the diffuse lobe again, added
  across the whole surface rather than at its silhouette.
  `nif.xml` (`/mnt/data/src/reference/nifxml/nif.xml:6605-6606`) gives
  `Lighting Effect 1` **default 0.3** and `Lighting Effect 2` **default 2.0**;
  every site listed above installs `0.0` for both. Note that in the *same*
  struct literals, two lines below, `#2589` already applied precisely this
  correction to the neighbouring fields — `grayscale_to_palette_scale: 1.0`,
  `fresnel_power: 5.0` — with a comment stating *"`0.0` here silently survived …
  producing a full-strength (`pow(1-cosθ,0)==1`) Fresnel term at every view
  angle the moment a shading consumer reads it — latent only because no consumer
  exists yet"*. A consumer now exists.
- **Impact**: Visual only, no crash. Reachable on (a) Skyrim content, where
  `parse_skyrim` hard-sets `rimlight_power = 0.0` by design and the real rim
  power lives in `lighting_effect_2`, so any `SLSF2_Rim_Lighting` material with
  an unset/zero `lighting_effect_2` over-brightens; and (b) any FO4+ material
  reaching the shader through `material_reference_stub` or `MaterialInfo::default`
  with the rim flag set. Real-content prevalence is **unmeasured** — this
  environment has no GPU and no census was run — so the finding is the
  degenerate branch, not a claim about how many meshes hit it.
- **Related**: `#2589` (SKY-D7-01, the identical fix on the sibling fields),
  `#2284`, `feedback_no_guessing.md`
- **Suggested Fix**: Apply `#2589`'s own rule to these two fields — seed
  `lighting_effect_1: 0.3` / `lighting_effect_2: 2.0` at the five no-value
  sites, matching `nif.xml`'s declared defaults — and/or give
  `bethesdaRimFactor` an explicit zero arm (`return 0.0`, or substitute the
  format default) the way `bethesdaBackFactor` already has one, so the clamp
  floor is never load-bearing.

#### REN-2026-08-27-D6-01: FO4's `Rimlight Power` `FLT_MAX` sentinel — a parser discriminator, and `nif.xml`'s declared default — is carried verbatim through NIFAL into `GpuMaterial` and clamps to the maximum rim exponent

- **Severity**: MEDIUM
- **Dimension**: NIFAL Material
- **Location**: `crates/nif/src/blocks/shader.rs:1070-1089` (the sentinel read),
  `crates/nif/src/import/material/dedicated_shader.rs:336`
  (`info.rimlight_power = shader.rimlight_power`),
  `byroredux/src/material_translate.rs:520`
  (`rimlight_power: source.rimlight_power`),
  `byroredux/src/render/static_meshes.rs:691`,
  `crates/renderer/shaders/include/lighting.glsl:110-116`
- **Status**: NEW
- **Description**: `parse_fo4` correctly implements `nif.xml`'s conditional:
  `Backlight Power` is present iff `Rimlight Power >= FLT_MAX`. That makes
  `FLT_MAX` a *discriminator*, not an authored exponent — and `nif.xml` declares
  it the field's **default** (`default="#FLT_MAX#"`,
  `/mnt/data/src/reference/nifxml/nif.xml:6608`), so it is the common value on
  FO4 content that authors backlighting. Nothing between the parser and the GPU
  normalises it: it flows unchanged into `ImportedMaterial.rimlight_power`,
  through `translate_material` into canonical `Material.rimlight_power`
  (`Material::sanitize`'s `fix_scalar!` only repairs non-finite values, and
  `FLT_MAX` is finite), into `GpuMaterial.rimlight_power`, and finally into
  `bethesdaRimFactor`, where `rimlightPower > 0.0` is true and
  `clamp(FLT_MAX, 0.25, 16.0)` yields exponent **16.0** — the tightest rim the
  shader can express — for a material that authored no rim power at all.
- **Evidence**: the parser's own comment names the value as a marker —
  *"nif.xml gates Backlight Power on `Rimlight Power >= FLT_MAX` … the
  `#FLT_MAX#` sentinel"* (`shader.rs:1076-1080`) — and then the struct literal
  at `shader.rs:1119` stores that same `rim` verbatim. The only site that
  overwrites it is `byroredux/src/asset_provider/material.rs:85`
  (`material.rimlight_power = bgsm.rim_power;`), which fires only when a
  BGSM/BGEM sidecar resolves.
- **Impact**: Visual only, and gated on `MAT_FLAG_RIM_LIGHTING` also being set,
  so it needs a FO4 lit material that both sets `SLSF2_Rim_Lighting` and leaves
  `Rimlight Power` at the backlight-marker default with no BGSM override —
  authoring that is inconsistent but expressible. The reason to fix it anyway
  is that this is a per-game wire encoding surviving past the NIFAL boundary
  into a canonical field, which is exactly what `docs/engine/nifal.md`'s
  no-fabrication rule forbids; the severity table's NIFAL floor applies.
- **Related**: `#1901` (the `FLT_MAX` bound that made the parse correct),
  `docs/engine/nifal.md`, `feedback_format_translation.md`
- **Suggested Fix**: Normalise at the parser, where the sentinel's meaning is
  known: when the `FLT_MAX` branch is taken, store `rimlight_power` as the
  format's real no-value default (BGSM's own `rim_power: 2.0`,
  `crates/bgsm/src/bgsm.rs:159`) or `0.0`, and keep `backlight_power` as the
  only thing the branch communicates. A regression test on the `rim >= f32::MAX`
  arm asserting that no `f32::MAX` reaches `ImportedMaterial` would pin it.

### LOW

#### REN-2026-08-27-D17-02: `shadowableLightRadiance`'s doc block now sits above three unrelated helpers inserted between it and the function it documents

- **Severity**: LOW
- **Dimension**: Disney BSDF / PBR gating
- **Location**: `crates/renderer/shaders/include/lighting.glsl:80-92`
- **Status**: NEW (introduced 2026-08-25 alongside the Bethesda lighting lobes)
- **Description**: The block beginning *"Direct Cook-Torrance contribution of
  cluster light `i` at this fragment — exactly the `brdfResult *
  unshadowedRadiance` the WRS streaming pass accumulates …"* documents
  `shadowableLightRadiance`, but the three new `bethesdaDiffuseLightFactor` /
  `bethesdaRimFactor` / `bethesdaBackFactor` helpers were inserted between the
  comment (ending line 91) and the function (now at line 127). As written, the
  comment reads as documentation for `bethesdaDiffuseLightFactor`.
- **Evidence**: `lighting.glsl:91` is the comment's last line; `:92` opens
  `vec3 bethesdaDiffuseLightFactor(`; `:127` opens
  `vec3 shadowableLightRadiance(` with no doc block of its own.
- **Impact**: Documentation only. It matters slightly more than usual because
  the displaced paragraph is the one stating the bit-for-bit
  accumulate-then-subtract invariant that gates every future edit to this
  function — an invariant this audit separately confirmed still holds (all five
  `shadowableLightRadiance` call sites in `triangle.frag` pass identical
  `lightingMask` / `backLightingMap` arguments).
- **Related**: `#1369`
- **Suggested Fix**: Move the three helpers above the comment block, or move the
  block down to immediately precede `shadowableLightRadiance`.

#### REN-2026-08-27-D18-01: `weather_system` still carries a pre-`#1199` comment claiming `unload_cell` removes `SkyParamsRes` on every cell unload — the invariant `#3323`'s correctness now depends on

- **Severity**: LOW
- **Dimension**: Sky / weather / exterior lighting
- **Location**: `byroredux/src/systems/weather.rs:725-727`
- **Status**: NEW
- **Description**: The `#803` cloud-scroll comment reads *"cloud scroll lives on
  `CloudSimState`, which survives cell transitions (unlike `SkyParamsRes`, which
  `unload_cell` removes on every cell unload)"*. That parenthetical was true
  before `#1199` and is false now:
  `byroredux/src/cell_loader/unload.rs:166-178` documents at length that
  `SkyParamsRes` / `CellLightingRes` / `WeatherDataRes` are worldspace-scoped
  with World lifetime and are deliberately **not** released per cell.
- **Evidence**: `unload.rs:166-178` — *"`SkyParamsRes` … are worldspace-scoped
  — acquired once by `apply_worldspace_weather` … at streaming bootstrap, not
  per cell load. The pre-#1199 pattern released them on every cell unload …
  Their lifetime now matches the World."* No `remove_resource::<SkyParamsRes>()`
  exists on any unload path.
- **Impact**: Documentation only, but newly load-bearing: `#3323`'s entire
  correctness argument for `GpuCamera.exterior_sky_tint` rests on
  `SkyParamsRes` surviving the transition into an interior so `weather_system`
  can keep advancing the exterior TOD colour the window-portal escape reads
  (`byroredux/src/render/sky.rs:58-71`). A reader who trusts the stale
  parenthetical would conclude `#3323` is a no-op on the exact path it was
  written for, and could "fix" the wrong end.
- **Related**: `#1199`, `#3323`, `#803`
- **Suggested Fix**: Drop the parenthetical or replace it with the current
  contract ("`SkyParamsRes` is likewise World-lifetime since `#1199`; this
  accumulator is separate because …").

## Reconfirmed / verified INTACT this run (not re-filed)

| Area | Verified |
|---|---|
| SPIR-V lockstep | All 22 GLSL sources recompiled with `glslangValidator -V`; all 22 `.spv` byte-identical to the checked-in copies |
| Renderer test suite | `cargo test -p byroredux-renderer --lib` → 768 passed, 0 failed |
| `GpuMaterial` GLSL mirror | field-for-field match through offset 428 (`backLightingMapIndex`) |
| `GpuCamera` mirrors | `exteriorSkyTint` present and last in all 5 direct re-declarers |
| `GpuInstance` mirrors | `surfaceId` present in all 5 `struct GpuInstance` sites |
| `#3372` residency predicate | `scene_geometry_resident`'s `deferred_plan_mesh_count` arm correctly holds post-plan latecomers out of raster/TLAS; driver gate cannot skip it |
| `#3374` morph drain | now outside the `(skin_compute, accel_manager)` guard, with a source-position pin |
| `#2769` TLAS LRU | second stamping pass removed; equivalence argument (stamp precedes the `missing_ssbo_instance` drop) verified against `build_tlas_instances` |
| `#2768` dispatch constants | `ssao.rs` / `caustic.rs` now use `WORKGROUP_X/Y` from `shader_constants` |
| `#2771` ping-pong | `restir.rs` / `svgf.rs` on the general `(f + N - 1) % N` previous-slot form |
| `#2766` draw counters | `dispatch_direct` returns whether it recorded; both early-outs no longer counted |
| Dim 12 two-sided split | `is_blend && b.two_sided && b.order_dependent_glass` — no `z_write` term, `order_dependent_glass` present |
| Dim 22 light-animation pair | `canonical_light_animation_flags` / `canonical_light_shadow_flags` remain a documented, deliberately-asymmetric mirrored pair |
| Dim 5 allocator ordering | `AllocatorResource` still removed from the `World` before `VulkanContext::drop` (`app_events.rs:59`) |
| Dim 4 semaphores | `render_finished` still per-swapchain-image (`sync.rs:56-86`) |
| Fresnel-power neutrality | `fresnel_power` defaults to `5.0` at every no-value site, so `fresnelSchlickPower`'s `abs(exponent - 5.0) < 1e-4` fast path keeps the exact pre-2026-08-25 `x^5` behaviour for all legacy content |
| Glass-optics neutrality | BGEM defaults (blur `0.4 × 1.0`, refraction `0.05`, Fresnel `[1,1,1]`) all normalise to `1.0` multipliers in `triangle.frag`, so older content is unchanged |

## Known-open, deliberately NOT re-reported

`#3247`, `#3246`, `#3244`, `#3305`, `#3282`, `#3073`, `#3061`, `#3045`,
`#2985`, `#2830`, `#2829`, `#2821`, `#2795`, `#2774`, `#2764`, `#2697`,
`#2610`, `#2573`, `#2572` — all re-confirmed still open and still accurately
describing current source. `#3244` (`MorphSlot` weight-buffer host-write race)
is in fact **already fixed** in the working tree by
`MorphSlot::stage_weights` / `flush_pending_weights` plus the
`draw_flushes_pending_morph_weights_after_waiting_both_fences` source pin —
it should be closed on GitHub, no code change needed.

Per `SKILL.md` Dimension 1, the two documented-not-fixed AS gaps from `#1793`
(no recovery path for a permanently-missing rigid BLAS; `--grid` burst
false-eviction via the shared `frame_counter`) were re-verified as unchanged
and are not re-reported. The Cornell metalness-vs-lighting confound and the
glass-stipple / IGN refraction jitter remain open observations, not harness
bugs.

## Prioritized Fix Order

1. **REN-2026-08-27-D5-01** — restore a size gate on the double-generation
   rebuild (correctness/safety on non-dev hardware; the only finding with an
   unrecoverable failure mode).
2. **REN-2026-08-27-D17-01** — seed the `lighting_effect_1/2` no-value sites
   from `nif.xml`'s declared defaults and/or give `bethesdaRimFactor` an
   explicit zero arm.
3. **REN-2026-08-27-D6-01** — normalise the `FLT_MAX` rim sentinel at the
   parser so no per-game wire marker crosses the NIFAL boundary.
4. **REN-2026-08-27-D3-01** — the doc pass, ideally with the automated
   size-literal check rather than a sixth manual sweep.
5. **REN-2026-08-27-D17-02**, **REN-2026-08-27-D18-01** — comment repairs.

## Needs-RenderDoc / device verification

No barrier, layout, or render-pass edit is proposed by this report. Three
items were examined by source read only and would need a live device to settle
definitively; none is reported as a defect:

* **REN-2026-08-27-D5-01's failure mode.** Whether a mid-range driver satisfies
  the second full-size `GpuOnly` allocation (rather than returning `Err`) is a
  driver-residency question. `#2374`'s own text asserts it does; confirming
  requires a memory-constrained card, which this environment does not have.
* **Per-chunk stall shape.** `advance_geometry_rebuild` issues one
  `copy_bytes_range` per call — a 64 MiB staging write plus a submit and fence
  wait on the graphics queue, inside `render_one_frame`. The elapsed-time
  trade-off is documented and intended; the per-frame cost is unmeasured, and
  ROADMAP `R6a-stale-20` gates any FPS/ms claim regardless.
* **FSR dispatch-failure recovery.** `take_new_dispatch_failure` →
  `signal_temporal_discontinuity(1)` reads correctly, but exercising it needs
  `BYRO_FSR_FORCE_DISPATCH_FAIL=1` on a real device.

## Coverage gaps declared

Per `_audit-common.md`'s un-owned-subsystem rule: this run covered
`crates/renderer` and `crates/debug-ui`'s `egui_pass` seam, and touched
`crates/fsr3-sys` only through `frame_upscaler.rs`'s Rust side (the FFI
contract itself is `/audit-safety` Dimension 1's, and today's
`AUDIT_SAFETY_2026-08-27.md` covered it). The FP32 FSR permutation remains
untested — carried scope, not a finding. `crates/sdk`, `crates/mod-runtime`,
`crates/facegen`, `crates/hkx`, the debug server/protocol, and the P2 gameplay
slice were **not** examined by this audit.
