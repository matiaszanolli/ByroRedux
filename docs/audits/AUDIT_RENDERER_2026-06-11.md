# Renderer Audit — 2026-06-11

**Scope**: Full Vulkan renderer, all 23 dimensions, `--depth deep`.
**Baseline**: HEAD `1e8a25ab`. Prior full audit `AUDIT_RENDERER_2026-06-09.md` at `67e1baaf` (2 days ago, largely clean). Dedup against 33 open GitHub issues + `docs/audits/`.
**Trigger**: `/audit-suite --preset renderer-deep` after the camera-relative rendering precision work (`bccf06f0` render-origin field, `36f66493` full cascade, PR #1485), with particular attention on those commits; all 23 dimensions still executed.
**Method**: One Task agent per dimension; conflicting skinned-path claims from early dimensions were reconciled by a dedicated Dim-12 deep dive, and the three highest-impact findings (REN2-01, REN2-02, REN2-03) were independently re-verified against source by the orchestrator before inclusion.

---

## Executive Summary

The audit found **0 CRITICAL, 3 HIGH, 3 MEDIUM, 14 LOW** (20 total after cross-dimension dedup). All four actionable findings from the 2026-06-09 audit are confirmed **fixed** at HEAD (`04acaa2b` App Drop ordering, `73a43fc8` hostQueryReset, `2f7bcf78` TAA luma-clamp gating, `44171cd5` spawn-time roughness — each verified correct and complete by the owning dimension).

The headline result: **the camera-relative cascade (36f66493) is correct for rigid geometry, TLAS, lights, cluster culling, volumetrics, SSAO/fog, and water surfaces — but it missed the skinned path and the caustic re-projection sites.** Two of the three HIGHs are regressions introduced by that commit; the third (skinned TLAS double-transform) is a pre-existing M29-era bug the cascade investigation surfaced:

- **REN2-01 (HIGH, regression)** — the skinned raster branch in `triangle.vert` builds positions from **absolute-world bone palettes** and projects them with the now-**relative** `viewProj`. Every skinned mesh (NPCs, creatures) rasterizes displaced by the full render origin — i.e. typically off-screen/invisible — whenever the camera is outside the single `[0,4096)³` world box (virtually all exterior play, and any interior with a negative coordinate). The Markarth verification scene contained no actors, which is why this shipped unnoticed.
- **REN2-02 (HIGH, pre-existing since M29 Phase 2)** — skinned BLAS vertices are already absolute world (palette = boneWorld × bindInverse), yet the TLAS instance applies the entity's absolute `model_matrix` on top: a double transform that displaces the **RT presence** (shadows cast, GI/reflection subject) of every placed actor, independent of render origin.
- **REN2-03 (HIGH, regression)** — both caustic deposit writers (`caustic_splat.comp:339`, `water.frag:585`) project an **absolute** TLAS landing point with the **relative** `viewProj`; the NDC guard then culls essentially every splat, so glass and water caustics silently vanish at any non-zero render origin.

The MEDIUMs are all bounded-impact: a one-frame full-screen motion-vector pulse on every 4096-unit origin snap (TAA/SVGF history drop, gracefully degraded but recurrent, and the in-code mitigation claim is false), a long-standing `screen_to_world_dir` direction skew in composite that the cascade shrank but made discontinuous, and an error-path render-pass imbalance in the egui overlay.

| Severity | Count | IDs |
|----------|-------|-----|
| CRITICAL | 0 | — |
| HIGH | 3 | REN2-01, REN2-02, REN2-03 |
| MEDIUM | 3 | REN2-04, REN2-05, REN2-06 |
| LOW | 14 | REN2-07 … REN2-20 |

**Known-OPEN issues confirmed still present and NOT re-reported**: #1481 (SVGF firefly clamp scope), #1482 (DBG value-pin gap), #1483 (timer-pool leak on allocator-None Drop), #1484 (renderer doc-rot batch), #1438, #1433, #1427, #1426 (premise looks partially stale — `device_wait_idle` now runs unconditionally at Drop entry, worth a re-triage), #1404, #1387, #1384, #1369, #1357.

---

## RT Pipeline Assessment

**Acceleration structures (Dim 8)**: the cascade is clean on the RT side for rigid geometry — TLAS instances are built from the **absolute** `draw_cmd.model_matrix` (`tlas.rs:180`); the origin rebase touches only the `GpuInstance` SSBO copy, whose sole RT consumer (`getHitTriNormal`) is translation-invariant. All prior pins hold (#1226/#1227/#1144/#1145, UPDATE-mode guards, instance_custom_index mapping, two_sided-gated cull-disable). The exception is the skinned double-transform (REN2-02). The |world| ≈ 176k f32 question resolves benignly: ~0.03 u quantization vs the 0.05–0.15 bias/tMin margins keeps 2–3× headroom until worldspaces approach ~0.7–1 M units (REN2-10, LOW note).

**Ray queries (Dim 9)**: fully consistent post-cascade — ray origins (reconstructed absolute `fragWorldPos`), TLAS transforms, camera position, and light SSBO are all absolute; worst-case reconstruction quantization (~0.023 u) leaves positive clearance at every one of the 6 `rayQueryEXT` sites. Flags, Frisvad bases, #789 glass keying, IGN seeding, and barycentric SSBO math all spot-verified; fresh `.spv` recompiles byte-match. One LOW improvement opportunity on derivative consumers (REN2-11).

**Denoiser & composite (Dim 10)**: motion-vector/reassembly chain, ping-pong, disocclusion, ACES order, fog-on-direct, SSAO-on-indirect, caustic decode all verified; composite/SSAO camera-position uses are origin-invariant **except** `screen_to_world_dir` (REN2-05). The origin-snap MV pulse degrades SVGF to a graceful full-frame history drop (bounds-check rejection), not ghosting — see REN2-04.

**Sync (Dim 1)**: no new hazards from the delta — all new UBO bytes ride existing per-frame host-coherent buffers with existing barriers; fence/semaphore/TLAS-build/SVGF/G-buffer barrier verdicts from the baseline all hold at HEAD.

---

## Rasterization Assessment

**Pipeline state (Dim 3)**: the CameraUBO 336 B layout change is airtight — Rust `#[repr(C)]` struct, both lockstep pin tests, and `spirv-dis` of all 6 shipped mirrors confirm `renderOrigin` at offset 320. Vertex input (9 attrs, 100 B), push constants, dynamic state, and reflection-validated compute layouts all verified.

**Render pass & G-buffer (Dim 4)**: structurally untouched since the clean baseline (helpers.rs/gbuffer.rs no delta); all attachment/format/dependency items re-verified, mesh-ID warn+clamp guard intact.

**Command recording (Dim 5)**: recording skeleton unchanged and correct; the render-origin work is recorded once per frame, per-frame-in-flight safe, batching-neutral; the `01251733` mesh-index diagnostic sits off the hot path. #1258/#1259/#1260/#1235 and per-image `render_finished` semaphores re-verified.

**Material table (Dim 14) & NIFAL (Dim 22)**: `44171cd5` honors its contract — spawn-once, idempotent roughness write-back at both spawn sites, zero remaining render-time roughness/metalness mutation, canonical `Material` flows untouched to `MaterialTable::intern`; `GpuMaterial` 300 B + 65 offset pins and `GpuInstance` 112 B across all 5 mirrors hold. Dims 7, 19, 20, 22 came back fully clean; Dims 15, 16, 17, 18, 21, 23 clean apart from the items below.

---

## Findings

### HIGH

#### REN2-01: Skinned raster path not rebased — every skinned mesh displaced by render_origin
- **Severity**: HIGH
- **Dimension**: GPU Skinning (canonical; independently found by Dims 1, 3, 5, 6, 11, 12)
- **Location**: `crates/renderer/shaders/triangle.vert:141-165,190`; root data path `byroredux/src/render/skinned.rs:160-167` → `crates/renderer/src/vulkan/context/draw.rs:835-841` → `crates/renderer/shaders/skin_palette.comp:78`; rigid-only rebase at `draw.rs:1759-1768`
- **Status**: NEW — regression introduced by `36f66493` (PR #1485)
- **Description**: The cascade rebased rigid per-instance model translations and made the uploaded `viewProj` camera-relative, but bone palettes remain absolute world (`bone_world × bind_inverse`, placement included) and the skinned vertex branch ignores `inst.model` entirely. `worldPos = Σwᵢ·palette[bᵢ] · pos` is absolute, fed into the relative `viewProj` → rendered as if at `p + render_origin`. Additionally `fragWorldPos = worldPos.xyz + renderOrigin.xyz` (`triangle.vert:190`) **double-adds** the origin for skinned fragments, so even visible skinned fragments get lighting/RT-origin/fog positions wrong by +origin. Orchestrator re-verified the shader branch and the verbatim palette upload directly.
- **Evidence**: No `renderOrigin` reference exists in the skinned branch; `upload_bone_worlds` receives unmodified `gt.to_matrix()` output; the `#markarth-precision` comment above `fragWorldPos` asserts "`worldPos` is in render-origin-relative space", which is false for the skinned branch.
- **Impact**: Whenever `render_origin ≠ 0` — camera outside the single `[0,4096)³` box, i.e. virtually all exterior play and any interior with a negative camera coordinate (the `floor()` snap makes any negative component yield −4096) — every skinned mesh rasterizes displaced ≥4096 units, typically invisible. Pre-delta this path was correct.
- **Suggested Fix**: In the skinned branch subtract `renderOrigin.xyz` from `worldPos` (and `prevWorldPos`) immediately after the palette blend, making the skinned path origin-relative like the rigid path; `fragWorldPos = worldPos + renderOrigin` then becomes uniformly correct. Keep palettes absolute so `skin_vertices.comp`/BLAS stay world-space. Recompile `triangle.vert.spv` (plain `-V`).
- **Related**: REN2-02, REN2-04; `docs/smoke-tests/m41-equip.sh` is the runtime check once fixed.

#### REN2-02: Skinned TLAS instances double-apply the entity transform (world-space BLAS × absolute model_matrix)
- **Severity**: HIGH
- **Dimension**: Acceleration Structures / GPU Skinning (Dims 8 + 12, independently converged)
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:177-180` (shared transform site, no skinned special-case); vertex space: `crates/renderer/shaders/skin_vertices.comp:113-120` ("skinned meshes encode the world transform through the bone palette"); matrix source `byroredux/src/render/static_meshes.rs:351,553`
- **Status**: NEW — pre-existing since M29 Phase 2, independent of the camera-relative delta (which correctly left `DrawCommand.model_matrix` absolute)
- **Description**: Skinned BLAS geometry is built/refit from the `skin_vertices.comp` output buffer, which is **already absolute world** (placement included via bone GlobalTransforms). The TLAS instance for `bone_offset != 0` draws nevertheless falls through to the shared `column_major_to_vk_transform(&draw_cmd.model_matrix)` — the mesh entity's absolute GlobalTransform (never identity for placed actors; mesh entities are parented under the REFR placement root, `spawn.rs:213-218`). The placement is applied twice. Orchestrator re-verified: `static_meshes.rs` emits `transform.to_matrix()` for all draws with no skinned override, and `tlas.rs` has no `bone_offset` branch at the transform site. The "skinned draws carry identity model_matrix" hypothesis is disproven — raster was unaffected pre-delta only because `triangle.vert` ignores `inst.model` in the skinned branch.
- **Impact**: The RT presence (shadow caster, reflection/GI subject) of every placed skinned actor sits at `R·w + t` instead of `w` (≈2× placement displacement near identity rotation): actors cast no shadow at their visual location and a phantom occluder exists elsewhere. Affects all games' NPCs since M29 Phase 2, any origin. Note: the `_audit-severity` table's "TLAS build with wrong geometry/address → CRITICAL" row was considered; rated HIGH because geometry and addresses are valid — the instance transform is wrong, producing mis-placed (not corrupt) RT geometry and no crash path.
- **Suggested Fix**: Emit an identity `VkTransformMatrixKHR` for TLAS instances with `draw_cmd.bone_offset != 0` (one branch at `tlas.rs:177-180`). Verify at runtime via a `byro-dbg` attach on an FNV/Skyrim NPC cell (actor shadow position) — the fix itself is code-inspectable; no RenderDoc-dependent sync change involved.
- **Related**: REN2-01; the prior audit's "Dim 12 clean" verdict covered sync/layout, never the space-convention × TLAS-transform composition — gap, not regression.

#### REN2-03: Caustic deposit re-projection multiplies ABSOLUTE landing points by the RELATIVE viewProj — caustics vanish at any non-zero render origin
- **Severity**: HIGH
- **Dimension**: Caustic Splat / Water (canonical Dim 13; independently found by Dims 3, 6, 17)
- **Location**: `crates/renderer/shaders/caustic_splat.comp:339` and `crates/renderer/shaders/water.frag:585`
- **Status**: NEW — regression introduced by `36f66493`
- **Description**: Both caustic writers correctly lift their unprojected G-buffer position by `+renderOrigin` (absolute) and trace against the absolute TLAS, but then re-project the landing point with the now-relative `viewProj`: `clip = viewProj * vec4(P, 1.0)` (caustic_splat) / `floorClip = viewProj * vec4(floorWorld, 1.0)` (water.frag). Since `VP_rel·x ≡ VP_abs·(x + o)`, the projected point is displaced by the full origin `o`. Orchestrator re-verified both sites.
- **Evidence**: With `|o| ≥ 4096`, NDC almost always falls outside ±1 and the guards (`caustic_splat.comp:342`, `water.frag:588-590`) silently `continue` — every splat dropped; when `o` aligns with the view direction the misprojection can pass the guard and deposit ghost caustics at wrong pixels. `water.frag:113` even carries a comment that `renderOrigin` is "Unused here".
- **Impact**: Glass caustics (#321) and water floor caustics (#1210 Phase E) silently disappear in nearly all real game content — only the `[0,4096)³` origin cell still works (why the cascade's manual checks missed it). No corruption; pure feature-loss regression under realistic conditions.
- **Suggested Fix**: Subtract the origin before projecting at both sites (`viewProj * vec4(P - renderOrigin.xyz, 1.0)`), fix the `water.frag:113` comment, recompile both `.spv`.

### MEDIUM

#### REN2-04: Origin-jump frame pairs old-origin `prevViewProj` with new-origin geometry — full-screen motion-vector pulse per 4096-unit grid crossing
- **Severity**: MEDIUM (borderline HIGH under "wrong SVGF motion vectors"; rated MEDIUM because the vectors are wrong only on the discrete jump frame and no persistent state corrupts)
- **Dimension**: cross-cutting (Dims 1, 4, 5, 6, 11; canonical write-up Dim 4)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:617,667-672,826` (`prev_view_proj` stored without its origin); `crates/renderer/shaders/triangle.vert:232`; `byroredux/src/render/camera.rs:93-97,156`
- **Status**: NEW — introduced by `36f66493`
- **Description**: `prev_view_proj` is last frame's relative matrix (origin O₁); this frame's positions are rebased by the current origin O₂. On any frame the camera crosses a 4096-unit grid line, `fragPrevClipPos = prevViewProj * (P_abs − O₂)` is off by ΔO (up to 4096 u/axis) → the motion-vector attachment is garbage for the entire screen for one frame. The mitigating comment at `camera.rs:93-97` ("the origin only moves when … streaming already resets temporal continuity") is factually wrong: streaming never resets TAA/SVGF history (`should_force_history_reset` fires only on resource recreation), and the raw `floor()` snap has no hysteresis, so oscillating near a grid line re-triggers every crossing.
- **Impact**: One-frame full-screen TAA aliasing flash + SVGF indirect-noise burst (graceful full-frame history drop via the prevUV bounds check, then ~10-20-frame re-convergence) on every grid crossing during exterior traversal. Recurrent in normal gameplay; worst when strafing along a boundary. Skinned MVs are additionally subsumed by REN2-01.
- **Suggested Fix**: Track `prev_render_origin` alongside `prev_view_proj` and upload the origin-corrected matrix `prev_vp · translation(O₂ − O₁)` (exact, keeps MVs valid across crossings). Cheaper fallback: force the SVGF/TAA history-reset on jump frames so the drop is at least intentional. Fix the `camera.rs:93-97` comment either way.

#### REN2-05: composite `screen_to_world_dir` omits the camera offset — sky/sun/cloud/haze direction drifts and pops at origin snaps
- **Severity**: MEDIUM
- **Dimension**: Denoiser & Composite (Dims 6, 10, 15 converged; canonical Dim 10)
- **Location**: `crates/renderer/shaders/composite.frag:104-119` (consumers: sky :320, aerial-perspective haze :559)
- **Status**: NEW (pre-existing bug; the cascade shrank it from ~30° at Markarth coordinates to ≤~1.35° but made it discontinuous)
- **Description**: The function returns `normalize(P_far)` — direction from the coordinate-space origin, not from the camera (`normalize(P_far − camera_pos)`). With the relative `inv_view_proj`, the camera sits up to ~7094 u from the relative origin against a 300000 far plane → up to ~1.35° skew (~75% of the sun disc's angular radius), varying continuously with camera position and jumping at every 4096-unit origin snap.
- **Impact**: Sky-dome swim under camera translation, sun disc misaligned vs the `sun_dir` used for shadows, near-horizon cloud projection error, one-frame sky/haze pop per grid crossing. Exterior-only; no geometry-lighting or SVGF impact.
- **Suggested Fix**: One line: `return normalize(world.xyz / w - params.camera_pos.xyz);` — `params.camera_pos` (already relative, same UBO) is the matching origin. Recompile `composite.frag.spv`.

#### REN2-06: egui `dispatch` error after `cmd_begin_render_pass` leaves the render pass open in a still-recorded command buffer
- **Severity**: MEDIUM (error-path Vulkan validity; would be a spec violation when triggered)
- **Dimension**: Debug Overlay & GPU Telemetry
- **Location**: `crates/renderer/src/vulkan/egui_pass.rs:185-200`; caller `crates/renderer/src/vulkan/context/draw.rs:3196-3220`
- **Status**: NEW
- **Description**: `renderer.cmd_draw(...)` is fallible and its `map_err(...)?` sits between `cmd_begin_render_pass` and `cmd_end_render_pass`. On Err, dispatch returns with the RP open; the caller logs and keeps recording (pending screenshot `vkCmdCopyImage` inside an active RP, then `end_command_buffer` → VUID-vkEndCommandBuffer-commandBuffer-00060) and submits the invalid buffer.
- **Impact**: Requires an egui-ash-renderer internal allocation failure first (e.g. VRAM pressure), but then a frame that should degrade gracefully becomes validation errors / UB in release.
- **Suggested Fix**: Capture the `cmd_draw` Result, call `cmd_end_render_pass` unconditionally after the begin, then propagate. Pure CPU-side recording-balance fix — code-inspectable, no RenderDoc needed.
- **Related**: #1427, #1433 (open, distinct egui issues).

### LOW

#### REN2-07: Camera-relative delta doc-rot cluster (5 sites)
- **Severity**: LOW · **Dimension**: cross-cutting · **Status**: NEW (all introduced or made stale by bccf06f0/36f66493)
- **Locations & issues**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:268-272` (render_origin doc names ssao.comp/composite.frag as CameraUBO declarers — they aren't — and overstates streaming resets); `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:38-59` (`gpu_camera_is_336_bytes` header + assertion message name the wrong reader set); `crates/renderer/src/vulkan/context/draw.rs:1784-1786` (SSBO transform-contract comment predates the rebase); `byroredux/src/render/camera.rs:96` (references the deleted `CameraView::render_origin` field; false "streaming resets temporal continuity" claim — also part of REN2-04's fix); `crates/renderer/shaders/composite.frag:46` (`camera_pos` still documented as "world position" — latent trap for future height-fog work).
- **Suggested Fix**: One doc pass alongside the REN2-01/03/04 fixes.

#### REN2-08: `VolumetricsParams` UBO has no block-size pin against the shipped `.spv`
- **Severity**: LOW · **Dimension**: Volumetrics (Dims 2 + 18) · **Location**: `crates/renderer/src/vulkan/volumetrics.rs:69-92` vs the CameraUBO-only pin pattern in `crates/renderer/src/vulkan/reflect.rs:433-466` · **Status**: NEW
- **Description**: The UBO just grew (render_origin, now 144 B, currently verified matching at offsets 0/64/80/96/112/128) but unlike CameraUBO has no `uniform_block_size_by_name` reflection pin — exactly the stale-`.spv` drift mode the delta risked.
- **Suggested Fix**: Add the same reflection size pin for `VolumetricsParams` (and consider CausticParams while there).

#### REN2-09: `RENDER_ORIGIN_SNAP = 4096.0` duplicated across two crates with no shared constant or lockstep test
- **Severity**: LOW · **Dimension**: Command Recording (Dims 4 + 5) · **Location**: `byroredux/src/render/camera.rs:97` and `crates/renderer/src/vulkan/context/draw.rs:580-585` · **Status**: NEW
- **Description**: The snap quantum appears in both crates guarded only by comments; drift would silently desync the CPU-side origin from the renderer's expectations.
- **Suggested Fix**: Hoist into a shared constant (e.g. `shader_constants_data.rs` or a core export) or add a cross-crate equality test.

#### REN2-10: RT absolute-space f32 quantization headroom — holds today, exhausts near ~0.7–1 M-unit worldspaces
- **Severity**: LOW (informational) · **Dimension**: Acceleration Structures · **Location**: `tlas.rs:180`, `skin_vertices.comp`, `triangle.vert:190`, ray sites in `triangle.frag` · **Status**: NEW
- **Description**: TLAS transforms, skinned BLAS vertices, and reconstructed ray origins stay absolute; at |world| ≈ 176k quantization is ~0.02-0.03 u vs 0.05-0.15 bias/tMin → 2-3× margin. The margin scales linearly with |world|; document the ceiling so future worldspaces don't trip it silently.
- **Suggested Fix**: Record the bound in `docs/engine/shader-pipeline.md` (or a debug_assert on worldspace extents at cell load).

#### REN2-11: Derivative consumers of `fragWorldPos` keep pre-cascade ULP noise at |world| ≥ 131k
- **Severity**: LOW (improvement opportunity, not a regression) · **Dimension**: RT Ray Queries / Tangent-Space (Dims 9 + 16) · **Location**: `crates/renderer/shaders/triangle.vert:190`; consumers `triangle.frag:1312` (flat-shading normal), `:1231-1234` (derivative TBN), `:1122-1125` (POM), `:1643` (rtLOD) · **Status**: NEW
- **Description**: The varying is absolute (`rel + origin` added before interpolation), so `dFdx/dFdy` consumers see ~0.0156 u quantization in far worldspaces (up to ~20% relative derivative noise close-up). Passing the relative position as the varying and reconstructing the absolute in the fragment shader would move quantization after the derivative stage at zero extra varying cost. Needs a RenderDoc capture of flat-shaded close-up content in a |coord|>131k cell to confirm visibility before acting.

#### REN2-12: Moving pixels under a parked camera keep the 1/(N+1) accumulation α
- **Severity**: LOW · **Dimension**: TAA · **Location**: `crates/renderer/shaders/taa.comp:235-263`, `crates/renderer/src/vulkan/taa.rs:684-688` · **Status**: NEW (residual of 2f7bcf78 — the original suggested fix's "normal α=0.1 fall-through" was not shipped)
- **Description**: The YCoCg clamp is re-armed for moving pixels, but α stays at the global static value (down to 1/256 after ~4 s parked) — history can pin at the clamp boundary, causing soft detail-loss on moving actors during long parked-camera scenes. Far milder than the pre-fix #1479 artifact.
- **Suggested Fix**: `float alpha = pixelStatic ? params.params.x : max(params.params.x, 0.1);` + recompile.

#### REN2-13: `water.vert` GpuInstance mirror excluded from the lockstep name-drift guard
- **Severity**: LOW · **Dimension**: Material Table · **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:196-210` vs `gpu_types.rs:27-32` · **Status**: NEW
- **Description**: The struct-name drift test covers 4 of the 5 GpuInstance-declaring shaders; `water.vert` is missing (its layout currently matches — verified).
- **Suggested Fix**: Add `water.vert` to the guarded list.

#### REN2-14: `MaterialTable::intern` doc claims a 4096 cap and pre-split file refs
- **Severity**: LOW · **Dimension**: Material Table · **Location**: `crates/renderer/src/vulkan/material.rs:1030-1046` · **Status**: NEW — doc-rot vs `MAX_MATERIALS = 16384` (`7823eb59`).

#### REN2-15: `NORMAL_ALPHA_SPEC_BIT` has no Rust↔GLSL lockstep pin
- **Severity**: LOW · **Dimension**: Material Table · **Location**: `byroredux/src/material_translate.rs:180` vs `crates/renderer/shaders/triangle.frag:1924-1925` · **Status**: NEW
- **Description**: The gloss-flag bit rides outside the generated-header contract that protects `MAT_FLAG_*`/`DBG_*`; a value flip would compile silently.
- **Suggested Fix**: Route it through `shader_constants_data.rs` like its siblings.

#### REN2-16: `triangle_frag_dbg_bits_not_redeclared` doc-comment counts "10" flags and overstates the value-pin companion
- **Severity**: LOW · **Dimension**: Tangent-Space & Normal Maps · **Location**: `crates/renderer/src/shader_constants.rs:199-207` · **Status**: NEW (adjacent to, but distinct from, open #1482).

#### REN2-17: Procedural water-noise hash degrades at large |world| (absolute-UV lattice overflow)
- **Severity**: LOW · **Dimension**: Water · **Location**: `crates/renderer/shaders/water.frag:139-143,163-198` · **Status**: NEW
- **Description**: `hash21` on absolute world XY genuinely bands at 176k-class coordinates, but only on paths not currently reachable (textured water covers shipping content at ~30× sub-texel margin). Flag for when procedural foam/noise paths activate.

#### REN2-18: `water.frag` push-constant doc claims `time = seconds since cell load`; actual source is engine-uptime `TotalTime`
- **Severity**: LOW · **Dimension**: Water · **Location**: `crates/renderer/shaders/water.frag:52-53` vs `byroredux/src/render/water.rs:43-46` · **Status**: NEW — doc-rot (f32 uptime precision is a known long-session wave-quality bound; the doc hides it).

#### REN2-19: `BGSM_AUTHORED` docs claim a fragment-shader spec-glossiness F0 branch that never existed
- **Severity**: LOW · **Dimension**: Disney BSDF / PBR Gating · **Location**: `crates/renderer/src/vulkan/material.rs:500-528`, `byroredux/src/cell_loader.rs:198-203` · **Status**: NEW
- **Description**: The flag's translation is CPU-side; the bit is telemetry-only and never mirrored to GLSL — the docs describe a nonexistent shader branch.

#### REN2-20: Residual `gpu_timers`/`egui_pass` doc-rot beyond the #1484 doc-table item
- **Severity**: LOW · **Dimension**: Debug Overlay & GPU Telemetry · **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:5,47-49,64-70,158`; `crates/renderer/src/vulkan/egui_pass.rs:181-184` · **Status**: NEW (extends Existing #1484)
- **Description**: Includes the host-vs-`cmd_reset_query_pool` line-5 claim the 2026-06-09 audit called out, still present post-73a43fc8.

---

## Prioritized Fix Order

**Correctness regressions from PR #1485 (one fix branch, do first):**
1. **REN2-01** (HIGH) — subtract `renderOrigin` in `triangle.vert`'s skinned branch (positions + prev positions); fixes both the displacement and the fragWorldPos double-add. Re-run `docs/smoke-tests/m41-equip.sh` in an exterior cell as the regression check.
2. **REN2-03** (HIGH) — `P - renderOrigin.xyz` before projection at `caustic_splat.comp:339` and `water.frag:585`; fix the "Unused here" comment.
3. **REN2-04** (MEDIUM) — track `prev_render_origin` and upload the origin-corrected `prev_view_proj`; correct the `camera.rs:93-97` claim.
4. **REN2-05** (MEDIUM) — one-line camera subtract in `screen_to_world_dir`.
5. **REN2-07/REN2-09** (LOW) — the delta doc-rot cluster + shared `RENDER_ORIGIN_SNAP` constant, same branch.

**Pre-existing RT correctness:**
6. **REN2-02** (HIGH) — identity TLAS transform for `bone_offset != 0` instances; verify actor shadow placement via byro-dbg afterwards.

**Hardening / cleanup (batchable):**
7. **REN2-06** (MEDIUM) — balance the egui error path (pairs with open #1427/#1433 work).
8. **REN2-08, REN2-13, REN2-15** (LOW) — three missing lockstep pins (VolumetricsParams size, water.vert GpuInstance, NORMAL_ALPHA_SPEC_BIT).
9. **REN2-12** (LOW) — TAA moving-pixel α floor.
10. Doc-rot pass: REN2-14, REN2-16, REN2-17 (note only), REN2-18, REN2-19, REN2-20 — fold into the open #1484 batch.

---

## Coverage Notes

- **Fully clean dimensions** (0 new findings at HEAD): 7 (Resource Lifecycle — both prior HIGH fixes verified), 19 (Bloom), 20 (Soft Shadows), 22 (NIFAL translation — 44171cd5 verified).
- **Prior-fix verification**: all four 2026-06-09 findings confirmed fixed (04acaa2b structural Drop ordering incl. unwind paths; 73a43fc8 feature-probe + clean None path; 2f7bcf78 correct though see residual REN2-12; 44171cd5 idempotent spawn-once write-back, tooling fidelity restored).
- **SPIR-V freshness**: every shader touched by the delta (and svgf/composite/taa) byte-matches a fresh `glslangValidator -V` recompile — the HIGH/MEDIUM shader findings are live in the shipped `.spv`, and the fixes require recompiles.
- **#1426 re-triage suggested**: `device_wait_idle` now runs unconditionally at Drop entry (`context/mod.rs:2656`), which appears to address the issue's core premise; the issue predates 04acaa2b.
- **Checklist drift noted by agents** (checklist stale, not code): Dim 13's "material flag from materials[material_id]" — the caustic-source bit is intentionally per-instance (#1098/#1111 deferred); Dim 19's "~10 barriers" — now 9 post-#1397; Dim 20's "sunAngularRadius at documented offset" — rides exclusively in `skyTint.w`; GLSL-PathTracer reference clone absent from `/mnt/data/src/reference/` (Dim 21 verified presets against the pinned prior-audit table instead).
- **Per-dimension raw reports**: generated under `/tmp/audit/renderer/dim_*.md` during the run (scratch, removed on cleanup).

---

*Next step: `/audit-publish docs/audits/AUDIT_RENDERER_2026-06-11.md`*
