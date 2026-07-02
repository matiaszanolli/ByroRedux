# Safety Audit — 2026-07-01

**Scope**: `unsafe` blocks, memory leaks, undefined behavior, Vulkan spec
compliance across all 21 crates + `byroredux/`. Includes the 103 commits landed
since the 2026-06-23 safety audit (CHARAL character layer, the #1748/#1749/#1670/
#1671 draw/init refactors, #1713 queue-mutex scoping, #1772 follower teardown,
#1759 non-coherent-atom guard).

**Tree state**: `HEAD = 1b4e8e84`, clean working tree.

**Method**: All 10 skill dimensions run inline. Every regression-guard claim
re-verified against current code; every cited test pin run green. **New this
audit**: the Dimension-5 evidence channel was actually exercised — a debug build
(`VK_LAYER_KHRONOS_validation` + sync-validation enabled) was run against the
default FNV `GSProspectorSaloonInterior` profile for 180 frames on the RTX 4070
Ti (`target/debug/byroredux --bench-frames 180`, log at `/tmp/audit/vk_run_cube.log`).
The run included a live swapchain recreate, NPC spawn/skinning, ragdoll-capable
physics, and a clean teardown ("Vulkan context destroyed cleanly", no
live-object errors at device destroy). Findings deduped against the OPEN-issue
list (`/tmp/audit/issues.json`, 21 open) and the closed-issue set via `gh`.

---

## Summary

**2 findings** (0 critical, 1 high, 1 medium, 0 low).

The safety posture remains strong — every regression guard called out by the
skill is intact, and both findings from the 2026-06-23 audit were closed
(#1729 pex recursion cap: fixed with `MAX_REBUILD_DEPTH = 1024` + test; the
unsafe-comment residue remains open work, re-counted below). The headline
finding is new and evidence-backed, not speculative: with synchronization
validation enabled, the engine emits **READ_AFTER_WRITE hazards on
`vkCmdBuildAccelerationStructuresKHR`** during first-sight skinned-BLAS frames —
the same-command-buffer BUILD → UPDATE-refit transition is guarded only by an
`AS_WRITE → AS_WRITE` barrier, which does not cover the refit's *read* of
`srcAccelerationStructure`. This is the one validation error class the entire
180-frame run produced.

---

## Findings

### SAFE-2026-07-01-01: Skinned-BLAS first-sight BUILD → same-cmd UPDATE refit lacks AS_READ in the barrier — sync-validation READ_AFTER_WRITE hazard

- **Severity**: HIGH
- **Dimension**: 5 — Vulkan Spec Compliance (Missing AS barrier)
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:606-620`
  (`record_scratch_serialize_barrier`); hazard sequence recorded in
  `crates/renderer/src/vulkan/context/draw.rs:1740-1899`
  (`record_skinned_blas_refit`: first-sight `build_skinned_blas_batched_on_cmd`
  at `:1781` → refit loop at `:1835-1899`)
- **Status**: NEW (deduped against #642, #644, #661, #983, #1095, #1140, #1300,
  #1436 — all cover the *scratch* WAW or the build-*input* access flags, none
  cover the src-AS read)
- **Description**: `refit_skinned_blas` records an UPDATE-mode build with
  `src == dst == entry.accel` (`blas_skinned.rs:540-549`). Per the Vulkan spec,
  an UPDATE-mode build **reads** `srcAccelerationStructure` with
  `VK_ACCESS_2_ACCELERATION_STRUCTURE_READ_BIT_KHR` at the
  `ACCELERATION_STRUCTURE_BUILD` stage. On an entity's first-sight frame the
  same command buffer first records the fresh BLAS BUILD (which *writes* the
  AS), then falls through to the refit loop (first-sight entities are in
  `pose_dirty`, so the #1196 skip gate does not fire). The only barrier between
  the two builds is the self-emitted scratch-serialize barrier — src
  `AS_WRITE` → dst `AS_WRITE` only. The BUILD's write to the AS backing buffer
  is therefore never made visible to the refit's `AS_READ` access → RAW hazard.
  The cross-frame steady-state case is fine: the closing
  `AS_WRITE → AS_READ` barrier after the refit loop (`draw.rs:1902-1909`)
  covers frame N+1's refit reading frame N's write. The extensive barrier
  comment at `draw.rs:1801-1827` reasons only about the shared *scratch*
  buffer; the src-AS read went unconsidered.
- **Evidence**: Validation output, verbatim (10 occurrences — one per
  first-sight skinned NPC — then the layer's `duplicate_message_limit` = 10
  suppressed the VUID; log timestamps coincide exactly with the NPC-spawn /
  cell-transition frame):

  ```
  [2026-07-01T21:57:29Z ERROR byroredux_renderer::vulkan::debug] [Vulkan]
  vkCmdBuildAccelerationStructuresKHR(): READ_AFTER_WRITE hazard detected.
  vkCmdBuildAccelerationStructuresKHR reads VkBuffer 0x2a450000002a45, which was
  previously written by another vkCmdBuildAccelerationStructuresKHR command. The
  buffer backs pInfos[0].srcAccelerationStructure (VkAccelerationStructureKHR
  0x2a460000002a46).
      The current synchronization allows VK_ACCESS_2_ACCELERATION_STRUCTURE_WRITE_BIT_KHR
  accesses at VK_PIPELINE_STAGE_2_ACCELERATION_STRUCTURE_BUILD_BIT_KHR, but to
  prevent this hazard, it must allow VK_ACCESS_2_ACCELERATION_STRUCTURE_READ_BIT_KHR
  accesses at VK_PIPELINE_STAGE_2_ACCELERATION_STRUCTURE_BUILD_BIT_KHR.
      Buffer access region: { offset = 0  size = 88192 }
  ```

  The barrier the layer is describing (`blas_skinned.rs:610-618`):

  ```rust
  memory_barrier(
      device, cmd,
      vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
      vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
      vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
      vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,   // ← no AS_READ
  );
  ```

  Aggravating doc rot: the `refit_skinned_blas` safety docstring
  (`blas_skinned.rs:381-383`) claims "The barrier is idempotent
  (`MEMORY_READ | MEMORY_WRITE → MEMORY_READ | MEMORY_WRITE`)" — i.e. it
  *documents* the READ coverage that the implementation does not have (the mask
  has been `AS_WRITE → AS_WRITE` since the module split, per `git log -L`).
- **Impact**: On first-sight frames the refit may consume a partially-written
  source BVH — on real hardware this is driver-dependent BVH corruption
  (garbage/missing skinned geometry in every RT effect: shadows, GI,
  reflections) and formally UB per the spec's synchronization requirements. It
  fires for **every** newly-spawned skinned NPC (cell transitions, streaming),
  i.e. under fully realistic conditions — the severity table's "Vulkan
  validation layer errors in normal operation" / "Missing AS barrier" HIGH
  rows. Not CRITICAL: geometry, addresses, and counts are all correct
  (#907/#1145 guards verified intact); the defect is visibility ordering.
- **Related**: #983 / #1140 (scratch-serialize invariant — the pinned predicate
  `requires_scratch_serialize_barrier_before` codifies scratch WAW only),
  #911 (moved first-sight builds onto the per-frame cmd, creating the same-cmd
  adjacency), #1436 (build-input access flags), #1139 (older docstring drift in
  the same function).
- **Suggested Fix**: Widen the dst access mask in
  `record_scratch_serialize_barrier` to
  `ACCELERATION_STRUCTURE_WRITE_KHR | ACCELERATION_STRUCTURE_READ_KHR`
  (one line; src mask stays `AS_WRITE`). Correct the `refit_skinned_blas`
  docstring to match. Re-run the validation-layer scenario (FNV saloon,
  180 frames) and confirm zero hazards; extend the #1140 predicate test to pin
  the READ bit so a refactor can't narrow it again.

### SAFE-2026-07-01-02: Residual ~218 renderer `unsafe {` blocks lack a SAFETY comment

- **Severity**: MEDIUM
- **Dimension**: 4 — Unsafe-Block Discipline
- **Location**: `crates/renderer/src/` (worst: `vulkan/composite.rs` 17,
  `vulkan/context/mod.rs` 16, `vulkan/context/helpers.rs` 16,
  `vulkan/texture.rs` 15, `vulkan/device.rs` 14, `vulkan/svgf.rs` 13,
  `vulkan/context/resize.rs` 13, `texture_registry.rs` 10,
  `vulkan/skin_compute.rs` 9, `vulkan/taa.rs` 9, `vulkan/compute.rs` 9)
- **Status**: Existing: #1644 (CLOSED — partial fix) — carry-over of
  SAFE-2026-06-23-01; the residue was never published as its own issue
- **Description**: Re-count of the current tree with the same methodology as
  the 2026-06-23 audit (non-test files, `unsafe {` block-openers with no
  `SAFETY` within the preceding 6 lines): **218** of 544 non-test unsafe-block
  openers lack a comment — statistically unchanged from the prior count of 219
  (the #1751/#1752 descriptor/compute-pipeline consolidations retired roughly
  as many uncommented sites as the CHARAL-era churn touched). A portion are
  batched-FFI false positives (one per-function SAFETY comment covering several
  consecutive ash calls), but the zero-comment files confirmed in the prior
  audit are unchanged: `texture_registry.rs` (10/0) and
  `vulkan/context/screenshot.rs` (5/0).
- **Evidence**: Recount script over `crates/` + `byroredux/src` (6-line window,
  `#[cfg(test)]`-module and `*_tests.rs` exclusion): 544 total openers, 218
  without SAFETY. Example uncommented sites: `texture_registry.rs:297`
  (`create_descriptor_set_layout`), `:313` (`create_descriptor_pool`), `:324`
  (`allocate_descriptor_sets`); `vulkan/context/screenshot.rs:224`
  (`create_buffer`), `:248`, `:272` (`destroy_buffer`).
- **Impact**: Defense-in-depth / maintainability gap, not live UB — spot-checks
  confirm the calls are sound today (live device, valid handles). Each
  undocumented invariant is one refactor away from being silently violated.
- **Related**: #1644 (closed, fixed 124 of ~327), #1432, #579 (both closed,
  earlier partial sweeps)
- **Suggested Fix**: Finish the #1644 sweep, prioritising the zero-comment
  files and creation/teardown paths; batch one SAFETY note per FFI cluster.

---

## Verified-Intact Regression Guards (PASS — not findings)

Every item re-checked against current code at `1b4e8e84`; cited tests run green.

**Dimension 1 — FFI lifetime (cxx)**: `crates/cxx-bridge/src/lib.rs` still
exposes only `native_hello() -> String` inside `unsafe extern "C++"`. No
`*const`, `&[u8]`, `Box<…>`, or Rust-reference-taking C++ fn. Dimension dormant.

**Dimension 2 — Memory corruption / UB**:
- ECS cached-pointer contract (`crates/core/src/ecs/query.rs`): `QueryRead` /
  `QueryWrite` / `ComponentRef` cache the downcast pointer once in `new()`,
  hold the `RwLock*Guard` as a struct field for the wrapper's lifetime, and
  gate `&mut *self.storage` behind `&mut self`. All four deref sites carry
  accurate SAFETY comments (#1367). The #35 unsound pattern stays excised.
- pex `OpCode::from_u8` (`crates/pex/src/opcode.rs:130-136`): transmute still
  guarded by `byte >= MAX_OPCODE` (= 51); enum has `Nop = 0` and 50 implicit
  successors — contiguous, no gaps. `from_u8_round_trips_and_rejects_oob` pins it.
- **#1729 fix verified in place** (was SAFE-2026-06-23-02): `Reconstructor::rebuild`
  (`crates/pex/src/decompile/control_flow.rs`) now threads `depth`, bails past
  `MAX_REBUILD_DEPTH = 1024` with `DecompileError::RecursionLimit`, and is
  pinned by `rebuild_rejects_excessive_recursion_depth`.
- sfmaterial `BuiltinType::from_u32` (`crates/sfmaterial/src/types.rs:37-55`):
  checked `match` with `_ => Err(UnsupportedBuiltin { raw })` — no transmute.
- NIF bulk POD reads (`crates/nif/src/stream.rs:350-379`, mirror
  `header.rs:360-382`): `count.checked_mul(size_of::<T>())` overflow guard +
  sealed local `unsafe trait AnyBitPattern` both present. The three
  `bs_geometry.rs` impls (`BoneWeight`/`Meshlet`/`CullData`) are `#[repr(C)]`
  all-bit-patterns-valid scalars with the MEM-05/#1439 SAFETY comment.
- Recursion caps intact: NIF walk `MAX_NIF_NODE_DEPTH = 128` (#1269), Papyrus
  expr `MAX_EXPR_DEPTH = 256` (#1270), pex `MAX_REBUILD_DEPTH = 1024` (#1729).
- `crates/save/` and `crates/scripting/` — zero `unsafe` (re-confirmed). The
  new CHARAL code (`crates/core/src/character/`) adds **no** unsafe; core's 6
  unsafe tokens are unchanged (4 in `ecs/query.rs`, 2 in `string/mod.rs`, all
  commented and sound — the `string` pair re-read this audit).
- `crates/facegen` / `crates/plugin` "unsafe" grep hits are comment prose only
  (facegen's lib.rs literally documents "No `unsafe`"); no unsafe code.

**Dimension 3 — Leaks / drop ordering**:
- AllocatorResource-before-device (#1406/#1477): `impl Drop for App`
  (`byroredux/src/main.rs:440-466`) still runs
  `remove_resource::<AllocatorResource>()` **then** `renderer.take()` on every
  teardown path — survived the #1670 `App::new` split intact (REG-08 comment
  in place).
- Rapier release on cell unload (#1520/#1531): `release_victim_rapier_bodies`
  (`byroredux/src/cell_loader/unload.rs:380`, wired at `:187`) cascades
  bodies/colliders/joints + ragdolls. All 7 `rapier_release_tests` green,
  including `release_removes_ragdoll_bodies_colliders_and_joints` and
  `release_sweeps_both_ragdoll_and_rapier_handles`.
- **#1772 (new since baseline)**: keyframed bone-follower bodies torn down on
  ragdoll activation (`byroredux/src/ragdoll.rs:238`), pinned by
  `activation_tears_down_keyframed_bone_bodies`.
- Deferred-destroy drain (#418/#732): `tick_deferred_destroy` runs AFTER
  `wait_for_fences` (`context/draw.rs:2166-2190`) — survived the #1748
  `draw_frame` extraction; shutdown `drain` with `device_wait_idle` first at
  `context/mod.rs:2490-2529`.
- Live-run leak evidence: the 180-frame validation run tore down cleanly with
  zero live-object / undestroyed-handle validation errors at device destroy.
- CPU-side growth: `MaterialTable::intern` caps at `MAX_MATERIALS` (map cannot
  grow past the cap; over-cap returns id 0 + one-shot warn);
  `AnimationClipRegistry` interns ASCII-lowercased keys (#790).

**Dimension 5 — Vulkan spec (beyond the finding)**:
- The sync-validation run's ONLY error class is SAFE-2026-07-01-01 — the
  geometry pass, TLAS build/refit, SVGF/TAA/bloom/SSAO compute chain, the
  #1671-split swapchain recreate (exercised live mid-run), and the egui pass
  all ran hazard-free in the exercised FNV interior scene. (Water/caustic
  paths were not exercised — no water in the saloon — and stay
  "needs validation-run on a water cell" rather than asserted-clean.)
- TLAS resize `device_wait_idle` before freeing old allocation (#1390):
  `acceleration/tlas.rs:322` present.
- TLAS UPDATE count guard (#1083): update path clamps to the BUILD-recorded
  count (`tlas.rs:530`); skinned refit count/flag guards (#907/#1145) verified
  present in `refit_skinned_blas` with drop-and-rebuild fallback.
- Volumetrics dispatch gate: `VOLUMETRIC_OUTPUT_CONSUMED` (`volumetrics.rs:143`)
  honored at both `draw.rs` call sites (`:569`, `:3335`) — survived the #1748
  `record_post_passes` extraction.
- `VK_KHR_ray_query` gating: `ray_query_supported` requires all
  `RT_EXTENSIONS` (`device.rs:270`); RT paths feature-gated on it.
- **#1759 (new since baseline)**: `NON_COHERENT_ATOM_SIZE = 256` assumption now
  asserted against the physical-device limit at device create (`device.rs:573-580`).
- **#1713 (new since baseline)**: graphics-queue Mutex released before the
  one-time fence wait (`texture.rs`); fence guard still spans the wait.
- SPIR-V reflection pins: all 5 `scene_descriptor_reflection_tests` green
  (triangle + water, RT-on and RT-off).

**Dimension 6 — Material table layout**: `GpuMaterial` pinned at **300 B**
(`gpu_material_size_is_300_bytes` green), per-field offsets pinned
(`gpu_material_field_offsets_match_shader_contract` green), GLSL field-name +
field-order pins green (6 material-layout tests total). No `[f32; 3]` fields in
any `#[repr(C)]` GPU struct (the `material.rs` hits are constructor *parameters*).
Intern cap (#797) and `upload_materials` `debug_assert` + `.min(MAX_MATERIALS)`
clamp in lockstep (`upload.rs:534-545`).

**Dimension 7 — RT IOR/glass**: `GLASS_RAY_BUDGET = 1048576` enforced at the
IOR gate (`triangle.frag:1192-1193`, with the #1438 overshoot nuance documented
in-shader, not re-reported); passthru loop bounded by
`REFRACT_PASSTHRU_BUDGET = 2` with the #789 same-texture identity check
(`triangle.frag:1377-1385`); Frisvad basis active (`triangle.frag:1271`);
`DBG_VIZ_GLASS_PASSTHRU = 0x80` uncollided (full `DBG_*` catalog re-swept —
values 0x1…0x10000 all distinct).

**Dimension 8 — NPC/animation spawn**: B-spline `FLT_MAX` pose-fallback
sentinel wired across `anim/bspline.rs` + `anim/transform.rs` (#772);
`AnimationClipRegistry` case-insensitive interning preserved;
`MAX_TOTAL_BONES` overflow guard — both `bone_palette_overflow_tests` green.

**Dimension 9 — NIFAL NaN boundary**: `material_translate.rs:157-160` seeds
`f32::NAN` sentinels then immediately calls `resolve_pbr()`;
`Material::resolve_pbr` (`components/material.rs:638-656`) detects NaN, fills
from the classifier, and unconditionally clamps (`[0,1]` / `[0.04,1]`).
`Material::default()` is finite (0.0/0.5); the Cornell harness constructors are
finite-literal + default. Collision translate finite guards (#1534/#1409)
present, including the new `radius_variation.is_finite()` from #1775; emitter
rate guard `(r.is_finite() && 0.0 < r && r < 3.0e38)` also encodes the #1771
zero-rate preset-fallback. Particle spawn is capped at `max_particles`
(hard-coded presets 96–256, never NIF-sourced), pinned by
`cap_at_max_particles_drops_extra_spawns`.

**Dimension 10 — debug-ui teardown**: The skill's premise is stale in a *safe*
direction — `DebugUiState` (`crates/debug-ui/src/lib.rs:50`) no longer holds
any Vulkan resource (egui context + winit state + panel state only). The
Vulkan side lives in `crates/renderer/src/vulkan/egui_pass.rs`, owned by
`VulkanContext`, and is destroyed at the very top of `VulkanContext::drop`
(`context/mod.rs:2842-2844`) — after `device_wait_idle`, before every other
teardown, with the device destroyed last. Allocator-before-device rule holds
structurally.

---

## Coverage Note

All 21 `crates/` + `byroredux/` swept for `unsafe` (Dimension-4 script covers
the whole tree). Non-test unsafe-block openers: 544 total (renderer carries
~95%); token counts: renderer 629, nif 11 (all POD-read infra + sealed-trait
impls, commented), core 6 (ECS cached pointers + string pool, commented),
byroredux 2, pex 1 (guarded transmute); save/scripting/audio/bsa/bgsm/papyrus/
physics/spt/sfmaterial/platform/ui/debug-* 0. facegen/plugin token hits are
comment prose, not code. The live validation-layer run is logged at
`/tmp/audit/vk_run_cube.log` (not checked in).

Next step: `/audit-publish docs/audits/AUDIT_SAFETY_2026-07-01.md`
