# Renderer Audit — 2026-08-16

**Scope**: `/audit-renderer` (all 23 dimensions), run as part of the
`comprehensive` audit-suite sweep.

**Repo state**: HEAD `85b77371`, branch `main`, clean tree apart from the
in-flight `.claude/commands/` edits. Dedup baseline: `/tmp/audit/issues.json`
(269 OPEN issues, fetched 2026-08-16) plus the 2 000-entry closed-issue cache
and every prior `docs/audits/AUDIT_RENDERER_*.md`.

**Verification**: `cargo test -p byroredux-renderer --lib` → **674 passed,
0 failed**. All 21 GLSL sources were recompiled locally and byte-compared
against their committed `.spv` (see §3).

---

## 1. Executive Summary

**0 CRITICAL · 1 HIGH · 1 MEDIUM · 3 LOW.**

| Dimension | Area | Findings |
|---|---|---|
| 2 | SSBO/Index plumbing & RT ray queries | 1 MEDIUM |
| 5 | GPU memory & resource lifecycle | 1 HIGH |
| 9 | GPU skinning + BLAS refit | 1 LOW |
| — | Audit-skill / reference-doc drift | 2 LOW |
| 1, 3, 4, 6–8, 10–23 | — | clean (see §2 / §4) |

Every severity floor this audit exists to guard came back clean:

- **AS/SSBO index contract** (CRITICAL floor) — `instance_map` is 1:1 with
  `draw_commands` under a `debug_assert_eq!`, the 24-bit `MAX_INSTANCES`
  const-assert holds, and `#2913`'s pin landed at `7d1c4f51`.
- **GPU-struct lockstep** (HIGH floor) — `GpuInstance` 128 B, `GpuCamera` 336 B,
  `GpuMaterial` 348 B, all five GLSL `GpuInstance` mirrors carry `surfaceId`,
  and `gpu_instance_glsl_copies_stay_in_lockstep` parses every mirror.
- **AS build → shader read barriers** (HIGH floor) — present at all build sites,
  including the two barriers newly added for the cluster-cull telemetry buffer.
  Source-read only; see the verification caveat below.
- **Deferred AS destruction** — `evict_unused_blas`, `drop_blas` and the retired
  `blas_scratch_buffer` all route through their deferred queues; no immediate
  `destroy_acceleration_structure` at any eviction or drop site.

### The two findings that matter

**REN-D5-01 (HIGH)** is a fresh regression introduced by `9aea0aa0` two commits
ago. `compute_blas_budget` was switched from *summing* DEVICE_LOCAL heaps to
taking the **smallest** one — but on the exact AMD/hybrid layout the fix's own
doc comment cites as motivation, the smallest DEVICE_LOCAL heap **is** the
~256 MB host-visible BAR aperture, not main VRAM. The budget therefore collapses
to the `MIN_BLAS_BUDGET_BYTES` floor on cards with 8–24 GB of usable VRAM. It
cannot be observed on the RTX 4070 Ti dev card, which reports a single heap —
which is precisely why it needs to be caught here rather than at runtime.

**REN-D2-01 (MEDIUM)** is a coupling nobody appears to have noticed when the
adaptive ray budget landed (`5798e467`, extended `9bf7d024`). The controller's
*only* input is `GpuPerFrameTimers`, an explicitly best-effort subsystem whose
documented failure mode is still "instrumentation will read zeros". Since
`AdaptiveRayBudget::observe` early-returns on `None`, a device without timestamp
queries pins `quality_tier` at 0 forever — and tier 0 ships
`max_path_segments: 0`, which `triangle.frag` reads as "skip the entire
one-bounce GI path". A best-effort telemetry subsystem is now load-bearing for
the render output, and nothing says so at either end.

### Structural observation

The renderer's own regression-guard discipline is in good shape and getting
better — `9bf7d024` and `999478ef` both shipped source-level guard tests
alongside their fixes, and those guards are what let this audit clear Dimensions
1/3/12/13/16/17/19 quickly. What has *not* kept pace is
`.claude/commands/audit-renderer/SKILL.md` itself: three of its checklist items
now describe code that was deliberately deleted in the last four days, and one of
them (`isInteriorFill`) was already reported stale on 2026-08-12 — the
*replacement* text that report recommended has since gone stale too. A checklist
that describes deleted code manufactures false positives for every future run.

### Verification caveats — read before acting on the barrier verdicts

- **No Vulkan device and no `BYRO_VALIDATION` run backed this audit.** Every
  barrier / layout verdict is source-read only. Per the project's standing
  no-speculative-Vulkan-fixes rule, treat them as "no defect visible in source",
  not "confirmed correct".
- **REN-D5-01 is not reproducible on the dev card.** It is derived from the
  Vulkan memory-heap model plus the function's own stated AMD motivation, not
  from an observed run. Confirming it needs an AMD (or hybrid-laptop) part, or a
  `VK_LAYER_LUNARG_device_simulation` profile with two DEVICE_LOCAL heaps.
- **The BLAS eviction machinery remains unexercised** on 12 GB — verified through
  `predicates.rs` and its unit tests, not under pressure. REN-D5-01 is
  specifically the finding that would make it exercised on other hardware.

---

## 2. RT Pipeline Assessment

**BLAS/TLAS.** `build_tlas`'s BUILD/UPDATE bookkeeping commit point is still
after `cmd_build_acceleration_structures` (#2674), `built_primitive_count`
matching is guarded in both directions (VUID-03708), and the empty-TLAS
`copy_size > 0` guard is intact. `999478ef` closed the two LOWs the 2026-08-14
rt-deep pass filed: the dead single-shot `build_blas` path is gone, and
`shrink_tlas_scratch_to_fit`'s live-slot arm no longer panics inside an open
command buffer. OPEN #2769 (the second LRU-stamp pass over `draw_commands`) is
still live and was skipped per the dedup rule.

**SSBO indexing.** `instance_custom_index` still equals the compacted SSBO draw
index; `build_instance_map` is now pinned by `#2913`'s assertion. `MAX_INSTANCES
= 0x40000` remains under the 24-bit field.

**Ray queries.** The glass path was substantially rewritten at `9bf7d024`:
material identity no longer depends on `roughness` or `rtLOD`, the tier-3
`isGlass = false` demotion is gone, and interface depth is now
`refractPassthruBudget = 2 + budgetTier * 2`. Two new source-level guards
(`triangle_frag_keeps_glass_identity_and_ior_across_rt_lods`,
`triangle_frag_scales_glass_interface_depth_with_honest_ray_cost`) pin the new
shape and explicitly assert `rtLOD < RT_LOD_IOR` never returns. The atomic
`rayBudgetCount` is documented as telemetry-only and confirmed never read back
by the CPU.

**Denoiser.** SVGF history ping-pong, the stable-surface-ID disocclusion path,
and the pre-`hasHistory` firefly clamp are all intact. OPEN #2767 (both SVGF
passes masking bit 31 off before comparing mesh IDs) is unchanged — skipped.

**ReSTIR-DI.** The 25° geometric-normal spatial-reuse cone and the
`inst.surfaceId & RESERVOIR_SURFACE_MASK` history tag both hold; the reservoir
is still 32 B.

---

## 3. GPU-Struct, Shader-Binary & Memory Assessment

**Layout pins.** All size and per-field-offset assertions pass. The five GLSL
`GpuInstance` declaration sites are exactly `include/bindings.glsl`,
`triangle.vert`, `ui.vert`, `water.vert`, `caustic_splat.comp`, and all five carry
`surfaceId`. `GpuLight` has four declaration sites, `CameraUBO` five — both match
the counts in their Rust doc comments. `reflect.rs`'s SPIR-V-reflection tests
cross-check `GpuCamera` and `GpuDalcCube` sizes against the committed binaries.

**Committed SPIR-V is not stale — verified, not assumed.** This audit added a
check the test suite does not perform. Every one of the 21 GLSL sources was
recompiled with the documented command (`glslangValidator -V -I <shaders>`) and
compared to its committed `.spv`: **all 21 are byte-identical.** As a second,
independent check, each source was also compiled *as of the commit that last
touched its `.spv`* and compared against HEAD — all 21 identical, so no semantic
drift has accumulated in any source since its binary was last regenerated. Eight
sources have a newer last-change commit than their `.spv` (`triangle.vert`,
`water.vert`, `composite.frag`, `bloom_upsample.comp`, `skin_palette.comp`,
`svgf_atrous.comp`, `svgf_temporal.comp`, `volumetrics_inject.comp`) but all
eight of those changes were comment-only. Nothing in `cargo test` enforces this
— `every_committed_spv_is_spirv_1_0` only checks the version word — but the
invariant holds today, so this is recorded as a verified-clean result rather than
a finding.

**Memory.** `AllocatorResource` is still removed from the `World` before
`VulkanContext::drop` (`byroredux/src/app_events.rs`). Deferred-destroy queues
for BLAS, skinned BLAS, BLAS scratch and the global geometry SSBO pair are all
present with `DEFAULT_COUNTDOWN`. The one defect is the budget *derivation* —
REN-D5-01.

---

## 4. Findings

### REN-D5-01: `compute_blas_budget` picks the BAR aperture, not VRAM, on any multi-DEVICE_LOCAL-heap GPU

- **Severity**: HIGH
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs`
  (`compute_blas_budget`, `blas_budget_for_heap`),
  `crates/renderer/src/vulkan/device.rs` (`smallest_device_local_heap_bytes`)
- **Status**: NEW (regression introduced by `9aea0aa0`, "Fix #2928")
- **Description**: `#2928` replaced `total_device_local_bytes` (a sum over every
  `DEVICE_LOCAL` heap) with `smallest_device_local_heap_bytes` (a `min` over the
  same set), on the stated grounds that "the common AMD / hybrid layout reports a
  small `DEVICE_LOCAL | HOST_VISIBLE` BAR window alongside the main VRAM heap"
  and that summing therefore over-counts VRAM. That premise is correct; the
  chosen remedy inverts the error instead of removing it. `VK_MEMORY_HEAP_DEVICE_LOCAL_BIT`
  is a *heap* flag and host-visibility is a *memory-type* property, so the small
  BAR aperture is reported as its own `DEVICE_LOCAL` heap — which means `min()`
  selects exactly the window the doc comment is warning about, and discards the
  main VRAM heap the BLAS allocations actually land in.
- **Evidence**:
  ```rust
  // device.rs
  mem_props.memory_heaps[..count].iter()
      .filter(|heap| heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
      .map(|heap| heap.size)
      .min()          // ← on AMD this is the ~256 MB BAR heap, not VRAM
      .unwrap_or(0)

  // predicates.rs
  pub(super) fn blas_budget_for_heap(heap_bytes: vk::DeviceSize) -> vk::DeviceSize {
      (heap_bytes / 3).max(MIN_BLAS_BUDGET_BYTES)   // MIN = 256 MiB
  }
  ```
  On a typical AMD layout (heap 0 = 8–16 GB `DEVICE_LOCAL`, heap 1 = system RAM,
  heap 2 = 256 MB `DEVICE_LOCAL` BAR window) this evaluates to
  `max(256 MiB / 3, 256 MiB) = 256 MiB`, versus the ~2.7–5.3 GB the policy
  intends. `blas_over_budget(static_blas_bytes, pending_bytes, blas_budget_bytes)`
  then drives `evict_unused_blas`.
- **Impact**: On every GPU exposing more than one `DEVICE_LOCAL` heap, static-BLAS
  eviction runs against a 256 MB ceiling regardless of real VRAM. Each eviction
  batch bumps `blas_map_generation` and sets `needs_full_rebuild` on both TLAS
  slots, so a scene whose static BLAS working set exceeds 256 MB — any exterior
  grid, and most FO4/Starfield interiors — enters a permanent
  evict → full-TLAS-rebuild → re-build-BLAS churn. It also makes the
  documented-not-fixed gap in #1793 reachable: a permanently-missing rigid BLAS
  has no recovery path (no per-frame build primitive exists), and #1793 records
  that as safe only because it is "gated behind `static_blas_bytes > budget`,
  unreachable on the 12 GB dev card". With a 256 MB budget it is reachable on a
  24 GB card. Blast radius is "every non-NVIDIA-single-heap GPU"; zero effect on
  the dev machine, which is why no test or bench can see it.
- **Related**: #2928 (the change), #1793 (missing-BLAS no-recovery gap),
  #1572 / `allocator.rs`'s 80 %-of-heap pressure warning, which uses the same
  `min` and has the same defect (log-only, so lower impact — but it is the
  precedent `#2928` cited to justify the switch, so fixing one should fix both).
- **Suggested Fix**: Select the *largest* `DEVICE_LOCAL` heap, or the heap
  backing the memory type gpu-allocator actually uses for `GpuOnly` allocations —
  not `min` and not `sum`. Add a unit test over a synthetic two-heap
  `VkPhysicalDeviceMemoryProperties` (`blas_budget_for_heap` is already pure, so
  only the heap-selection half needs the fixture) asserting a
  `[16 GiB, 256 MiB]` pair yields a multi-GiB budget.

---

### REN-D2-01: A missing GPU timer silently disables one-bounce GI for the whole session

- **Severity**: MEDIUM
- **Dimension**: Ray Queries / SSBO plumbing
- **Location**: `crates/renderer/src/vulkan/scene_buffer/ray_budget.rs`
  (`AdaptiveRayBudget::observe`, `AdaptiveRayBudget::settings`),
  `crates/renderer/src/vulkan/context/draw.rs` (the `measured_lighting_ms`
  binding), `crates/renderer/src/vulkan/context/mod.rs` (the `gpu_timers` match
  arm), `crates/renderer/shaders/triangle.frag` (the GI gate)
- **Status**: NEW
- **Description**: The adaptive ray-budget controller has exactly one input, and
  it is an explicitly best-effort subsystem. `GpuPerFrameTimers::new` returns
  `Ok(None)` when the driver lacks `timestamp_compute_and_graphics`, and the
  construction site swallows a creation error into `None` as well, logging only
  that "PERF-DIM7 instrumentation will read zeros". `draw_frame` derives
  `measured_lighting_ms` from `self.gpu_timers.as_ref().map(..)`, so it is `None`
  on every frame in that case; `observe` then early-returns via
  `let Some(sample) = measured_lighting_ms.filter(..) else { return; }`, leaving
  `tier` at its cold-start value of 0 permanently. There is no time-based or
  frame-count-based fallback promotion.
- **Evidence**:
  ```rust
  // ray_budget.rs — tier 0 is the watchdog-safe cold-start floor
  0 => GpuRayBudget { direct_shadow_samples: 1, max_path_segments: 0,
                      max_shaded_hits: 0, volumetric_light_cap: 2,
                      quality_tier: 0, .. },
  ```
  ```glsl
  // triangle.frag
  if (giRayEnabled
      && rayBudget.maxPathSegments > 0u
      && rayBudget.maxShadedHits > 0u
      && ...) { /* one-bounce GI path */ }
  ```
  Pinned by the crate's own unit test
  `cold_start_uses_the_watchdog_safe_quality_floor`, which asserts exactly those
  zeroes — the values are correct as a *cold start*; the defect is that nothing
  guarantees the controller ever leaves it.
- **Impact**: On any device without timestamp queries (and after any query-pool
  allocation failure on a device that has them), the engine renders with
  one-bounce GI entirely off, one direct shadow sample, a volumetric light cap of
  2, and — since `9bf7d024` tied glass interface depth to the same tier
  (`refractPassthruBudget = 2 + budgetTier * 2`) — the two-interface glass
  allowance whose own commit message describes it as producing "a colour mosaic"
  on multi-submesh Skyrim creatures. The only diagnostic is a `log::warn!` that
  frames the loss as an instrumentation problem. `--cornell` cannot surface it
  either: the harness runs on the same dev card, where timers exist.
- **Related**: #2821 (REN-D20-02, `_active` flags ignored by four telemetry
  readers) is adjacent but distinct — that is about `0.0`-vs-not-run inside a
  *present* snapshot; this is about the snapshot source being absent entirely.
  #2686 (`GLASS_RAY_BUDGET` dead constant) touches the same struct.
- **Suggested Fix**: Give `AdaptiveRayBudget` an explicit no-telemetry default
  tier (a fixed mid tier, or the tier implied by a `--rt-quality` override) and
  make `VulkanContext::new` select it when `gpu_timers.is_none()`, rather than
  leaving the cold-start floor as the permanent steady state. At minimum,
  upgrade the `gpu_timers == None` log line to say that adaptive RT quality is
  pinned, so the symptom is traceable.

---

### REN-D9-01: The #2923 hot-path FxHash conversion stopped one field short of its own hot path

- **Severity**: LOW
- **Dimension**: Skinning / hot-path hygiene
- **Location**: `crates/renderer/src/vulkan/context/mod.rs`
  (`skin_dispatch_seen_scratch`, `skin_built_this_frame_scratch`),
  `crates/renderer/src/vulkan/acceleration/mod.rs` (`skinned_blas`)
- **Status**: NEW
- **Description**: `#2923` moved `SkinSlotPool`'s collections, `FrameInputs.pose_dirty`
  and the `skin_offsets` map to `rustc-hash`, and added
  `pose_dirty_crosses_the_crate_boundary_without_siphash` to pin the crossing.
  Three collections on the same per-frame, per-entity keyspace were not
  converted: `skin_dispatch_seen_scratch` and `skin_built_this_frame_scratch`
  are `std::collections::HashSet<EntityId>` fields of `VulkanContext`, taken and
  returned every frame by `record_skinned_blas_refit`
  (`context/skinned_blas_refit.rs`); and `AccelerationManager::skinned_blas` is a
  `std::collections::HashMap<EntityId, BlasEntry>` probed once per TLAS-eligible
  skinned draw in `build_tlas`'s LRU-stamp loop and again in
  `build_tlas_instances`, plus per entity in `has_skinned_blas`.
- **Evidence**: `grep -rn "std::collections::Hash" crates/renderer/src` returns
  these three alongside genuinely cold users (`reflect.rs`, `texture_registry.rs`,
  `mesh.rs`). The existing guard test asserts only that `draw.rs` and
  `skinned_blas_refit.rs` contain the literal string
  `rustc_hash::FxHashSet<EntityId>`, which those two files satisfy through
  `pose_dirty` alone.
- **Impact**: SipHash-1-3 on `EntityId` keys, several times per skinned entity
  per frame. Bounded and small (tens of NPCs), which is why this is LOW — but it
  is the same pattern #1368/#2174/#2923 were each filed to remove, in the same
  functions, and the guard as written will not notice.
- **Related**: #2923, #2174, #1368; the hot-path hashing rule in
  `.claude/commands/_audit-common.md`.
- **Suggested Fix**: Convert the three to `FxHashSet`/`FxHashMap` and widen the
  existing guard test to assert the absence of `std::collections::HashSet<EntityId>` /
  `HashMap<EntityId` across `context/mod.rs` and `acceleration/mod.rs` too.

---

### REN-DOC-01: Three `audit-renderer/SKILL.md` checklist items describe code deleted in the last four days

- **Severity**: LOW
- **Dimension**: Audit-skill drift
- **Location**: `.claude/commands/audit-renderer/SKILL.md` — Dimension 2
  (thin-glass gate bullet), Dimension 17 (soft-shadow checklist), Dimension 18
  (sky/weather checklist)
- **Status**: NEW for the Dim-2 item; the Dim-17/18 `isInteriorFill` items are
  **Existing** (reported as P-5 in `docs/audits/AUDIT_RENDERER_2026-08-12b.md`)
  but their recommended *replacement* text has itself gone stale since.
- **Description**: Three checklist claims no longer describe the code.
  1. Dim 2 quotes the thin-glass gate as
     `glassIORAllowed = isGlass && !isThinGlass && rtEnabled && !isWindow && rtLOD < RT_LOD_IOR`.
     `RT_LOD_IOR` was deleted at `9bf7d024`; the live expression is
     `isGlass && !isThinGlass && reflectionGlassRayEnabled && !isWindow`, and
     `crates/renderer/src/shader_constants.rs`'s
     `triangle_frag_keeps_glass_identity_and_ior_across_rt_lods` now *asserts*
     that `rtLOD < RT_LOD_IOR` is absent. An auditor following the checklist
     would report the guard test as the defect.
  2. Dim 17's "Interior fill (`radius < 0.0` → `isInteriorFill`) bypasses the
     cone sample" and Dim 18's "interior fill at 0.6× ambient with `radius = −1`
     (unshadowed), gating RT shadow on `!isInteriorFill`" — `isInteriorFill`
     returns zero hits repo-wide.
  3. The 2026-08-12b report's recommended correction for (2) — "`collect_lights`
     maps it to `(3.0, VisibilityMask::NONE)`, and the shader gates on
     `if (lightType > 2.5)`" — was undone by `77b540d0` two days later. The
     interior XCLL directional is now GpuLight type `2.0` with
     `VisibilityMask::FULL`, and no shader carries a `lightType > 2.5` branch.
- **Evidence**: `grep -rn "RT_LOD_IOR\|isInteriorFill" crates byroredux` → the
  only live hit is the *negative* assertion in `shader_constants.rs`.
  `grep -n "> 2.5" crates/renderer/shaders/` → no hits.
- **Impact**: Backticked symbols in a skill file assert "this exists right now"
  under the `_audit-validate.sh` convention. Three of them do not, in the two
  dimensions with the highest audit-rerun frequency. This is the mechanism that
  produced roughly one stale finding in six in past sweeps.
- **Related**: AUDIT_RENDERER_2026-08-12b P-5; #1200 (the original
  symbol-anchoring fix); the path/symbol convention in `_audit-common.md`.
- **Suggested Fix**: Rewrite the Dim-2 bullet around the live
  `glassIORAllowed` expression and name
  `triangle_frag_keeps_glass_identity_and_ior_across_rt_lods` as its guard.
  Delete the `isInteriorFill` / `radius = −1` language from Dim 17 and 18 and
  replace it with the live contract (interior XCLL emits an ordinary
  `VisibilityMask::FULL` directional; the `directional_source_contract_tests`
  module in `byroredux/src/render/lights.rs` is the pin). Run
  `.claude/commands/_audit-validate.sh` afterwards.

---

### REN-DOC-02: `_audit-common.md`'s shader-include roster lists 9 of the 12 live headers

- **Severity**: LOW
- **Dimension**: Audit-skill drift
- **Location**: `.claude/commands/_audit-common.md`, the `Shader Includes:` row
- **Status**: NEW
- **Description**: The row enumerates `bindings.glsl`, `clusters.glsl`,
  `lighting.glsl`, `material_sampling.glsl`, `math_common.glsl`, `pbr.glsl`,
  `ray_hit.glsl`, `raytrace.glsl`, `shader_constants.glsl`.
  `crates/renderer/shaders/include/` contains three more:
  `ray_origin.glsl` (self-intersection origin offsetting, added `5f970bae`),
  `shadow_common.glsl` (added `5798e467`) and `shadow_transport.glsl` (added
  `f1fa9c38`). The sibling `Shaders:` row's "21 GLSL sources" count *is* correct
  and was cross-checked.
- **Evidence**: `ls crates/renderer/shaders/include/` → 12 files. The three
  missing ones are non-trivial: `shadow_transport.glsl` owns the effect-card /
  fire-refraction shadow folding, and `ray_origin.glsl` owns the ray-origin bias
  that Dimension 2's "ray self-intersection (wrong tMin/origin bias) = HIGH"
  severity row exists to police.
- **Impact**: `_audit-common.md` is the shared layout map every audit skill
  defers to. A HIGH-floor concern (ray-origin bias) has no entry-point listing,
  so an auditor working from the layout map alone will not open the file that
  implements it.
- **Related**: REN-DOC-01; the path convention in `_audit-common.md` itself.
- **Suggested Fix**: Add the three headers to the row with one-line roles, and
  note that `include/` has no `.spv` of its own so a change to any of them
  requires recompiling every dependent shader.

---

## 5. Prioritized Fix Order

Correctness → safety → optimization.

1. **REN-D5-01** — a two-commit-old regression that silently mis-sizes the BLAS
   budget on all non-single-heap GPUs, and re-opens a documented no-recovery
   gap. Fix the heap selection and add the two-heap unit test before anything
   else here; the change is small and the current behaviour is worse than what
   it replaced on the hardware it was written for.
2. **REN-DOC-01** — costs minutes, and until it lands every future
   `/audit-renderer` run will re-derive three false positives in Dimensions 2,
   17 and 18. Cheapest correctness-per-effort item in this report.
3. **REN-D2-01** — decide the policy (fixed fallback tier vs. an explicit
   quality override) and, whichever way it goes, make the `gpu_timers == None`
   log line say that RT quality is pinned. The current log actively misdirects.
4. **REN-DOC-02** — fold into the same skill-text pass as REN-DOC-01.
5. **REN-D9-01** — mechanical; do it next time `skinned_blas_refit.rs` is open.

## 6. Needs-RenderDoc

No barrier or layout change is proposed by this report, so nothing here is
blocked on a capture. Two items are recorded as *unverified by source reading
alone*, per the standing no-speculative-Vulkan-fixes rule:

- The cluster-cull telemetry buffer's `cmd_fill_buffer` → dispatch → `HOST_READ`
  barrier pair (`compute.rs::dispatch`, new at `9c805cd7`) reads correctly but
  has never been through a `BYRO_VALIDATION` run.
- The caustic pipeline's new set-0 bindings 9/10 (global vertex/index SSBOs,
  new at `9bf7d024`) are re-pointed once per frame for the current
  frame-in-flight slot only, mirroring the pre-existing set-1 bindings 8/9
  pattern and its `DEFAULT_COUNTDOWN` deferred free. The arithmetic is exactly
  tight (one slot re-pointed per frame, `MAX_FRAMES_IN_FLIGHT` slots,
  `DEFAULT_COUNTDOWN == MAX_FRAMES_IN_FLIGHT`); a sync-validation run on a
  cell-stream geometry-pool growth would confirm it empirically.

## 7. Coverage

All 23 dimensions were examined. Dimensions with no finding and no open-issue
delta: 1 (AS), 3 (GPU-struct), 4 (sync), 6 (NIFAL material), 7 (material table),
8 (denoiser), 10 (render origin), 11 (pipeline/G-buffer), 12 (command buffer),
13 (TAA), 14 (caustics), 15 (water), 16 (volumetrics/bloom), 17 (Disney/soft
shadows), 19 (tangent space), 20 (telemetry), 21 (Cornell harness), 22 (light
animation), 23 (FSR). Existing OPEN findings in those dimensions (#2767, #2769,
#2772–#2780, #2788–#2830, #2152, #779 and others) were confirmed still live and
skipped per the dedup rule rather than re-reported.

Un-owned subsystems not covered by this audit, per `_audit-common.md`'s coverage
note: the gameplay slice (`combat.rs` / `inventory.rs` / `settings_io.rs` / the
action half of `interaction.rs`), FaceGen, and the mod runtime. `crates/fsr3-sys`
was covered only at the Dimension-23 checklist level; its FFI safety contracts
belong to `/audit-safety` Dimension 1.

---

*Report generated by `/audit-renderer` on 2026-08-16 · HEAD `85b77371`.*
