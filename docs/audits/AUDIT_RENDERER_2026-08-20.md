# Renderer Audit — 2026-08-20

**Scope**: `/audit-renderer` (all 23 dimensions), run as part of the
`comprehensive` 25-audit suite sweep. Delta-weighted toward session 70's
WATAL water / volumetrics work per the suite briefing: Dimensions 15
(water), 16 (volumetrics + bloom), 14 (caustics), 2 (SSBO + ray queries),
3 (GPU-struct layout) and 4 (sync/barriers) carried the emphasis.

**Repo state**: HEAD `bb0b92f2`, branch `main`. Delta since the previous
sweep (`85b77371`, 2026-08-16): 335 commits, 88 renderer-owned files,
+11 471 / −1 753 lines.

**Dedup baseline**: `/tmp/audit/issues.json` (400 issues, all states),
`docs/audits/AUDIT_RENDERER_2026-08-16.md` and its older siblings.

**Verification method**: static analysis only — per the suite briefing no
`cargo` command was run. Two mechanical checks were performed instead and
both are reproducible:

1. **Every committed `.spv` was recompiled and byte-compared.** All 21 GLSL
   sources were rebuilt with `glslangValidator -V -I. <shader>` (glslang
   11:16.2.0) into a scratch directory and `cmp`-ed against the committed
   binary. **20 of 21 are byte-identical; `triangle.frag.spv` is not** —
   see REN-D3-01.
2. **The GPU-struct mirrors were diffed field-by-field by script**, not by
   eye: `GpuInstance` (5 declaration sites), `GpuMaterial` (2 sites, 87
   fields), `GpuCamera`/`CameraUBO` (5 sites, 13 fields), `GpuWaterParams`/
   `WaterParams` (3 sites, 22 slots), and every `#define` in the generated
   `include/shader_constants.glsl` against its `shader_constants_data.rs`
   constant. **All are in lockstep at HEAD.**

No Vulkan device and no `BYRO_VALIDATION` run backed this audit. Per the
project's standing no-speculative-Vulkan-fixes rule, every barrier / layout
verdict below is "no defect visible in source", not "confirmed correct".

---

## 1. Executive Summary

**0 CRITICAL · 1 HIGH · 2 MEDIUM · 2 LOW.**

| Dimension | Area | Findings |
|---|---|---|
| 3 | GPU-struct layout / shader-binary lockstep | 1 MEDIUM |
| 15 | Water | 1 MEDIUM, 1 LOW |
| 16 | Volumetrics | 1 HIGH |
| — | Audit-skill drift | 1 LOW |
| 1, 2, 4–14, 17–23 | — | clean (see §2 / §3) |

Every severity floor this audit exists to guard came back clean:

- **AS/SSBO index contract** (CRITICAL floor) — `instance_custom_index`
  still equals the compacted SSBO draw index; `MAX_INSTANCES = 0x40000`
  remains under the 24-bit field; the water path's parallel
  `instance_index` contract (`water_commands_match_draw_slots`) holds and
  `sort_draw_commands` still runs *before* `reemit_water_planes` in
  `build_render_data`.
- **GPU-struct lockstep** (HIGH floor) — `GpuInstance` 128 B across all
  five GLSL mirrors (all carry `surfaceId`), `GpuMaterial` 348 B / 87
  fields with name *and* type parity against `include/bindings.glsl`,
  `GpuCamera` 352 B with `renderDebug` present in all five `CameraUBO`
  declarations, `GpuWaterParams` 352 B / 22 slots identical in
  `water.vert` and `water.frag`.
- **AS build → shader read barriers** (HIGH floor) — present at all build
  sites. Source-read only.
- **Volumetrics teardown** — `VolumetricsPipeline::destroy` drains all six
  froxel-volume vectors plus both noise volumes and every owned buffer; the
  three boundary-geometry buffers are borrowed, not owned. No leak on the
  resize destroy/recreate path.

### The finding that matters

**REN-D16-01 (HIGH)** is an accounting failure, not a code bug, but its
magnitude makes it the one item worth acting on. Session 70 grew the
volumetric froxel grid along two axes at once — the default
`froxel_xy_divisor` went 8 → 4 (a 4× froxel-count increase) and the
per-FIF volume set went from 2 to 6 (44 B/froxel instead of 16 B/froxel).
`docs/engine/memory-budget.md`'s VRAM ledger still carries the pre-Session-62
row (`~29.5 MB (1080p) / ~118 MB (4K)`). Actual allocation is **~730 MB at
1080p native and ~2.92 GB at 4K native** — a 24.7× understatement in the
doc the audit skill designates as authoritative for VRAM ceilings, and
enough on its own to break the documented `< 4 GB` target at 4K.

### The finding that is a fresh regression

**REN-D3-01 (MEDIUM)** is the invariant the 2026-08-16 audit explicitly
verified clean and that has broken inside the delta. `triangle.frag.spv`
was last regenerated at `3d3e3a7b` (2026-08-16); `2325c1de` (2026-08-17)
then bumped `RENDER_DEBUG_MODE_MAX` from 7 to 8 and never recompiled it.
The committed binary and its source now disagree by exactly one
instruction. The existing stale-`.spv` guard (`reflect.rs`, #1447) pins
*block sizes*, so a `#define` value change is structurally invisible to it.

### Structural observation

The water and volumetrics code that landed this session is unusually well
guarded for new work — `water.rs` alone ships 30+ source-level regression
tests, `resize.rs` pins the water set-2 rebind shape with two dedicated
tests, and `predicates.rs`/`ray_budget.rs` both shipped guards alongside
their fixes. What has *not* kept pace is the surrounding documentation:
`memory-budget.md`'s ledger, `audit-renderer/SKILL.md`'s Dimension 16
froxel figures, `_audit-common.md`'s volumetrics and shader-include rosters,
and `WaterPipeline`'s own doc comment all describe a renderer that stopped
existing three days ago. Two of the five findings below are that.

### Prior-audit disposition (2026-08-16)

| Finding | Issue | Status at HEAD |
|---|---|---|
| REN-D5-01 (HIGH) `compute_blas_budget` picks the BAR aperture | #3043 | **FIXED** — now `device_local_heap_bytes_for_memory_type_bits`, filtered by the AS-storage buffer's `memory_type_bits` |
| REN-D2-01 (MEDIUM) missing GPU timer disables GI | #3044 | **FIXED** — `observe`'s `None` arm calls `spend_stable_headroom(2)`; pinned by `missing_timer_samples_promote_gi_to_the_normal_budget` |
| REN-D9-01 (LOW) FxHash conversion one field short | #3045 | **Unchanged** — `skin_dispatch_seen_scratch`, `skin_built_this_frame_scratch`, `AccelerationManager::skinned_blas` are still `std::collections::*`. OPEN, skipped per dedup rule |
| REN-DOC-01 (LOW) SKILL.md checklist items describe deleted code | #3046 | **Unchanged**, OPEN, skipped. REN-DOC-02 below is a *new* instance in a different dimension |
| REN-DOC-02 (LOW) shader-include roster | #3047 | **Unchanged and worse** — the roster listed 9 of 12; there are now **14** headers (`caustic_kernel.glsl` and `mesh_id.glsl` added this delta). OPEN, skipped |

Other open issues touched and deliberately skipped: **#2763** (water.vert's
stale "112-byte invariant" comment, still citing the removed
*gpu_instance_is_112_bytes_std430_compatible*) — still present verbatim at
`water.vert` lines 47-48. **#2787** (water.frag ampScale/freqScale sentinel
lockstep) — REN-D15-01 below is adjacent but distinct; it is about the
struct/array contract, not the sentinel constants.

One open issue looks **already fixed** and is worth re-checking before the
next sweep: **#2767** (both SVGF passes masking mesh-ID bit 31 off before
comparing) — `crates/renderer/shaders/include/mesh_id.glsl` now exists with
`meshIdHasStableHistory` / `stableMeshIdsMatch`, and `svgf_temporal.comp`,
`svgf_atrous.comp` and `taa.comp` all `#include` it. Not reported as a
finding; flagged for closure.

---

## 2. RT Pipeline Assessment

**BLAS/TLAS.** No change in shape this delta beyond the budget-derivation
fix. `blas_static.rs` / `blas_skinned.rs` still route eviction and drop
through their deferred-destroy queues; no immediate
`destroy_acceleration_structure` appears at any eviction or drop site. The
`compute_blas_budget` rewrite (see the table above) is the substantive
change and it is correct: the probe buffer's `memory_type_bits` now
constrains which heap is measured, so a host-visible BAR aperture that the
AS-storage usage cannot land in is no longer eligible. The helper uses
`.find()` (first compatible DEVICE_LOCAL type) rather than a max, which
matches the driver's own preference ordering and therefore matches what the
allocator will pick.

**SSBO indexing.** Verified end to end for the two parallel index spaces:
`GpuInstance[instance_custom_index]` for the main path, and the water
path's separate `WaterPush.waterIndex` → `waterParams.params[]` UBO index.
The latter is bounded on both ends by the same `.take(MAX_WATER_DRAWS)` —
`upload_params` truncates the upload and `geometry_pass.rs`'s draw loop
truncates the enumeration — so no shader index can escape the 186-element
array. Over-cap is a `Once`-gated `warn!`, not a panic.

**Ray queries.** `rt_live = ray_query_supported && tlas_written[frame]`
gates the entire water draw, matching the `sceneFlags.x` gate everywhere
else; the follow-up for a shader-side `sceneFlags.x < 0.5` early-out is
documented in place as needing RenderDoc / non-RT verification and is not
re-reported here. The adaptive ray budget's tier-0 settings still ship
`max_path_segments: 0` by design, but the `None`-timer arm no longer
selects tier 0 (see #3044).

**Denoiser.** SVGF history ping-pong, the stable-surface-ID disocclusion
path and the pre-`hasHistory` firefly clamp are intact. The new
`mesh_id.glsl` helper centralises the bit-31 semantics that #2767 was
filed against.

**Caustics.** Both writers (`caustic_splat.comp` for glass,
`water.frag` for water) share `CAUSTIC_FIXED_SCALE` and now share the
5×5 Gaussian footprint via the new `include/caustic_kernel.glsl`.
`composite.frag` promotes both accumulators to float before summing
(#1575), divides by the shared scale, applies the combined firefly cap, and
adds the result alongside `direct` — never into the SVGF-denoised indirect.
The `caustic_flags.x` gate correctly suppresses the water read when binding
8 is fallback-bound to the same view as binding 5, so the glass term cannot
be double-counted.

---

## 3. GPU-Struct, Shader-Binary & Memory Assessment

**Layout pins — all verified by script, not by reading.**

| Struct | Sites | Result |
|---|---|---|
| `GpuInstance` | `include/bindings.glsl`, `triangle.vert`, `ui.vert`, `water.vert`, `caustic_splat.comp` | 5/5 identical field lists, all carry `surfaceId`, all end `uvec4 _reserved; // offset 120 -> total 128` |
| `GpuMaterial` | `material.rs`, `include/bindings.glsl` | 87 fields, 348 B, name **and** type parity (the GLSL uses comma-lists, so a naive parser under-reads it — the script expands them) |
| `GpuCamera` / `CameraUBO` | `include/bindings.glsl`, `triangle.vert`, `water.vert`, `cluster_cull.comp`, `caustic_splat.comp` | 3 × mat4 + 10 × vec4 = 352 B; `uvec4 renderDebug` present and last in all five |
| `GpuWaterParams` / `WaterParams` | `water.rs`, `water.vert`, `water.frag` | 22 × 16 B = 352 B, identical order including `uvec4 noise_indices` at slot 9 — **but unguarded**, see REN-D15-01 |
| `include/shader_constants.glsl` | generated by `build.rs` | every literal-valued `#define` matches its `shader_constants_data.rs` constant; the 117 apparent mismatches are all expression-valued constants the comparator cannot evaluate, spot-checked correct |

**Committed SPIR-V is stale — one file.** See REN-D3-01. The other 20 are
byte-identical to a fresh compile of their current source.

**Memory.** Volumetrics is the whole story this delta — see REN-D16-01.
The lifecycle itself is sound: `resize.rs` destroys and recreates the
`VolumetricsPipeline` wholesale (its XY follows the render extent),
re-runs `initialize_layouts`, and rewrites the composite descriptors for
the new views. `destroy` drains all six volume vectors. The water side is
equally clean: `WaterPipeline` is destroyed and rebuilt on resize,
`WaterCausticAccum::recreate_on_resize` is called, and the set-2 rebind is
gated on `self.water` alone rather than the accumulator (#2142), with two
source-level tests pinning that shape. `GpuBuffer::create_host_readback` —
new this delta for the combustion light-moment readback — handles
`nonCoherentAtomSize` alignment on both flush and invalidate and is tested
for the sub-atom case.

---

## 4. Findings

### REN-D16-01: memory-budget.md's volumetrics VRAM row understates the froxel grid by ~24×, breaking the documented 4 GB ceiling at 4K

- **Severity**: HIGH
- **Dimension**: Volumetrics / Memory
- **Location**: `docs/engine/memory-budget.md` (the "Volumetrics (M55)"
  section and the "VRAM Rough Budget" ledger row),
  `crates/renderer/src/vulkan/volumetrics.rs`
  (`VolumetricsPipeline::new`, `FROXEL_FORMAT`, `COMBUSTION_FIELD_FORMAT`,
  `EMISSION_HISTORY_FORMAT`),
  `crates/renderer/src/vulkan/upscaling.rs` (`VolumetricsConfig::default`)
- **Status**: NEW (introduced by `0ff7b537`, 2026-08-17)
- **Description**: The volumetric froxel grid grew along two independent
  axes inside this delta and neither growth reached the VRAM ledger.
  (a) `VolumetricsConfig::default`'s `froxel_xy_divisor` went **8 → 4** in
  `0ff7b537`, quadrupling the froxel count (`validate`'s lower bound was
  simultaneously relaxed from 4 to 2). (b) The per-FIF volume set went from
  two (`lighting_volumes`, `integrated_volumes`) to **six** — the same two
  plus `emission_history_volumes` (`R32_SFLOAT`),
  `combustion_state_volumes`, `combustion_dynamics_volumes` and
  `combustion_optical_volumes` (all `R16G16B16A16_SFLOAT`). Per-froxel cost
  per FIF is therefore **44 B**, not the documented 8 B × 2 volumes = 16 B.
  The prose in the Volumetrics section was updated for (a) but still reads
  "Two volumes per frame (lighting + integrated) × 2 FIF", and the
  `VRAM Rough Budget` ledger row was never updated for either — it still
  carries `~29.5 MB (1080p) / ~118 MB (4K)`, which is the figure for the
  *pre-Session-62* divisor-12, two-volume grid (160×90×64 × 8 B × 2 × 2).
- **Evidence**: Six `Self::create_volume(...)` calls inside the
  `for i in 0..MAX_FRAMES_IN_FLIGHT` loop in `VolumetricsPipeline::new`,
  pushing to six distinct `Vec<FroxelSlot>` fields; the code's own comment
  above that loop reads "Six volumes per frame". `MAX_FRAMES_IN_FLIGHT = 2`
  (`sync.rs`). `froxel_extent` = `render.{width,height}.div_ceil(4) ×
  froxel_z_slices (64)`. Arithmetic on the doc's own decimal-MB basis:

  | Render extent | Froxels | Documented (ledger) | Documented (section table, 2 volumes) | Actual (6 volumes, 2 FIF) |
  |---|---|---|---|---|
  | 1920×1080 | 480×270×64 = 8 294 400 | ~29.5 MB | ~265.4 MB | **~729.9 MB** |
  | 2560×1440 | 640×360×64 = 14 745 600 | — | ~471.9 MB | **~1.30 GB** |
  | 3840×2160 | 960×540×64 = 33 177 600 | ~118 MB | ~1061.7 MB | **~2.92 GB** |

  The commit that made the divisor change describes it as "Adjusted froxel
  grid configuration to improve memory usage and performance"; at a fixed
  `froxel_z_slices`, halving the XY divisor does the opposite by 4×.
- **Impact**: The ledger's `Estimated total ~1.59 GB` becomes ~2.29 GB at
  1080p native once volumetrics is counted correctly, and the `< 4 GB
  target` peak column is broken by volumetrics alone at 4K (2.92 GB before
  ReSTIR's 531 MB, SVGF's 332 MB, textures, BLAS…). The grid keys on
  *render* extent, so FSR Quality at 1080p output softens it to ~324 MB —
  still 11× the ledger row. On the documented 6 GB RT-minimum card this is
  the difference between comfortable and tripping the allocator's own 80%-
  of-heap warning. Nothing here is a leak or a correctness bug; the cost is
  real, intended and freed correctly. What is broken is that the project's
  authoritative VRAM analysis no longer describes the engine, so no future
  budget decision made from it is sound.
- **Related**: #2801 / #2679 (the same class of ledger drift, both closed),
  `docs/engine/memory-budget.md`, `feedback_vram_baseline.md`
- **Suggested Fix**: Update both the Volumetrics section (six volumes,
  44 B/froxel/FIF, corrected table) and the `VRAM Rough Budget` row and
  total. Separately, confirm the 8 → 4 divisor default was an intentional
  quality decision rather than a sign flip — it is a 4× VRAM *and* 4× inject-
  dispatch-workload change shipped under a commit message describing a
  memory improvement, and it is not covered by any bench-of-record refresh.

---

### REN-D3-01: committed `triangle.frag.spv` is stale — the shipped binary and its source disagree by one constant

- **Severity**: MEDIUM
- **Dimension**: GPU-Struct Layout / shader-binary lockstep
- **Location**: `crates/renderer/shaders/triangle.frag.spv` vs
  `crates/renderer/shaders/triangle.frag` (the `RENDER_DEBUG_MODE_MAX`
  guard in `main()`), `crates/renderer/src/shader_constants_data.rs`
  (`RENDER_DEBUG_VOLUMETRIC_TERM`, `RENDER_DEBUG_MODE_MAX`)
- **Status**: NEW — regression of the invariant
  `docs/audits/AUDIT_RENDERER_2026-08-16.md` §3 verified clean four days ago
  ("all 21 are byte-identical")
- **Description**: `2325c1de` (2026-08-17) added
  `RENDER_DEBUG_VOLUMETRIC_TERM = 8` and redefined
  `RENDER_DEBUG_MODE_MAX = RENDER_DEBUG_VOLUMETRIC_TERM`, which `build.rs`
  duly regenerated into `include/shader_constants.glsl`. `composite.frag`
  was recompiled and its `.spv` is current. `triangle.frag.spv` was not —
  it was last regenerated at `3d3e3a7b` (2026-08-16). The shipped binary
  therefore still encodes the *old* bound.
- **Evidence**: Recompiling all 21 sources with
  `glslangValidator -V -I. <shader>` and `cmp`-ing against the committed
  binaries yields 20 identical and one differing. `spirv-dis` narrows the
  difference to a single instruction:

  ```
  committed:  %7626 = OpUGreaterThan %bool %7625 %uint_7
  fresh:      %7626 = OpUGreaterThan %bool %7625 %uint_8
  ```

  which is `if (!legacyDebugMode && debugMode > RENDER_DEBUG_MODE_MAX)` —
  the "unrecognised structured mode" contract-failure branch that writes
  magenta and returns.
- **Impact**: Bounded today. `r.debug volumetric` (`RenderDebugMode::VolumetricTerm`,
  `render_debug.rs`) makes the shipped `triangle.frag` treat mode 8 as
  corrupt: every fragment takes the magenta early-out. All eight MRTs are
  still written before that return (locations 6/7 at the top of `main`,
  1/2/3 before the guard, 0/4/5 inside it), and `composite.frag`'s mode-8
  branch returns the mapped froxel field without reading `direct4`, so the
  displayed image is unaffected — the geometry pass is simply doing no work
  for that view. The real cost is the broken invariant: source and shipped
  binary disagree, nothing in `cargo test` notices, and the next person to
  recompile `triangle.frag` for an unrelated reason ships an unreviewed
  behaviour change bundled with theirs. The existing stale-`.spv` guard
  (`reflect.rs`, #1447 — `every_committed_spv_*` block-size pins) is a
  *struct-size* check and is structurally blind to `#define` value drift;
  `shader_constants.rs`'s `correctness_debug_views_require_raw_frame_graph_output`
  loop over `1..=RENDER_DEBUG_MODE_MAX` reads GLSL *source*, not SPIR-V.
- **Related**: #1447 (the last stale-`.spv` incident and the guard it
  produced), REN-DOC-01 / #3046,
  `feedback_triangle_frag_spv_recompile.md`
- **Suggested Fix**: Recompile `triangle.frag.spv` with the documented
  plain `-V` invocation (not `-g0`; the reflection test needs `OpName`).
  Then close the class: add a test that recompiles each GLSL source at test
  time and byte-compares, or — if a build-time glslang dependency is
  unwanted — extend `build.rs` to fail the build when
  `shader_constants.glsl` is regenerated with different content while any
  `.spv` predates the change.

---

### REN-D15-01: `GpuWaterParams` has three declaration sites and no lockstep guard, on the two most-edited shader files of the delta

- **Severity**: MEDIUM
- **Dimension**: Water / GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/water.rs` (`GpuWaterParams`,
  `MAX_WATER_DRAWS`, `water_gpu_contract_layouts_are_stable`,
  `water_vertex_shader_keeps_the_full_material_array_stride`),
  `crates/renderer/shaders/water.vert`, `crates/renderer/shaders/water.frag`
  (both `struct WaterParams` + `WaterParams params[186]`)
- **Status**: NEW
- **Description**: `GpuWaterParams` is a 352-byte std140 UBO record declared
  in three places — once in Rust and hand-mirrored in `water.vert` and
  `water.frag` — with the same blast radius as `GpuInstance` and
  `GpuMaterial`, but none of their protection. `GpuInstance` has
  `gpu_instance_glsl_copies_stay_in_lockstep` (parses all five GLSL
  mirrors); `GpuMaterial` has the #1657 cross-check that parses both
  declarations and compares field-for-field. `GpuWaterParams` has neither.
  What exists instead is `water_vertex_shader_keeps_the_full_material_array_stride`,
  which asserts that eight `vec4 <name>;` substrings appear *somewhere* in
  `water.vert` — it cannot detect a reordered slot, a `vec4`↔`uvec4` type
  flip, or a slot inserted mid-struct in one mirror only. Separately, the
  UBO array bound is the bare literal `186` in both shaders, with no pin
  against `MAX_WATER_DRAWS`; `water_gpu_contract_layouts_are_stable` checks
  only that `MAX_WATER_DRAWS × 352 ≤ 64 KiB`, which stays true if the
  constant and the shaders diverge.
- **Evidence**: `grep -c` over the three declarations gives 22 slots each;
  a field-by-field script diff (Rust snake_case → GLSL snake_case, including
  `uvec4 noise_indices` at slot 9) shows **zero drift at HEAD** — the
  contract is currently intact. The gap is the absence of a guard, on files
  that took 29 (`water.frag`) and 15 (`water.vert`) commits in this delta
  alone. `grep -n "186" crates/renderer/src/vulkan/water.rs` returns only
  the `MAX_WATER_DRAWS` definition; `grep -n "params\[186\]"` over
  `crates/renderer/shaders/` returns two hand-written sites.
- **Impact**: Defense-in-depth only — nothing is broken today. But per
  `feedback_shader_struct_sync.md` this hand-mirrored-GLSL-struct pattern is
  the project's documented #1 source of silent GPU desync, and a slot
  inserted into `water.frag`'s mirror but not `water.vert`'s would shift
  every subsequent `vec4` in the vertex stage by 16 bytes with no test
  failure and no validation error — it would surface as wrong wave
  amplitude or wrong scroll velocity, i.e. as a *tuning* bug, which is the
  hardest kind to trace back to a layout cause.
- **Related**: OPEN #2787 (water.frag ampScale/freqScale sentinels,
  tautological test) — adjacent but about shader-internal constants, not the
  struct/array contract; `feedback_shader_struct_sync.md`
- **Suggested Fix**: Extend the existing `parse_rust_struct_fields` /
  `parse_glsl_struct_fields` machinery in
  `scene_buffer/gpu_instance_layout_tests.rs` to cover `GpuWaterParams`
  against both `water.vert` and `water.frag`, and add an assertion that both
  shaders contain `params[{MAX_WATER_DRAWS}]` built from the constant.

---

### REN-D15-02: `WaterPipeline`'s doc comment tells the reader the opposite of what `resize.rs` now does

- **Severity**: LOW
- **Dimension**: Water / documentation accuracy
- **Location**: `crates/renderer/src/vulkan/water.rs` — the doc comment
  above `pub struct WaterPipeline`
- **Status**: NEW
- **Description**: The comment reads: *"Extent-independent: … no descriptor
  binds reference a fixed-extent resource … No `recreate_on_resize` method
  exists — and intentionally so … if water ever picks up such a resource
  (e.g. a dedicated caustic accumulator), wire the resize hook at that
  time."* Every clause is now false. Water owns exactly the resource the
  comment names as the hypothetical trigger — the per-FIF screen-sized
  `R32_UINT` `WaterCausticAccum`, bound as set 2 via
  `update_water_caustic_descriptors` — and `context/resize.rs` already
  handles it: the pipeline is destroyed and rebuilt (it depends on the
  render pass), `WaterCausticAccum::recreate_on_resize` runs, and set 2 is
  rebound to the new views under a fallback that survives the accumulator
  going away (#2142).
- **Evidence**: `resize.rs` — `if let Some(mut old) = self.water.take()` →
  `WaterPipeline::new(...)`; then the `#1255 / Phase C of #1210` block
  calling `wca.recreate_on_resize`; then the `if let Some(w) =
  self.water.as_ref()` block calling `update_water_caustic_descriptors`.
  Two tests pin that shape:
  `water_caustic_rebind_is_not_gated_on_accumulator_presence` and
  `init_path_water_set_2_falls_back_and_drops_the_stale_comment`.
- **Impact**: Documentation only, but of the actively-misleading kind: the
  comment instructs a future reader to *add* a resize hook when a condition
  is met, and that condition has been met for some time by code the comment
  is unaware of. A reader trusting it would either duplicate the existing
  handling or conclude the accumulator is unhandled.
- **Related**: #1130 / REN-D17-NEW-01 (the finding the comment was written
  to close), #2142
- **Suggested Fix**: Rewrite the comment to state what is true — water owns
  a fixed-extent resource, is destroyed/recreated by `recreate_swapchain`,
  and its set 2 is rebound there — and point at the two guard tests.

---

### REN-DOC-03: `audit-renderer/SKILL.md` Dimension 16 and `_audit-common.md` describe a froxel grid three defaults out of date

- **Severity**: LOW
- **Dimension**: Audit-skill drift
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (Dimension 16
  checklist, Dimension 3 `CameraUBO` bullet),
  `.claude/commands/_audit-common.md` (the `Volumetrics(M55)` project-layout
  row)
- **Status**: NEW (distinct from OPEN #3046, which covers Dimension 2/17/18
  items)
- **Description**: Three separate drifts, all of which will manufacture
  false positives on the next run:
  1. Dimension 16 states the froxel grid is *"`froxel_xy_divisor` /
     `froxel_z_slices` (defaults 12 / 64 …), so 160×90×64 at 1080p native"*.
     The live default is **4 / 64** → 480×270×64. The `12` figure predates
     even the previous default of `8`.
  2. `_audit-common.md`'s Volumetrics row still says *"160×90×128 froxel
     grid"* — wrong on both the XY derivation and the Z-slice count, and
     wrong about the volume count now that six volumes exist per FIF.
  3. Dimension 3 says *"all 6 shaders that re-declare `CameraUBO` —
     `triangle.vert`, `triangle.frag`, `water.vert`, `water.frag`,
     `cluster_cull.comp`, `caustic_splat.comp`"*. There are **five**
     declaration sites: `include/bindings.glsl`, `triangle.vert`,
     `water.vert`, `cluster_cull.comp`, `caustic_splat.comp`.
     `triangle.frag` and `water.frag` now obtain it by `#include`. The
     2026-08-16 report already recorded the correct count of five.
- **Evidence**: `grep -n "froxel_xy_divisor" crates/renderer/src/vulkan/upscaling.rs`
  → `froxel_xy_divisor: 4` in `Default::default`, and
  `assert_eq!(config.froxel_xy_divisor, 4)` in its own test.
  `grep -rn "uniform CameraUBO" crates/renderer/shaders/` → five hits.
- **Impact**: A checklist that quotes stale numbers is worse than one that
  quotes none — the Dimension 16 froxel figures are exactly the numbers this
  audit had to re-derive from source to produce REN-D16-01, and an auditor
  who trusted the skill would have concluded the grid was 9× smaller than it
  is and missed the finding entirely.
- **Related**: OPEN #3046 (REN-DOC-01), OPEN #3047 (REN-DOC-02 — the
  shader-include roster, now stale by five: `caustic_kernel.glsl` and
  `mesh_id.glsl` joined the 12 it already under-counted)
- **Suggested Fix**: Correct all three, and consider replacing the quoted
  defaults with a pointer to `VolumetricsConfig::default` the way the skill
  already does for `DBG_BITS` ("read `DBG_BITS` rather than trusting any
  figure quoted here").

---

## 5. Prioritized Fix Order

1. **REN-D3-01** — recompile `triangle.frag.spv`. One command, and it
   restores an invariant that a prior audit had certified. Then close the
   class with a recompile-and-compare test, because the existing guards
   provably cannot see this failure mode.
2. **REN-D16-01** — correct `memory-budget.md`, and confirm the 8 → 4
   divisor default was deliberate. The doc fix is mechanical; the second
   half is a decision, not an edit.
3. **REN-D15-01** — extend the existing struct-lockstep test machinery to
   `GpuWaterParams`. Cheap, and it covers the two files most likely to keep
   changing.
4. **REN-DOC-03**, **REN-D15-02** — documentation corrections.

## 6. Needs-RenderDoc

Nothing new. No barrier, layout or render-pass change is proposed by this
audit. Two pre-existing items remain in that category and are recorded here
only so they are not lost:

- The shader-side `sceneFlags.x < 0.5` early-out for `water.frag`, which
  `geometry_pass.rs` documents in place as a follow-up requiring RenderDoc /
  non-RT verification.
- The cross-frame `lighting_volumes[previous]` / `combustion_*[previous]`
  history reads in `VolumetricsPipeline::dispatch`. These rely on
  submission-order pipeline barriers rather than a fence, which is correct
  per spec for same-queue submissions, but the pattern's correctness is not
  observable from `cargo test` and the combustion fields are new this delta.

## 7. Coverage

All 23 dimensions were examined. Dimensions 2, 3, 4, 14, 15 and 16 received
the delta-weighted deep treatment requested in the briefing, including the
two mechanical verifications described in the header. Dimensions 1, 5, 8–13
and 17–23 were checked against their regression-guard sets and the delta's
diff surface; the guards named in the skill were confirmed present by symbol
grep, and no divergence was found. Dimensions 6, 7 and 19 (NIFAL material,
material table, tangent space) saw no meaningful change in this delta and
are covered in depth by `/audit-nifal` and `/audit-nif`.

**Not covered**: no Vulkan device, no `BYRO_VALIDATION` run, and no
`cargo test` execution (all three excluded by the suite briefing). Every
barrier and layout verdict is source-read only. The FSR FP32 permutation
(Dimension 23) remains unexercised — it needs a GPU without `shaderFloat16`
and the dev box has one; carried scope, not a finding.

TALLY: CRITICAL=0 HIGH=1 MEDIUM=2 LOW=2
