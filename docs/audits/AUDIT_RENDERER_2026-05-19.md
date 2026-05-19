# Renderer Audit — 2026-05-19

**Scope**: Full 20-dimension audit of the Vulkan renderer pipeline.
**Trigger**: Delta against the [2026-05-16 baseline](AUDIT_RENDERER_2026-05-16.md) and the [2026-05-18 DIM11/DIM14 focus](AUDIT_RENDERER_2026-05-18_DIM11_DIM14.md). ~30 renderer-touching commits since baseline, including the #952 fence-reset reorder (closed today), the #869 NiWireframeProperty/`flat_shading` consumer pair, the #1147 Phase 2a BGSM-flag plumbing, the #1115 `render.rs` → `render/` submodule split, and a cluster of doc/test pins (#1144/#1145/#1129/#1130/#1131/#1155/#1158/#1166).
**Method note**: The first attempt at this audit dispatched 3 renderer-specialist agents (Dims 1–3) in parallel; all three exhausted their tool budget on deep code reads before writing reports. Switched to in-thread delta auditing scoped by `git log` since the 2026-05-16 baseline. This is documented for the post-mortem on agent-orchestrated audits — large checklists with `depth=deep` are not tractable in one agent invocation.
**Prior base**: 2026-05-16 closed at 14 findings (2 MEDIUM, 11 LOW, 1 INFO) with 4 carry-over opens. 12 of the 14 landed as fixes in the 2026-05-17 → 2026-05-19 batch closes; the 4 carryovers split 2-and-2 (closed-today / still-open).

**Open issues checked**: 12 REN-D / FO4-D / PERF-D issues open, 95+ REN-D issues closed in the last 14 days.

---

## Executive Summary

**1 new finding** across 20 dimensions. No CRITICAL or HIGH issues.

| Severity | Count |
|----------|-------|
| CRITICAL | 0     |
| HIGH     | 0     |
| MEDIUM   | 1     |
| LOW      | 0     |
| INFO     | 0     |

**Carry-over status**:

| Issue | Title | 2026-05-16 status | Today |
|-------|-------|-------------------|-------|
| #952  | REN-D1-NEW-04 — `reset_fences` before fallible recording | OPEN (carry-over) | **CLOSED** (0f9dc8eb, today 00:45 UTC) |
| #924  | REN-D15-NEW-02 — Composite distance-fog gated OFF post-M55 | OPEN (carry-over) | **CLOSED** (today 00:38 UTC) |
| #1092 | REN-D11-001 — SSAO + cluster reconstruct from jittered `inv_view_proj` | OPEN (carry-over) | OPEN (no code delta) |
| #1104 | REN-D16-002 — Path-2 screen-space derivative bitangent ignores UV-mirror handedness | OPEN (carry-over) | OPEN (no code delta) |

**Highest-priority new finding** (by blast radius):
1. **REN-D1-NEW-05 (MEDIUM)** — `recreate_in_flight_for_frame` (the #952 submit-failure recovery helper, landed 0f9dc8eb today) destroys + replaces `in_flight[frame]` but leaves any `images_in_flight[i] == old_fence` entries pointing at the destroyed handle. On the next acquire that returns the same swapchain image index, `wait_for_fences` is called with a dangling `VkFence`. Sibling-bug to the very issue #952 was meant to close, on the same fence slot. See finding below.

**RT pipeline assessment**: BLAS / TLAS flag composition is now triple-pinned (#1144 + #1145 + the 2026-05-16 `built_primitive_count` invariant test). The skinned-BLAS flag split (`SKINNED_BLAS_FLAGS = PREFER_FAST_BUILD | ALLOW_UPDATE`) and static (`STATIC_BLAS_FLAGS = PREFER_FAST_TRACE`) constants are correctly applied across all four skinned-BLAS call sites and the static path. VUID-03667 (build/update flag match) and VUID-03708 (build/update primitiveCount match) hold.

**Rasterization assessment**: The #869 wireframe variant (`PolygonMode::LINE`) is correctly gated on `device_caps.fill_mode_non_solid_supported` at both the cache-key normalization site ([draw.rs:1403](../../crates/renderer/src/vulkan/context/draw.rs#L1403)) and the read site ([draw.rs:1784](../../crates/renderer/src/vulkan/context/draw.rs#L1784)). The `INSTANCE_FLAG_FLAT_SHADING = 1 << 7` bit is unique, outside both packed windows (render-layer bits 4-5, terrain-tile bits 16-31), pinned at 128 against the shader-side `(inst.flags & 128u)` literal, and `fragNormalEffective` flows correctly through all six `fragNormal` consumer sites in [triangle.frag](../../crates/renderer/shaders/triangle.frag) (POM, lighting, RT, G-buffer normal write).

**Material table (R1)**: Phase 2a (#1147) added bits `BGSM_PBR = 1<<5`, `BGSM_TRANSLUCENCY = 1<<6`, `BGSM_MODEL_SPACE_NORMALS = 1<<7` to the host-side `material_flag::*`. The bits are populated and uploaded but shader-side branching is gated for Phase 2b (#1147 still OPEN, by design). No collision with existing `MAT_FLAG_*` literals in the shader (bits 0–4).

**Refactor risk (#1115)**: The `render.rs` → `render/{sky, lights, water, particles, camera, skinned, static_meshes}.rs` split is mechanical — no behavior change, no GPU contract drift. Tests track the split (`bone_palette_overflow_tests`, `frustum_tests`, `variant_pack_gating_tests`, `draw_sort_key_tests`).

---

## Findings

### MEDIUM

---

### REN-D1-NEW-05: `recreate_in_flight_for_frame` leaves stale fence handles in `images_in_flight`

- **Severity**: MEDIUM
- **Dimension**: Vulkan Sync
- **Location**: [crates/renderer/src/vulkan/sync.rs:263-280](../../crates/renderer/src/vulkan/sync.rs#L263-L280), [crates/renderer/src/vulkan/context/draw.rs:231](../../crates/renderer/src/vulkan/context/draw.rs#L231), [crates/renderer/src/vulkan/context/draw.rs:2486-2489](../../crates/renderer/src/vulkan/context/draw.rs#L2486-L2489)
- **Status**: NEW
- **Description**: The submit-failure recovery helper landed today (0f9dc8eb, fix for #952) destroys `in_flight[frame]` and replaces it with a fresh SIGNALED fence. The replacement closes the deadlock window for the next frame's `wait_for_fences(&[in_flight[frame]], ...)`. However, `images_in_flight[img]` was set to the **old** fence handle at [draw.rs:231](../../crates/renderer/src/vulkan/context/draw.rs#L231) (`self.frame_sync.images_in_flight[img] = self.frame_sync.in_flight[frame];`) **before** the submit failed. After `recreate_in_flight_for_frame` destroys the old fence at [sync.rs:273](../../crates/renderer/src/vulkan/sync.rs#L273), `images_in_flight[img]` is left pointing at a destroyed `VkFence`. If the next acquire on this frame slot returns the same swapchain image index (`img' == img`), the swapchain-image-still-in-use guard at [draw.rs:216-217](../../crates/renderer/src/vulkan/context/draw.rs#L216-L217) reads the stale handle and calls `wait_for_fences(&[image_fence], true, u64::MAX)` with a dangling handle.

  Compare to the resize-path sibling (`recreate_for_swapchain` at [sync.rs:177-198](../../crates/renderer/src/vulkan/sync.rs#L177-L198)), which destroys + recreates every `in_flight` fence AND explicitly zeroes `images_in_flight` at [sync.rs:182](../../crates/renderer/src/vulkan/sync.rs#L182). The new partial-recreation helper at [sync.rs:263](../../crates/renderer/src/vulkan/sync.rs#L263) implements only the first half.

- **Evidence**:

  ```rust
  // draw.rs:215-231 (acquire path)
  let image_fence = self.frame_sync.images_in_flight[img];
  if image_fence != vk::Fence::null() && image_fence != self.frame_sync.in_flight[frame] {
      unsafe {
          if let Err(e) = self.device.wait_for_fences(&[image_fence], true, u64::MAX) // <- stale on recovery path
              ...
      }
  }
  self.frame_sync.images_in_flight[img] = self.frame_sync.in_flight[frame]; // <- captures OLD fence

  // draw.rs:2470-2489 (submit-failure recovery)
  // submit fails → recreate_image_available_for_frame (OK)
  //              → recreate_in_flight_for_frame: destroys old, makes new SIGNALED fence
  //              → images_in_flight[img] STILL points at destroyed fence
  ```

- **Trigger conditions**: `queue_submit` fails (device-lost, host OOM, validation-driven abort, etc.) AND the next acquire on the same `frame` slot returns the same `img` index. Probability of matching `img` is non-trivial — FIFO swapchains with N>2 images can cycle predictably under steady load.

- **Impact**: The check at [draw.rs:217](../../crates/renderer/src/vulkan/context/draw.rs#L217) only filters `null` and `== in_flight[frame]`. The stale handle is neither, so `wait_for_fences` is called on a destroyed `VkFence`. Per Vulkan spec this is undefined behavior. In practice: (a) validation layers flag VUID-vkWaitForFences-pFences-parameter; (b) without validation, the driver may have already recycled the handle for a newly-created fence elsewhere — `wait_for_fences` then targets an unrelated fence, returning either spuriously immediately (if signaled) or hanging u64::MAX (if unsignaled). Sibling-bug to the very deadlock #952 was meant to close, on the same fence slot.

- **Related**: #952 (REN-D1-NEW-04, the parent fix, closed 2026-05-19 00:45 UTC), #908 (sibling resize-path issue, closed earlier — uses the correct full-table-clear pattern), #910 (REN-D5-NEW-01, the `image_available` sibling helper).

- **Suggested Fix**: In `recreate_in_flight_for_frame` ([sync.rs:263](../../crates/renderer/src/vulkan/sync.rs#L263)), after `std::mem::replace` returns the old handle but before `destroy_fence`, walk `images_in_flight` and null out any slot equal to the old handle:

  ```rust
  let old = std::mem::replace(&mut self.in_flight[frame], new_fence);
  for slot in &mut self.images_in_flight {
      if *slot == old {
          *slot = vk::Fence::null();
      }
  }
  device.destroy_fence(old, None);
  ```

  Same idiom as `recreate_for_swapchain`'s line 182 wipe, scaled to the single-frame case. Add a unit test that calls `recreate_in_flight_for_frame` after seeding `images_in_flight[2] = old_fence` and asserts the slot is null post-call.

---

## No-Delta Dimensions (Verified)

The following dimensions had **zero touching commits** since the 2026-05-16 baseline (or only doc/test pins that do not change runtime behavior) and the prior audit's "no new findings" verdicts stand:

- **Dim 4 — Render Pass / G-Buffer**: No code change. Carryover open #1128 (REN-D4-NEW-01: GBuffer `Drop` safety-net fires during panic-unwind from `new()`) unchanged — not regressed.
- **Dim 5 — Command Recording**: No code change.
- **Dim 7 — Resource Lifecycle**: #1174 (`ScreenshotBridge` mutex-poison recovery) and #1163 / #1165 (allocator-lock hoisting) are local safety improvements, no lifecycle drift.
- **Dim 11 — TAA**: #1124 added a shared-counter rationale docstring; no behavior change. **Carryover #1092** (jittered `inv_view_proj` consumed by SSAO + cluster cull) **still open** — confirmed by SSAO entry point read; the un-jittered projection is still not threaded through.
- **Dim 13 — Caustics**: No code change.
- **Dim 15 — Sky / Weather / Exterior Lighting**: Carryover #924 closed today (fog re-enabled / volumetric-replacement-gate flipped). Verify next bench cycle.
- **Dim 16 — Tangent-Space & Normal Maps**: **Carryover #1104** (Path-2 derivative bitangent doesn't honor UV-mirror) **still open** — no code touch.
- **Dim 18 — Volumetrics**: #1130 doc-only. The 2026-05-16 "volumetric output consumed" gate (#928) still holds; carryover #924 closure should re-engage the composite-side sample.
- **Dim 19 — Bloom**: No code change.
- **Dim 20 — M-LIGHT Soft Shadows**: No code change.

## Touched-But-Clean Dimensions

- **Dim 1 — Vulkan Sync**: One new finding above (REN-D1-NEW-05). #952 itself is fixed correctly; the residue is in the recovery helper.
- **Dim 2 — GPU Memory**: #1142 amortizes the `build_tlas` missing-samples scratch `Vec` (perf, not correctness). #1127 (skinned BLAS scratch doesn't shrink in `build_skinned_blas_batched_on_cmd`) **still open**, not regressed.
- **Dim 3 — Pipeline State**: #869 part 1 (NiWireframeProperty → LINE variant) wired correctly. `device_caps.fill_mode_non_solid_supported` gate applied symmetrically at cache-key compute and read sites. Minor: when the device lacks `fillModeNonSolid`, content with `wireframe=true` still produces a separate batch-boundary at the sort-key level (the `PipelineKey::*` carries the un-gated bit), which then collapses to the same pipeline at bind time — extra pipeline-bind cost, no correctness issue. Not worth filing.
- **Dim 6 — Shader Correctness**: #869 part 2 wiring fully audited above. #1147 Phase 2a bits OR'd into `GpuMaterial.material_flags` are upload-only, no shader read yet (Phase 2b pending). #1152 (compute shaders use `WORKGROUP_X/Y` symbolic constants instead of hardcoded 8) verified — `caustic_splat.comp`, `ssao.comp`, `svgf_temporal.comp`, `taa.comp` all switched. #1135 (`MAX_BONES_PER_MESH` 128 → 144) propagated to `shader_constants.glsl`, `triangle.vert`, `skin_vertices.comp`, and the bone-palette overflow tests.
- **Dim 8 — Acceleration Structures**: #1144 + #1145 + the existing `built_primitive_count` invariant test triple-pin the flag composition. #1141 deletes dead `build_skinned_blas` sync path. No drift.
- **Dim 9 — RT Ray Queries**: No code change since 2026-05-17 audit. Carryover #1125 (REN-D9-NEW-01: `traceReflection` miss-fallback blends `skyTint` into sealed interior cells) **still open**.
- **Dim 10 — Denoiser & Composite**: #1131 doc-only. #645d3b90 dropped `BLOOM_INTENSITY` / `VOLUME_FAR` shader-side redeclarations in favor of the include. Open: #1159 (REN-D10-NEW-12: SVGF nearest-tap doesn't mask `ALPHA_BLEND_NO_HISTORY`), #1160 (REN-D10-NEW-13: composite outgoing render-pass dep uses deprecated `BOTTOM_OF_PIPE`).
- **Dim 12 — GPU Skinning**: #1135 bump verified; `MAX_TOTAL_BONES = 32768` / `MAX_BONES_PER_MESH = 144` → max ~227 skinned meshes. Overflow test covers the boundary. #1133 / #1134 scratch-cluster + instance-buffer dirty-gate landed.
- **Dim 14 — Material Table**: 2026-05-18 focused audit had no critical findings; #1147 Phase 2a adds three host-side bits with no upload-size delta (still within the 260-byte `GpuMaterial` pin). Open: #1147 itself (Phase 2b shader consumer), #1148 (FO4 BGSM template-cycle), #972 (TextureSet `HasModelSpaceNormals`), #973 (MSWP material-swap per-shape).
- **Dim 17 — Water**: 2026-05-14 focused audit and 2026-05-16 base both clean. Today: #1187 corrected the `water.vert` `GpuInstance` path comment post-Session-34 (doc only). #1129 added a forward-compat dynamic-state coverage test. No runtime change.

---

## Prioritized Fix Order

1. **REN-D1-NEW-05** — `images_in_flight` invalidation in `recreate_in_flight_for_frame`. Self-contained fix (~10 LOC + test). Blocks a real but probabilistic post-submit-failure UB / hang path.

The 10 still-open prior-audit findings (#1092, #1104, #1125, #1127, #1128, #1132, #1147, #1148, #1159, #1160, #972, #973) carry their original prioritization; no regression detected.

---

## Notes for Next Audit

- The 2026-05-18 focused audit covered Dims 11 + 14 specifically. Carry that scope forward — if Phase 2b of #1147 lands before next sweep, re-focus Dim 14 to verify the new shader branches don't collide with the existing `MAT_FLAG_EFFECT_*` bits.
- The fence-recovery helper pattern is now duplicated across `recreate_image_available_for_frame`, `recreate_in_flight_for_frame`, and (the whole-table) `recreate_for_swapchain`. After REN-D1-NEW-05's fix lands, consider a follow-up refactor to a single helper that nulls cross-references symmetrically — same pattern hazard could recur on any future per-slot recreate.
- The `INSTANCE_FLAG_FLAT_SHADING` docstring at [scene_buffer/constants.rs:163-165](../../crates/renderer/src/vulkan/scene_buffer/constants.rs#L163-L165) describes the bit as sitting *"between PRESKINNED (bit 6) and the render-layer slot (bits 4..5)"*. Bit 7 is *above* bit 6, not between bits 4-5 and 6. Pure doc nit; not filed.
