# Renderer Audit — 2026-05-06

**Scope**: Dimensions 4, 5, 6 (Render Pass & G-Buffer / Command Buffer Recording / Shader Correctness)
**Coverage**: 3 dimensions, run in parallel via `renderer-specialist` agents
**Repo state**: `main` @ HEAD (commits `0ef36fa` #570, `9784b43` #376, `3852bc9` #91, `cda40a1` #625, `6995a7c` #845, `f813546` #864, `8862394` #863, `a34cb04` #861, `ffaf74a` #860, `cd0265c` #869, `ef86bbd` #762 probe in tree)

## Executive Summary

**Verdict: clean.** No CRITICAL, HIGH, MEDIUM, or LOW findings across the three audited dimensions. The renderer's structural contracts (G-buffer attachments, command-buffer ordering, shader-side struct layouts) are well-pinned by the existing test suite (`cargo test -p byroredux-renderer` reports **152 passed / 0 failed**) plus zero SPV drift across all 12 shaders.

Three INFO observations and one **stale-issue identification** (#51) round out the audit. No regression-class findings; no spec violations.

| Dim | Topic | Findings | Pass-rate evidence |
|-----|-------|----------|--------------------|
| 4 | Render Pass & G-Buffer | 1 INFO (subpass-dep clarification) | 152/0 + zero SPV drift on triangle.frag |
| 5 | Command Buffer Recording | 2 INFO (TAA-ordering audit-spec mismatch + #51 stale) | 152/0 |
| 6 | Shader Correctness | 1 INFO (`VERTEX_STRIDE_FLOATS` docstring stale) | 152/0 + zero drift across 12 SPVs |

## Rasterization Assessment — Render Pass + G-Buffer + Command Recording

**Overall: spec-compliant, no race hazards exposed by static analysis.**

- **6 color + 1 depth attachment**, all `LOAD_OP_CLEAR` / `STORE_OP_STORE`. Final layouts: `SHADER_READ_ONLY_OPTIMAL` for the color set, `DEPTH_STENCIL_READ_ONLY_OPTIMAL` for depth. (`crates/renderer/src/vulkan/context/helpers.rs:70-98`)
- **G-buffer image usage** — every attachment has `COLOR_ATTACHMENT | SAMPLED`; depth is `DEPTH_STENCIL_ATTACHMENT | SAMPLED`. (`crates/renderer/src/vulkan/gbuffer.rs:88` + `helpers.rs:273`)
- **Format/shader contract verified**: `outNormal` (vec2) → `RG16_SNORM` octahedral (Schied 2017); `outMotion` → `R16G16_SFLOAT`; `outMeshID` → `R16_UINT` (intentional 15-bit id + bit 15 = ALPHA_BLEND_NO_HISTORY flag, see Notes); `outRawIndirect` + `outAlbedo` → `B10G11R11_UFLOAT_PACK32`; HDR direct → `RGBA16F`.
- **Subpass dependencies** correctly cover both incoming (write-after-read on prev frame's G-buffer slot) and outgoing (`COLOR_ATTACHMENT_OUTPUT | LATE_FRAGMENT_TESTS` → `FRAGMENT_SHADER | COMPUTE_SHADER` for SVGF / composite / SSAO consumers). `BOTTOM_OF_PIPE` already removed per Sync2 spec (#573 / SY-2). (`helpers.rs:149-176`)
- **Resize symmetry verified**: `recreate_swapchain` destroys + rebuilds depth + all 5 G-buffer attachments + re-runs `initialize_layouts` for first-frame SVGF temporal validity. (`resize.rs:257-272`, `gbuffer.rs:308-360`)
- **Shared depth image hazard** — analysed and **mitigated** by the double-fence wait at `draw.rs:108-120` (#282) which under `MAX_FRAMES_IN_FLIGHT = 2` waits on every in-flight slot before frame N+1 touches the shared depth buffer. See Notes for the static-assert recommendation if `MAX_FRAMES_IN_FLIGHT` is ever raised.

**Command-buffer recording sequence verified** (per Dim 5 forensic walk of `draw.rs::draw_frame`):

1. Per-frame fence wait + reset (`:108-156`) → `reset_command_buffer` (`:193`) → `begin_command_buffer` (`:202`) — no `?` early-returns inside the begin/end scope.
2. Skin compute → BLAS refit → TLAS build all recorded **outside** any render pass (`:587-755`).
3. `cmd_begin_render_pass` for the main pass at `:1149` → batched draws → `cmd_end_render_pass` at `:1580`.
4. SVGF (`:1609`) → caustic (`:1645`) → TAA (`:1668`) → SSAO (`:1691`) → composite (`:1814`).
5. `composite.dispatch` internally records its own `begin_render_pass` → fullscreen → `end_render_pass` (`composite.rs:761-794`); swapchain transitions `UNDEFINED → COLOR_ATTACHMENT_OPTIMAL → PRESENT_SRC_KHR`.
6. `end_command_buffer` (`:1824`) → `queue_submit` waiting on `image_available` at `COLOR_ATTACHMENT_OUTPUT`, signalling `render_finished` + `in_flight` fence.

**Dynamic state coverage** — every state declared dynamic on the pipeline (`pipeline.rs:280-288`) is emitted in the draw loop (initial + per-batch gated re-emit): `cmd_set_depth_bias` / `_depth_test_enable` / `_write_enable` / `_compare_op` / `_cull_mode` / `_viewport` / `_scissor`. Two-sided alpha-blend correctly toggles FRONT/BACK around the same batch via the `set_cull` closure (`draw.rs:1404-1409`).

**Validation layers enabled** in debug builds via `cfg!(debug_assertions)` gate on `VK_LAYER_KHRONOS_validation` (`instance.rs:9-12, 41-49`).

## Shader Correctness Assessment

**SPV drift: zero.** All 12 freshly-recompiled shaders are byte-identical to checked-in `.spv`:

| Shader | Bytes |
|--------|-------|
| triangle.vert | 12 652 |
| triangle.frag | 103 852 |
| svgf_temporal.comp | 13 520 |
| composite.vert | 1 328 |
| composite.frag | 16 932 |
| ssao.comp | 6 996 |
| cluster_cull.comp | 11 732 |
| taa.comp | 13 880 |
| caustic_splat.comp | 14 096 |
| skin_vertices.comp | 10 608 |
| ui.vert | 3 596 |
| ui.frag | 868 |

**Rust ↔ GLSL contract pins all hold:**

- `GpuInstance` lockstep across the 3 shaders that consume it (triangle.vert, triangle.frag, ui.vert) — aligned with `crates/renderer/src/vulkan/scene_buffer.rs::GpuInstance`.
- `GpuMaterial` 260-byte invariant (#804) + per-field offsets (#806) — both regression tests pass.
- `material_kind = uint` post-#570 (commit `0ef36fa`) — GLSL `uint` is already 32-bit; no shader-side change required.
- Vertex attributes (`Vertex` struct ↔ `triangle.vert layout(location = N)`) — aligned at 25 f32 / 100 B per side.
- TLAS binding declared as `accelerationStructureEXT` at set 1, binding 2.
- Closest-hit / ray-query intersection paths use `gl_InstanceCustomIndexEXT` (NOT `gl_InstanceID`) for SSBO lookup.
- `sceneFlags.x > 0.5` RT gate precedes every `rayQueryInitializeEXT` invocation.
- `validate_set_layout` (descriptor reflection at pipeline-create time) called from 8 production sites — pipeline reflection IS gated.

## Findings

### [INFO] RP-DIM4-01: Outgoing subpass dependency stage masks documented for completeness
**Dimension**: Subpass Dependencies
**Location**: `crates/renderer/src/vulkan/context/helpers.rs:149-176`
**Status**: NEW (informational only)

`dependency_out` covers `COLOR_ATTACHMENT_OUTPUT | LATE_FRAGMENT_TESTS` in `src_stage_mask` and `FRAGMENT_SHADER | COMPUTE_SHADER` in `dst_stage_mask`. Spec-compliant; comment at `:160-171` explicitly notes that `BOTTOM_OF_PIPE` was dropped per Sync2 spec via #573 / SY-2. No fix needed.

### [INFO] D5-OBS-01: Audit-spec vs implementation TAA ordering mismatch
**Dimension**: Command Recording
**Location**: `crates/renderer/src/vulkan/context/draw.rs:1655-1678`
**Status**: NEW (audit-spec issue, not a renderer issue)

The `/audit-renderer` task spec for Dim 5 states "TAA compute runs after composite, before present." The implementation runs TAA **before** composite (TAA at `:1666`, composite at `:1814`). The data dependency requires this ordering: composite's binding 0 samples TAA's HDR output; the `composite.fall_back_to_raw_hdr` path rebinds binding 0 to raw HDR when TAA permanently fails (`:1666-1678`). Running TAA after composite would feed YCoCg neighborhood clamp the tone-mapped LDR swapchain image — wrong domain.

**Impact**: None on the renderer. Update audit-spec wording to "TAA compute runs after SVGF/caustic, **before** composite" so future audits don't re-flag this.

### [INFO] D5-OBS-02: Issue #51 ("Perf: unconditional `cmd_set_depth_bias`") is stale
**Dimension**: Dynamic State
**Location**: `crates/renderer/src/vulkan/context/draw.rs:1346-1351`
**Status**: EXISTING (close issue #51)

Per-batch `cmd_set_depth_bias` is already gated on `last_render_layer != Some(batch.render_layer)` — the per-render-layer ladder collapses redundant emissions. The unconditional emission referenced in #51 is the once-per-frame initial set at `:1261`, which is **required** by Vulkan spec when the pipeline declares `DEPTH_BIAS` as dynamic. The issue's premise is invalidated by the layer-key gate.

**Suggested action**: Close issue #51 — the perf-tracking concern is already addressed.

### [INFO] D6-INFO-01: `VERTEX_STRIDE_FLOATS = 21` docstring stale (actual = 25)
**Dimension**: Vertex Attributes
**Location**: `crates/renderer/src/vertex.rs` + `crates/renderer/shaders/skin_vertices.comp`
**Status**: NEW (doc-string staleness only)

The audit-task spec cited `VERTEX_STRIDE_FLOATS = 21` (84 B / vertex) per the M29.3 GPU skinning roadmap entry. Current vertex stride agrees Rust ↔ GLSL at 25 floats / 100 B — the M-NORMALS tangent + bitangent-sign extension widened the layout but the docstring / ROADMAP entry never caught up. Tests are green (152/0).

**Impact**: None on correctness; documentation drift only.

**Suggested fix**: Update the M29.3 / `VERTEX_STRIDE_FLOATS` reference in the affected README / ROADMAP / audit-template to 25 floats / 100 B.

## Verifications

### Dim 4 — Render Pass & G-Buffer

- [x] Attachment load/store ops: CLEAR / STORE for all 6 color + depth; stencil DONT_CARE/DONT_CARE
- [x] Layout transitions on render-pass begin/end: UNDEFINED → COLOR_ATTACHMENT_OPTIMAL → SHADER_READ_ONLY_OPTIMAL
- [x] Subpass dependencies cover SVGF / SSAO compute reads + composite fragment reads
- [x] Format choices match shader contract (5 attachments verified, see RP table above)
- [x] Depth attachment hazard analysis: shared depth image is safe under `MAX_FRAMES_IN_FLIGHT = 2` via double-fence wait
- [x] Image usage flags include SAMPLED on every attachment
- [x] Resize recreation symmetric across all G-buffer attachments
- [x] SPV drift on triangle.frag: zero (103 852 B byte-identical)
- [x] Tests: 152 passed / 0 failed

### Dim 5 — Command Buffer Recording

- [x] Pool flags include `RESET_COMMAND_BUFFER`: PASS (`helpers.rs:470`)
- [x] Per-frame fence wait + reset before cmd-buffer touch: PASS (`:107-156`)
- [x] `begin/end_command_buffer` and `cmd_begin/end_render_pass` balanced (no `?` early-return between)
- [x] TLAS build outside any render pass: PASS (`:729` < `:1149`)
- [x] Skin compute → BLAS refit → render-pass-begin ordering correct: PASS
- [x] SVGF dispatch after `cmd_end_render_pass` + before composite: PASS (`:1609` between `:1580` and `:1814`)
- [x] Composite swapchain transitions: PASS (`UNDEFINED → COLOR_ATTACHMENT_OPTIMAL → PRESENT_SRC_KHR`)
- [x] TAA placement: BEFORE composite by design (D5-OBS-01)
- [x] Caustic accumulator: clear + 4 barriers + dispatch + post-dispatch barrier all present
- [x] Per-draw dynamic state coverage: 7 states, all emitted (initial + per-batch gated)
- [x] Validation layers enabled in debug
- [x] Recent commits (#801, #821, #826) clean of `draw.rs`

### Dim 6 — Shader Correctness

- [x] All 12 SPVs byte-identical to fresh rebuild
- [x] `GpuInstance` lockstep across 3 shaders
- [x] `GpuMaterial` 260-byte size + per-field offsets (#804 / #806 tests pass)
- [x] `material_kind = uint` (post-#570)
- [x] Vertex attribute alignment Rust ↔ GLSL (25 f32 / 100 B)
- [x] TLAS binding `accelerationStructureEXT` at set 1, binding 2
- [x] `gl_InstanceCustomIndexEXT` (not `gl_InstanceID`) on RT paths
- [x] RT gate `sceneFlags.x > 0.5` precedes every `rayQueryInitializeEXT`
- [x] SVGF temporal: motion + mesh_id reproject + alpha clamp + first-frame reset path
- [x] Composite math: ACES + fog-to-direct + caustic-to-direct (#321 Option A)
- [x] TAA: Halton + YCoCg + reset path (#801 / commit `f3dc1ee` wiring intact)
- [x] Skin compute: stride 25, `local_size_x = 64`
- [x] `validate_set_layout` reflection: 8 production call sites

## Cross-Dimension Notes

- **`mesh_id` is `R16_UINT`**, not the `R32_UINT` cited in audit prompts. Intentional bandwidth optimisation: 15-bit id + bit 15 = `ALPHA_BLEND_NO_HISTORY` flag for SVGF disocclusion. Hard ceiling at 32 767 instances per frame is guarded by a `debug_assert!` in `draw.rs::draw_frame`. Documented at `helpers.rs:54-62`. **Update audit-renderer.md prompt** to drop the `R32_UINT` claim.
- **Shared depth image** (single `vk::Image`, not per-frame-in-flight) is safe today because `MAX_FRAMES_IN_FLIGHT = 2` and `draw.rs:108-120` waits on both in-flight fences before each frame. Recommendation: add a `static_assert!(MAX_FRAMES_IN_FLIGHT == 2)` near the depth declaration so the invariant doesn't silently break if anyone bumps the constant.
- **#51 (perf: unconditional `cmd_set_depth_bias`)** is stale — close it. The per-render-layer gate at `draw.rs:1346-1351` already collapses redundant emissions.
- **#661 (SY-4: skin compute → BLAS refit barrier uses legacy `ACCELERATION_STRUCTURE_READ_KHR`)** remains accurate per static analysis at `draw.rs:603` — out of scope for Dims 4-5-6 (Dim 1 / Vulkan Sync), but cross-flagged here so the next sync-focused audit doesn't miss it.

## Prioritized Fix Order

Nothing CRITICAL / HIGH / MEDIUM / LOW. The audit is clean.

**Cleanup actions** (housekeeping only, all optional):

1. **Close #51** as stale (D5-OBS-02). The perf concern was already addressed by the per-render-layer gate; the open issue misleads.
2. **Update audit-renderer.md prompt** to fix two stale claims:
   - Dim 4: `mesh_id` format is `R16_UINT` (not `R32_UINT`).
   - Dim 5: TAA runs **before** composite (not after). The data dependency is composite ← TAA HDR.
3. **Update `VERTEX_STRIDE_FLOATS` documentation** (D6-INFO-01) — actual stride is 25 f32 / 100 B, not 21 / 84.
4. **Add `static_assert!(MAX_FRAMES_IN_FLIGHT == 2)`** near the depth-image declaration (`mod.rs:580-582`) cross-referencing `draw.rs:108-120` as the safety contract.

---

Suggested next step: `/audit-publish docs/audits/AUDIT_RENDERER_2026-05-06.md` if any of the housekeeping items are worth filing as tracker issues. Note: items 1 + 2 are audit-template hygiene, items 3 + 4 are docs / static-assert touches — all LOW or INFO; the publish step may emit zero new issues.
