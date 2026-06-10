# Renderer Audit — 2026-06-09

**Scope**: Full Vulkan renderer, all 23 dimensions, `--depth deep`.
**Baseline**: HEAD `67e1baaf`. Dedup against 29 open GitHub issues + `docs/audits/` (most recent prior full renderer audit `AUDIT_RENDERER_2026-06-02.md`; focused `DIM17`/`DIM18` on 2026-06-04).
**Method**: One Task agent per dimension (renderer-specialist), each re-reading the live code path and adversarially disproving its own findings. The three highest-impact findings (D7, D22, D23) were independently re-verified against source by the orchestrator before inclusion; D7 severity was recalibrated (see its entry).

---

## Executive Summary

The renderer is in **strong shape**. Across 23 dimensions and ~140 checklist items, the audit found **0 CRITICAL**, **2 HIGH**, **2 MEDIUM**, and **8 LOW** new issues. The overwhelming majority of checklist items verified OK and trace to prior fixes with inline VUID rationale, `const_assert`/`debug_assert` pins, and regression tests. Several dimensions (1 Sync, 2 Memory, 3 Pipeline, 5 Command Recording, 6 Shaders, 8 Acceleration Structures, 12 Skinning, 13 Caustics, 14 Material Table, 15 Sky/Weather, 17 Water, 18 Volumetrics, 19 Bloom, 20 Soft Shadows, 21 Disney BSDF) came back **fully clean** (new findings = 0).

The two HIGH findings are both **abnormal-path** correctness issues, not steady-state bugs:
- **REN-D7-NEW-01** — `App` field declaration order drops `VulkanContext` before the ECS `World`, re-arming the #1406 allocator-teardown hazard on any **panic unwind** (the #1406 fix only patched the normal `CloseRequested` exit).
- **REN-D23-NEW-01** — GPU timestamp timers call host-side `vkResetQueryPool` without the `hostQueryReset` device feature on a **timestamp-capable, RT-less GPU** (the feature is gated on `ray_query_supported`), a per-frame Vulkan spec violation.

Neither manifests on the project's stated target hardware (RT-mandatory, RTX 4070 Ti) in normal operation — but both are reachable and cheap to fix structurally.

| Severity | Count | IDs |
|----------|-------|-----|
| CRITICAL | 0 | — |
| HIGH | 2 | REN-D7-NEW-01, REN-D23-NEW-01 |
| MEDIUM | 2 | REN-D11-NEW-01, REN-D22-NEW-01 |
| LOW | 8 | REN-D4-NEW-01, REN-D4-NEW-02, REN-D9-NEW-08, REN-D10-NEW-01, REN-D16-NEW-01, REN-D23-NEW-02, REN-D23-DOC-01, REN-D8-NOTE-01 |

**Pipeline areas affected**: Resource lifecycle (abnormal teardown), GPU telemetry (non-RT devices), TAA (parked-camera ghosting), NIFAL material translation (one render-time roughness leak). All else clean.

**Known-OPEN issues confirmed still present and NOT re-reported**: #1438 (IOR ray-budget atomicAdd), #1433/#1427 (egui RP/destroy), #1426 (allocator Arc leak / wait_idle), #1404 (R32_UINT atomic format unqueried), #1387 (skin buffer VERTEX_BUFFER), #1384 (three bitfields = 128u), #1369 (WRS reservoir occupancy), #1357 (BGSM_* aliases).

---

## RT Pipeline Assessment

**BLAS/TLAS (Dim 8): clean.** All four geometry-build sites use `R32G32B32_SFLOAT` @ stride-0 / `UINT32` indices / `OPAQUE` / `PREFER_FAST_TRACE` (skinned deliberately `PREFER_FAST_BUILD`, a measured ~18% FNV-Prospector bench bisect, not a violation). TLAS UPDATE never runs with a mismatched live count (double-guarded VUID-03708 + `decide_use_update`); padding sizes the buffer only — `primitiveCount` always equals the real instance count. `instance_custom_index` = compacted SSBO index via the shared `instance_map` (#419); `TRIANGLE_FACING_CULL_DISABLE` correctly gated on `two_sided` (#416), not blanket. Closed fixes #1226/#1227/#1144/#1145/#907/#960/#1300 all still in place and test-pinned.

**Ray queries (Dim 9): clean** (one LOW doc-rot). All 6 `rayQueryEXT` sites use `TerminateOnFirstHit | Opaque`; every site uses `tMin 0.05` matched to its normal-bias push-off plus self-instance rejection — no self-intersection. Frisvad `buildOrthoBasis` confirmed at the refraction roughness-spread, GI hemisphere, and shadow-jitter sites (the legacy degenerate `cross(N, up)` path is gone). The #789 glass-passthru identity check is now keyed on `materialKind == GLASS` (the texture-equality bug is fixed). `GLASS_RAY_BUDGET` is `1048576` (raised in `shader_constants_data.rs`); the in-shader comment still says `8192` (folded into the doc-rot consolidation below).

**Denoiser/SVGF + Composite (Dim 10): clean** (one LOW). Motion-vector convention is consistent across `triangle.vert` → `triangle.frag` (`(currNDC - prevNDC)*0.5`) → `svgf_temporal.comp` (`prevUV = uv - motion`). Ping-pong read-prev/write-current, bit-31 mesh-ID masking + normal-cone (`dot < 0.9`) disocclusion, alpha clamp + first-frame reset, ceiling-division dispatch, ACES-after-reassembly, fog-on-direct-only, SSAO-on-indirect-only, and R32_UINT caustic decode (`usampler2D ÷ 65536.0`, added as a direct-light term) all verified. Shipped `composite.frag.spv` / `svgf_temporal.comp.spv` byte-match a fresh recompile.

**Sync (Dim 1): clean.** `render_finished` correctly per-swapchain-image (548c1b69, reverts the 913f8047 per-frame regression that tripped VUID-00067). TLAS-build→fragment, SVGF compute-write→fragment-read, G-buffer→compute, and composite→SVGF barriers all carry correct stage/access scopes. The skinning COMPUTE→AS-BUILD→FRAGMENT chain (Dim 12) and volumetrics within-frame WAR barriers (Dim 18) are likewise correct.

---

## Rasterization Assessment

**Pipeline state (Dim 3): clean.** All 9 vertex attributes match `triangle.vert` by format/location/offset (stride 100 B pinned by `offset_of!` tests); `GpuInstance` is byte-identical (112 B) between Rust and all 5 shaders; every graphics pipeline supplies 7 blend states + `.subpass(0)`; composite/SSAO/cluster-cull descriptor layouts are guarded at init by SPIR-V reflection (`validate_set_layout`, #427) which panics on drift.

**Render pass & G-buffer (Dim 4): clean** (two LOW doc-rot). All 7 color attachments CLEAR+STORE, depth CLEAR+STORE with SAMPLED for SSAO; layouts UNDEFINED→COLOR_ATTACHMENT→SHADER_READ; both subpass deps cover the full color+early/late-depth masks gating FRAGMENT|COMPUTE reads; all 6 G-buffer formats match `triangle.frag` outputs exactly. The mesh-ID overrun guard is confirmed a one-shot `log::error!` + clamp (not a `debug_assert!`) — #992/#956 intact.

**Command recording (Dim 5): clean.** `draw_frame` records a correct, balanced order: begin → pre-RP transfers/skin → TLAS build (outside RP) + AS barrier → cluster cull → host uploads + blend-cache pre-pop → main RP (draws/water/UI) → SVGF/caustic/volumetrics/TAA/SSAO/bloom compute → composite (own RP) → egui → screenshot → submit → present. #1258/#1259/#1260/#1235 and the per-image `render_finished` all verified.

**Material table (Dim 14): clean.** `GpuMaterial` 300 B + all 65 per-field offsets asserted, matching the single GLSL declaration field-for-field through the #1248–#1250 Disney additions; all fields scalar, zero padding, byte-Hash dedup consistent; over-cap intern returns id 0 without poisoning the index; `ui.vert` correctly reads `textureIndex` not `materialId` (the recurring #785 trap held).

---

## Findings (grouped by severity)

### HIGH

#### REN-D7-NEW-01: `App` field order drops `VulkanContext` before `World` on panic-unwind → #1406 allocator-teardown hazard re-armed
- **Severity**: HIGH *(recalibrated from the dimension agent's CRITICAL — see Impact)*
- **Dimension**: Resource Lifecycle
- **Location**: `byroredux/src/main.rs:293-296` (struct field order), `:1884-1894` (normal-path #1406 fix), `crates/renderer/src/vulkan/context/mod.rs:2846-2884` (Drop leak branch)
- **Status**: NEW — #1406 / `299e6a84` fixed only the `WindowEvent::CloseRequested` arm; no issue covers the panic/early-exit path.
- **Description**: `App` declares `renderer: Option<VulkanContext>` (line 295) **before** `world: World` (line 296). Rust drops fields in declaration order, so any teardown that does **not** go through `CloseRequested` — a panic unwind anywhere inside `event_loop.run_app(&mut app)` (there are many `expect()`/`unwrap()` sites reachable from `draw_frame`/resize), or any future early return — drops `renderer` (firing `VulkanContext::Drop`) while `world` still holds the `AllocatorResource` `Arc` clone. There is no `impl Drop for App`, no `catch_unwind`, no panic hook. The #1406 fix is procedural (lives only in the `CloseRequested` handler), not structural.
- **Evidence**: On unwind, `VulkanContext::Drop` reaches the `Arc::try_unwrap(alloc_arc)` guard at `mod.rs:2846`, which returns `Err` (outstanding clone in `world`) and takes the **leak branch** (`:2849-2882`): logs an error, `debug_assert!(false, …)`, and returns without destroying device/instance/allocator. The normal path (`main.rs:1890-1892`) does `world.remove_resource::<AllocatorResource>()` then `renderer.take()` — correct, but only on `CloseRequested`.
- **Impact**: The `Arc::try_unwrap` guard **prevents a true use-after-free** (this is why I recalibrated from CRITICAL): in release it leaks device/instance/allocator handles (reclaimed at process exit) and the subsequent `AllocatorResource` drop runs `vkFreeMemory` on the still-valid leaked device — no UAF. The real harm is (a) **debug builds**: `debug_assert!(false)` fires *during* the unwind → panic-during-unwind → `process::abort()`, masking the original panic and crippling debuggability of every render-loop panic; (b) the trigger that #1406 classified CRITICAL is re-armed and would become a live UAF the moment the `try_unwrap` guard is weakened. Trivial structural fix.
- **Suggested Fix**: Add `impl Drop for App { fn drop(&mut self) { self.world.remove_resource::<AllocatorResource>(); self.renderer.take(); } }` so every exit path (normal, panic, early return) gets the #1406 ordering for free, independent of the `CloseRequested` arm.
- **Related**: #1406, #1426 (VKC-005, open), REN-D23-NEW-02 (sibling abnormal-teardown leak).

#### REN-D23-NEW-01: GPU timer host `vkResetQueryPool` used without the `hostQueryReset` feature on RT-absent devices
- **Severity**: HIGH (Vulkan spec violation → ≥ HIGH per severity rules)
- **Dimension**: Debug Overlay & GPU Telemetry
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:184-213` (`new`), `:298` (`read_and_reset`); feature gate at `crates/renderer/src/vulkan/device.rs:475`
- **Status**: NEW — distinct from the open egui issues #1433/#1427.
- **Description**: `GpuPerFrameTimers::new()` gates creation purely on `caps.timestamp_supported` (`gpu_timers.rs:188`). Both `new()` (`:211`) and per-frame `read_and_reset()` (`:298`) call host-side `device.reset_query_pool(...)`, which requires the `hostQueryReset` feature. But `device.rs:475` enables that feature as `.host_query_reset(caps.ray_query_supported)` — i.e. **only when ray queries are present**. RT is optional: `is_device_suitable` requires only the swapchain extension + `synchronization2`; `ray_query_supported` is an optional probe, never a rejection criterion. `timestamp_supported` (`timestampComputeAndGraphics == TRUE`) is independent of RT and is reported by many non-RT GPUs.
- **Evidence**: Confirmed by orchestrator: `device.rs:475` `.host_query_reset(caps.ray_query_supported)`; `gpu_timers.rs:188` gates on `timestamp_supported` alone. On a timestamp-capable, RT-less device the timers are created and immediately call `reset_query_pool` with the feature disabled → VUID-vkResetQueryPool-None-02665, at init and once per frame thereafter. The module doc (`gpu_timers.rs:64-70`) asserts the reverse ("timestamp support implies the RT gate") — that is backwards.
- **Impact**: Per-frame Vulkan spec violation / UB on any timestamp-capable GPU that lacks the RT extension set. Does **not** affect the project's stated RT-mandatory target hardware (where `ray_query_supported` is always true), but it is reachable because device selection does not require RT. Narrow real-world trigger, but a clear spec violation.
- **Suggested Fix**: Track the enabled `hostQueryReset` in `DeviceCapabilities` and gate `GpuPerFrameTimers::new()` on `timestamp_supported && host_query_reset_enabled`; **or** switch the resets to command-buffer `cmd_reset_query_pool` at the top of `draw_frame` (no host feature needed) — the module's own line-5 doc already claims `cmd_reset_query_pool` is used, so code and comment disagree; **or** enable `host_query_reset` unconditionally (widely supported, cheap). If the engine is truly RT-mandatory, consider also rejecting non-RT devices in `is_device_suitable`.
- **Related**: #1194.

### MEDIUM

#### REN-D11-NEW-01: Parked-camera luma-clamp skip ghosts moving actors
- **Severity**: MEDIUM (visual quality; no crash, no validation error)
- **Dimension**: TAA (M37.5)
- **Location**: `crates/renderer/shaders/taa.comp:164-237` (cameraStatic branch), `crates/renderer/src/vulkan/context/draw.rs:635-638` (`camera_static` derivation), `crates/renderer/src/vulkan/taa.rs:660-709` (`upload_params`, α = 1/(N+1))
- **Status**: NEW (predates none of the prior DIM11 sweeps — those predate the M37.5 `cameraStatic` luma-skip).
- **Description**: `camera_static` is derived **only** from the frame-to-frame view-proj matrix delta (`draw.rs:635`) and is blind to scene/object motion. A walking actor, swinging door, or animated mesh leaves the camera matrix unchanged → `camera_static = true`. That flag (a) drops α to `1/(N+1)` capped at 1/256, and (b) sets `params.z = 1.0`, which makes `taa.comp:225` **skip the luma (Y) variance clamp**, keeping only the chroma clamp. A moving actor's interior pixels have non-zero motion vectors but the same `mesh_id` at the reprojected pixel (it occupies both current and reprojected pixel), so `disocclusion == false`, the temporal tap is taken at ~99.6% history weight with the anti-ghost luma clamp disabled → luminance trails.
- **Impact**: Visible luminance smear/ghost trails on moving actors and animated geometry whenever the player stands still (dialogue, menus, AFK, scripted scenes — common). Worsens the longer the camera is parked (α → 1/256). Blast radius is the moving silhouette interior, not the whole frame.
- **Suggested Fix**: Gate the luma-clamp skip on per-pixel motion, not the global flag: treat a pixel as static only when `cameraStatic && dot(motion,motion) < epsilon` (motion is already dilated at `taa.comp:115-129`); otherwise fall through to the full `histYc = clamp(histYc, yMin, yMax)` path with normal α=0.1. ~3 lines, no host change; preserves glass/rough-metal convergence on truly static pixels.

#### REN-D22-NEW-01: Per-draw roughness re-classification in static-mesh render path overrides canonical `Material.roughness`
- **Severity**: MEDIUM (NIFAL contract drift + observability; output in-range, no GPU hazard)
- **Dimension**: NIFAL Material Translation
- **Location**: `byroredux/src/render/static_meshes.rs:390-407` (the `normal-alpha-as-spec` block); seeded at `:304`, consumed into `DrawCommand` at `:575`
- **Status**: NEW
- **Description**: The `mut roughness` local (seeded correctly from canonical `m.roughness` at `:304`) is **overwritten at render time** for any draw matching `material_kind < 100 && metalness < 0.3 && env_map_scale <= 0.3 && normal_map_index != 0 && gloss_map_index == 0`: `:402` `roughness = (1.0 - glossiness/100.0).clamp(0.05,0.95)` and `:404` `roughness = (0.85 - (specular_strength-1.0)*0.1).clamp(0.4,0.85)`. Both `glossiness` and `specular_strength` are canonical fields `resolve_pbr()` already consumed at the translate boundary. This is the render-time roughness heuristic NIFAL exists to eliminate — the mutated value flows `:575` → `DrawCommand::to_gpu_material()` → `GpuMaterial.roughness` → `MaterialTable` intern (Dim 14), so the value Dim 14 receives is **not** the canonical translate output for this population.
- **Nuance (verified by orchestrator)**: the gate genuinely depends on **render-side texture-resolution facts** — `normal_has_alpha` and the resolved `normal_map_index`/`gloss_map_index` (TextureRegistry handles, unavailable at NIF-import translate time). So the *condition* legitimately can't move to translate wholesale; only the **roughness scalar derivation** (which uses translate-available `glossiness`/`specular_strength`) can. The fix is not a naive "move the block to translate."
- **Impact**: For the gated population (the bulk of Skyrim/Oblivion architecture/clutter — lit, normal-mapped, no dedicated gloss map, `env_map_scale≈0`, `metalness<0.3`), the canonical resolved roughness is silently discarded and replaced by a render-time value; `material_dump`/`mat.*` tooling reports a roughness the GPU never uses for these meshes. No crash (clamped). Re-introduces a slice of the per-draw material work the NIFAL refactor claimed to delete. Prior audits' "no per-draw re-classify" claims are narrowly true (the *keyword* classifier is gone) but missed this arm.
- **Suggested Fix**: Resolve `normal_has_alpha` once when the normal map is attached (spawn / texture-load), write the derived roughness back into `Material.roughness`, and have `static_meshes.rs` only set the `NORMAL_ALPHA_SPEC_BIT` gloss-map flag (the per-pixel modulation is legitimately a shader concern) — never recompute the scalar per-frame.
- **Related**: #1357 (open), `feedback_format_translation`, `docs/engine/nifal.md`.

### LOW

#### REN-D4-NEW-01: Stale `debug_assert!` doc claim for the mesh-ID overrun guard
- **Severity**: LOW (doc-rot, actively misleading)
- **Dimension**: Render Pass & G-Buffer
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs:95-97`, `crates/renderer/src/vulkan/scene_buffer/constants.rs:104-105`
- **Status**: NEW
- **Description**: Both comments claim a `debug_assert!` enforces the `MAX_INSTANCES` contract, but that assert was deliberately removed under #992/#956 (it leaked the in-flight command buffer on unwind). The guard is now a one-shot `log::error!` + clamp. The stale comment invites a maintainer to "restore" the leak.
- **Suggested Fix**: Update both comments to describe the warn-once + clamp guard.

#### REN-D4-NEW-02: Stale "6 color attachments" comments + stale `triangle.frag:980` line reference
- **Severity**: LOW (doc-rot)
- **Dimension**: Render Pass & G-Buffer
- **Location**: render-pass/G-buffer comments (actual count is 7 color attachments); the mesh-ID write is at `triangle.frag:1531`, not `:980`.
- **Suggested Fix**: Correct the count to 7 and the line reference.

#### REN-D9-NEW-08: GI-bounce escape comment cites stale "3000u" while `tMax` is 6000u
- **Severity**: LOW (doc-rot)
- **Dimension**: RT Ray Queries
- **Location**: `crates/renderer/shaders/triangle.frag:3589-3591` (comment) vs `:3537` (`tMax = 6000.0`) and `:3483` (`giFade` ends at 6000).
- **Suggested Fix**: Change "within 3000u" to "within 6000u". *(Bundle with the `GLASS_RAY_BUDGET` stale comment at `triangle.frag:2305` — says 8192, actual 1048576 — in one doc-rot pass.)*

#### REN-D10-NEW-01: SVGF spatial firefly clamp scoped inside the `hasHistory` branch
- **Severity**: LOW (cosmetic, self-heals next frame)
- **Dimension**: Denoiser & Composite
- **Location**: `crates/renderer/shaders/svgf_temporal.comp:280-317`
- **Description**: The spatial firefly clamp lives inside `if (hasHistory)`, so a GI spike on a freshly-disoccluded pixel enters history un-clamped for one frame.
- **Suggested Fix**: Hoist the spatial clamp ahead of the history branch so it applies on the first (no-history) frame too.

#### REN-D16-NEW-01: `generated_header_contains_all_defines` pins only 4 of 13 `DBG_*` bit values
- **Severity**: LOW (test-coverage gap, diagnostic-only blast radius)
- **Dimension**: Tangent-Space & Normal Maps
- **Location**: `crates/renderer/src/shader_constants.rs:46-97` (value-pin test) vs `:208-233` (redeclaration test)
- **Description**: The redeclaration test lists all 13 `DBG_*` names, but the value-pin test asserts the emitted `#define` value for only 4 (`DBG_BYPASS_POM`, `DBG_VIZ_NORMALS`, `DBG_BYPASS_NORMAL_MAP`, `DBG_DISABLE_HALF_LAMBERT_FILL`). The other 9 — incl. the M-NORMALS-relevant `DBG_VIZ_TANGENT` (0x8), plus `DBG_VIZ_GLASS_PASSTHRU` (0x80), `DBG_BYPASS_VERTEX_COLOR` (0x400), `DBG_DISABLE_AO` (0x800), `DBG_LEGACY_LIGHT_ATTEN` (0x1000) — have no value assertion. A `build.rs` reorder/copy-paste typo would compile, pass both tests, and ship a wrong-valued diagnostic bit silently. (Note: the live catalog has grown past what the audit checklist documents — `0x400/0x800/0x1000` exist beyond the listed `0x200` ceiling; checklist is the stale party, not the code.)
- **Suggested Fix**: Extend `generated_header_contains_all_defines` to assert the emitted value of all 13 `DBG_*` bits.

#### REN-D23-NEW-02: GPU timer query pools leak when allocator teardown takes the `None` (early-return) path
- **Severity**: LOW (process-exit-only leak)
- **Dimension**: Debug Overlay & GPU Telemetry
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:2684,2742-2744` (Drop)
- **Description**: `gpu_timers.destroy()` is nested inside `if let Some(ref alloc) = self.allocator`. The timer pools need no allocator, but if `self.allocator` is `None` at Drop (the #1426 early-return scenario), the `Some(timers)` branch is never reached and `MAX_FRAMES_IN_FLIGHT` `VkQueryPool`s leak (validation "destroyed device with live objects").
- **Suggested Fix**: Move the timer-destroy out of the allocator guard, alongside `egui_pass.destroy()` at the top of Drop.
- **Related**: REN-D7-NEW-01, #1426 (both are abnormal-teardown hardening).

#### REN-D23-DOC-01: `gpu_timers.rs` header doc-table stale (query/bracket counts)
- **Severity**: LOW (doc-rot)
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:5,77-78,158-159,233,239`
- **Description**: `QUERIES_PER_FRAME = 24` with 12 bracket pairs / 12 active bits, but prose still says "14 queries", "twelve brackets … currently 7", "all three ms fields", "6-query pool" — drift across the Phase-6/7 expansions. No runtime effect.
- **Suggested Fix**: Update to 24 queries / 12 brackets / 12 active bits.

#### REN-D8-NOTE-01: `missing_blas` three-way split surfaced via warn-line only, not bench-stats
- **Severity**: LOW (checklist over-specification, **not a code defect**)
- **Dimension**: Acceleration Structures
- **Description**: The #1228 three-way `missing_blas` counters (skinned/rigid/ssbo_evicted) are surfaced via the rate-limited `log::warn!` line only, not a `bench-stats` telemetry getter. `git show 289fb07a` confirms the closed issue's scope was warn-line-only; the audit checklist's "surfaced via bench-stats" over-specifies. Recorded for completeness; no action required unless telemetry exposure is independently desired.

---

## Prioritized Fix Order

**Correctness / safety (do first):**
1. **REN-D7-NEW-01** (HIGH) — add `impl Drop for App` with `world.remove_resource::<AllocatorResource>()` then `renderer.take()`. Trivial, structural, kills the panic-unwind hazard for good. *Pairs with #1426 and REN-D23-NEW-02 — consider one "abnormal-teardown hardening" change.*
2. **REN-D23-NEW-01** (HIGH) — fix the `hostQueryReset` gate (track the enabled feature and gate timer creation on it, or move resets to `cmd_reset_query_pool`, or enable the feature unconditionally). Resolves a per-frame spec violation on non-RT devices.

**Visual / contract correctness:**
3. **REN-D11-NEW-01** (MEDIUM) — per-pixel-motion gate on the TAA luma-clamp skip (~3 lines). High user-visible value (ghosting while standing still).
4. **REN-D22-NEW-01** (MEDIUM) — resolve normal-alpha-as-spec roughness once at spawn/texture-attach, write back into `Material.roughness`; leave only the `NORMAL_ALPHA_SPEC_BIT` flag in `static_meshes.rs`. Restores the NIFAL single-resolve contract + tooling fidelity.

**Hardening / cleanup:**
5. **REN-D23-NEW-02** (LOW) — hoist timer destroy out of the allocator Drop guard.
6. **REN-D10-NEW-01** (LOW) — hoist the SVGF spatial firefly clamp ahead of the history branch.
7. **REN-D16-NEW-01** (LOW) — extend the DBG value-pin test to all 13 bits.

**Doc-rot consolidation (one pass):** REN-D4-NEW-01, REN-D4-NEW-02, REN-D9-NEW-08 (+ the `GLASS_RAY_BUDGET` 8192→1048576 comment), REN-D23-DOC-01, and the audit-checklist updates implied by REN-D8-NOTE-01 and the DBG catalog growth.

---

## Coverage Notes

- **Fully-clean dimensions** (0 new findings, all checklist items verified-OK against live source): 1, 2, 3, 5, 6, 8 (1 over-spec note), 12, 13, 14, 15, 17, 18, 19, 20, 21.
- **Soft-shadow reconciliation (Dim 20)**: Dim 9's "~1.15° via `skyTint.w`" is confirmed correct, not an inconsistency — there is no standalone `sunAngularRadius` UBO field; the angular radius rides exclusively in `skyTint.w` (`env_translate.rs:199` const `0.020` rad = 1.146° → `sky.rs` → `draw.rs:735` → `triangle.frag:3406`). Single source of truth, no drift.
- **Water (Dim 17)**: the prior HIGH (water-caustic sun-direction sign inversion) is verified **fixed** since 2026-06-04 across all three `water.frag` sign sites + `caustic_splat.comp`; #1210 Phases A–E all live.
- **Checklist drift discovered**: several skill-checklist line numbers / values are stale relative to HEAD (`GLASS_RAY_BUDGET` 8192→1048576, the DBG catalog beyond `0x200`, the `gpu_material_size_is_260_bytes` test-name reference, Dim 21's "MAT_FLAG_* bits 5-9 not in shader_constants_data.rs" — they migrated in #1285). These are the *checklist* being stale, not the code; worth a checklist refresh.
