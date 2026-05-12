# Renderer Audit — 2026-05-11 (Dimension 10 focus)

**Scope**: Dimension 10 — Denoiser & Composite Pipeline (SVGF temporal accumulation, composite reassembly, ACES tonemap, fog mix, caustic/bloom/volumetric integration).
**Depth**: deep.
**Method**: orchestrator + single dimension agent.

## Executive Summary

- **Findings**: 0 CRITICAL, 0 HIGH, 0 MEDIUM, 4 LOW.
- **Pipeline areas affected**: SVGF barrier hygiene + composite-render-pass external dep + per-FIF history-reset counter. All defence-in-depth / forward-looking; no live correctness hazards.
- **Net verdict**: **CLEAN.** SVGF temporal accumulation faithful to Schied 2017 §4 (2×2 bilinear consistency loop, mesh-id rejection mask #904, normal-cone #650, NaN/Inf #903). Composite reassembly formula honours the #784 display-space-fog invariant. Post-#865 fog curve branch is well-formed. `CompositeParams` 272 B layout pin survived #865 untouched.

## RT Denoiser & Composite Assessment (positive checks)

- **SVGF ping-pong** (`svgf.rs`): slot indexing `prev = (frame + 1) % MAX_FRAMES_IN_FLIGHT` correct, guarded by `const_assert >= 2` (#918).
- **SVGF reprojection** (`svgf_temporal.comp:16-19, 99-102`): `prevUV = uv - motion`, sign convention matches `triangle.frag` motion-vector output. Format R16G16_SFLOAT consumed via `texelFetch` (no filter/format concern).
- **SVGF mesh-id rejection** (`svgf_temporal.comp:93, 142`): 15-bit mask correctly applied post-#904 — early-out on `currID == 0 || (currID & 0x8000u) != 0u` (sky / alpha-blend), bilinear-tap reject masks both sides with `& 0x7FFFu`.
- **SVGF α floor / recovery**: Schied 2017 §4 floor = 0.2, recovery = 0.5 over N frames; 4 unit tests pin the state machine.
- **SVGF dispatch ceiling division** (`svgf.rs:887-889`): `gx = (width + 7) / 8`, `gy = (height + 7) / 8` matches `local_size_x = 8, local_size_y = 8`. In-shader bounds check at L69 covers partial tiles.
- **Composite reassembly** (`composite.frag`): `direct + indirect * albedo + caustic + bloom * BLOOM_INTENSITY`, ACES applied pre-fog, fog mix happens in display space (post-ACES) per #784.
- **Fog mix post-#865** (`composite.frag:441-480`): curve gate requires BOTH `fog_clip > 0 && fog_power > 0`; falls back to linear when either is 0; both branches `clamp(fog_t, 0.0, 1.0)`; `worldDist` recomputed independently of volumetric branch ordering; exterior-gated (`depth_params.x > 0.5 && depth < 0.9999`).
- **CompositeParams 272 B pin**: #865 packed `fog_clip`/`fog_power` into existing `fog_params.z/w` slots without growing the UBO; offsets 0..208 + 64-byte mat4 unchanged; `composite_params_is_16_byte_aligned_std140_shape` test still passes.
- **Caustic accumulator** (`composite.frag:70, 323-327`): `usampler2D` `texelFetch` → divide by `CAUSTIC_FIXED_SCALE = 65536.0` (matched to `caustic.rs` constant via #667 include_str grep test) → modulated by albedo → added to **direct** lighting only (never to indirect, which would double-modulate by albedo).
- **TAA wire-up**: composite binding 0 swapped to TAA output at GENERAL via `rebind_hdr_views`; `fall_back_to_raw_hdr` (#479) covers TAA failure path. Disable path skips the TAA dispatch entirely.
- **Volumetric froxel `* 0.0` gate**: aligns with host-side `VOLUMETRIC_OUTPUT_CONSUMED` constant (#928 closed). Per-fragment 3D-texture sample cost is negligible.
- **Resize lockstep** (`resize.rs:273-490`): gbuffer → svgf → caustic → bloom → composite → taa → composite.rebind_hdr_views. All per-FIF history images rebuilt in lockstep.
- **SSAO note** (checklist item 10): not consumed by composite — sampled in `triangle.frag` (`aoTexture` set=1 binding=7) and baked into geometry pass outputs before composite reassembles. Checklist item doesn't apply to this dimension.

## Findings

### [LOW] SVGF UBO host barrier is per-dispatch, not folded into bulk barrier
**Dimension**: Denoiser & Composite
**Location**: `crates/renderer/src/vulkan/svgf.rs:828-841`
**Severity**: LOW
**Observation**:
```rust
let ubo_barrier = vk::MemoryBarrier::default()
    .src_access_mask(vk::AccessFlags::HOST_WRITE)
    .dst_access_mask(vk::AccessFlags::UNIFORM_READ);
device.cmd_pipeline_barrier(cmd, HOST, COMPUTE_SHADER, ..., &[ubo_barrier], &[], &[]);
```
A separate HOST→COMPUTE memory barrier is emitted on every SVGF dispatch for the params UBO write.
**Why bug**: Not incorrect. Composite recently folded the same HOST→FRAGMENT barrier into the bulk pre-render barrier (#909). SVGF still emits its own — duplicated GPU work.
**Fix**: Hoist HOST→{COMPUTE, FRAGMENT} into the existing bulk barrier in `draw.rs` after host writes to per-frame UBOs (svgf params + composite params + taa params).
**Confidence**: HIGH
**Dedup**: Mirror of #909 (which solved the composite half).

### [LOW] SVGF pre-dispatch image barrier `src_stage_mask` over-specifies FRAGMENT_SHADER
**Dimension**: Denoiser & Composite
**Location**: `crates/renderer/src/vulkan/svgf.rs:866-875`
**Severity**: LOW
**Observation**:
```rust
device.cmd_pipeline_barrier(
    cmd,
    vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
    vk::PipelineStageFlags::COMPUTE_SHADER,
    ...
);
```
The pre-dispatch image barrier widens `src` to `COMPUTE | FRAGMENT`, citing "descriptor sampling of the same slot in the previous frame." Composite reads SVGF output via FRAGMENT through `indirectTex`, not through the OUT slot view directly — those reads are serialised by the per-FIF fence; the only true previous-stage producer is COMPUTE.
**Why bug**: Spec-legal but over-specified. Cosmetic over-sync; the in-flight fence already orders FRAGMENT consumption of the prior frame's slot.
**Fix**: Drop FRAGMENT_SHADER from `src_stage_mask` on the pre-dispatch image barrier; keep the post-dispatch FRAGMENT|COMPUTE dst widening (#653) intact.
**Confidence**: MED
**Dedup**: None.

### [LOW] Composite render-pass external dep lacks UNIFORM_READ
**Dimension**: Denoiser & Composite
**Location**: `crates/renderer/src/vulkan/composite.rs:404-415`
**Severity**: LOW
**Observation**:
```rust
.dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER)
.dst_access_mask(vk::AccessFlags::SHADER_READ);
```
The UBO host-write→fragment-uniform-read execution dependency is currently covered by the bulk pre-render barrier (#909). The composite render-pass external dependency itself never enumerates UNIFORM_READ.
**Why bug**: Validation-clean today because #909 covers the UBO. If someone removes or restructures the bulk barrier so composite is omitted, the render-pass external dep wouldn't pick up the UBO read. Defence-in-depth gap.
**Fix**: Add `vk::AccessFlags::UNIFORM_READ` to `composite_dep_in.dst_access_mask` so the render-pass dependency stands on its own.
**Confidence**: MED
**Dedup**: Adjacent to #909 invariant.

### [LOW] SVGF `should_force_history_reset` shares counter across FIF slots
**Dimension**: Denoiser & Composite
**Location**: `crates/renderer/src/vulkan/svgf.rs:132-134, 805-809`
**Severity**: LOW
**Observation**:
```rust
pub(super) fn should_force_history_reset(frames_since_creation: u32) -> bool {
    frames_since_creation < MAX_FRAMES_IN_FLIGHT as u32
}
```
The counter is shared across both FIF slots; advances on every dispatch, resets only on `recreate_on_resize`. Sky/alpha-blend early-out at `svgf_temporal.comp:93-97` writes `moments.b = 0` — interleaving frame-A vs frame-B determines which slot was "primed" before the gate ends.
**Why bug**: Safe under current `MAX_FRAMES_IN_FLIGHT == 2` with strict alternating frame index — the gate closes after frame 1 when both slots have been written. At `MAX_FRAMES_IN_FLIGHT == 3` (which #918's `const_assert >= 2` permits) the gate closes while one of three slots is still UNDEFINED-in-spirit.
**Fix**: Track per-slot freshness via `frames_since_creation: [u32; MAX_FRAMES_IN_FLIGHT]`, OR change the gate to `frames_since_creation < MAX_FRAMES_IN_FLIGHT * 2`.
**Confidence**: LOW (conditional on a future invariant change)
**Dedup**: Adjacent to #918 / #648.

## Prior-Audit Cross-checks

- **#917 (SVGF `frames_since_creation` increments on dispatch, not GPU completion)** — confirmed live; downgrade from MEDIUM to LOW. The in-flight fence resync papers over this; by the time the counter rolls past MAX_FRAMES_IN_FLIGHT, all subsequent draws have completed (or #479 fallback engages).
- **#928 (volumetric dispatch gate)** — closed and verified.
- **#904 (15-bit mesh-id mask in SVGF)** — closed and verified.
- **#909 (composite UBO host barrier folded into bulk)** — closed and verified; SVGF half not yet folded (LOW finding above).
- **#865 (fog curve consumption)** — verified working; CompositeParams pin intact.

## Prioritized Fix Order

None urgent. If filing the four LOWs:

1. **LOW** — Hoist SVGF HOST→COMPUTE barrier into the bulk pre-render barrier (mirrors #909).
2. **LOW** — Add `UNIFORM_READ` to composite render-pass external dep (defence-in-depth).
3. **LOW** — Drop `FRAGMENT_SHADER` from SVGF pre-dispatch image barrier `src_stage_mask` (over-spec).
4. **LOW** — Per-FIF `frames_since_creation` array, or 2× gate constant. Only matters if MAX_FRAMES_IN_FLIGHT is ever bumped past 2.

Also re-evaluate #917 severity — currently MEDIUM in tracker, audit recommends LOW.

## Notes

- Dimensions 1, 3, 4, 5, 8, 9, 10 are all CLEAN as of 2026-05-11. Broad 2026-05-09 sweep covered 2, 6, 7, 11–16.
- Checklist item 10 (SSAO multiplies indirect at composite) doesn't apply — SSAO is consumed in `triangle.frag` and baked before composite. The orchestrator/checklist should retarget this to Dim 6 (Shader Correctness) or drop it.
