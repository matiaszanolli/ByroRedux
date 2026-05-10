---
issue: 0
title: REN-LOW-BUNDLE-2026-05-09: 33 LOW + 5 INFO findings from renderer audit
labels: renderer, low
---

Bundle issue for the LOW + INFO findings from `docs/audits/AUDIT_RENDERER_2026-05-09.md`. None individually rise to user-visible bugs; tracked together for opportunistic cleanup.

**Excluded from this bundle**:
- REN-D6-NEW-03 (`MAX_LIGHTS_PER_CLUSTER = 32` not pinned to Rust const) — fold into existing #636 (FNV-D4-02).
- REN-D15-NEW-06 (Sun south tilt z = -0.15 actually north) — DUPLICATE-OF #802 (SUN-N2).

---

## GPU Memory (Dim 2) — 2 LOW + 2 INFO

- **REN-D2-NEW-01** (LOW) — `gbuffer::Attachment` and `svgf::HistorySlot` lack `GpuBuffer`-style Drop safety net (gbuffer.rs:51-154, svgf.rs:133-137). Not a current leak.
- **REN-D2-NEW-02** (LOW) — TLAS instance-buffer 8192 floor wastes ~1 MB BAR on cells with <100 instances (acceleration.rs:1944-1961).
- **REN-D2-NEW-03** (INFO) — `NON_COHERENT_ATOM_SIZE = 256` is the worst-case fallback (buffer.rs:367); could be replaced with the queried device value.
- **REN-D2-NEW-04** (INFO) — TLAS resize destroys + recreates synchronously without defensive `device_wait_idle` (acceleration.rs:1924-1935).

## Command Recording (Dim 5) — 1 LOW

- **REN-D5-NEW-04** (LOW) — UI-overlay defensive viewport/scissor re-set at draw.rs:1573-1586 is redundant.

## Shader Correctness (Dim 6) — 2 LOW

- **REN-D6-NEW-01** (LOW) — `triangle.frag` `getHitUV` uses raw `* 25` literals in 3 RT hit-fetch sites; should be a named const.
- **REN-D6-NEW-02** (LOW) — `composite.frag:362` `vol.rgb * 0.0` keep-alive comment doesn't reference an issue number for the M-LIGHT re-enable.

## Resource Lifecycle (Dim 7) — 6 LOW

- **REN-D7-NEW-01** (LOW) — `pipeline_ui` shares the raster `pipeline_layout`; ordering brittle.
- **REN-D7-NEW-02** (LOW) — `pipeline_cache` saved to disk AFTER subsystem `destroy()` calls.
- **REN-D7-NEW-03** (LOW) — `failed_skin_slots: HashSet<EntityId>` not cleared in Drop / no contract.
- **REN-D7-NEW-05** (LOW) — Drop calls `accel_manager.destroy()` without first calling `tick_deferred_destroy`.
- **REN-D7-NEW-06** (LOW) — `terrain_tile_scratch` cluster has no comment block tying the parallel scratch Vecs together.
- **REN-D7-NEW-08** (LOW) — `recreate_swapchain` SSAO destroy + new keeps the same `pipeline_cache` handle (cosmetic).

## Acceleration Structures (Dim 8) — 11 LOW + 1 INFO

- **REN-D8-NEW-01** (LOW) — `geometry.flags = OPAQUE` on `INSTANCES` is spec-meaningless.
- **REN-D8-NEW-02** (LOW) — Host→device copy relies on `write_mapped`'s implicit flush; document or assert.
- **REN-D8-NEW-03** (LOW) — Empty-TLAS `mem::take` / restore is harmless; documented.
- **REN-D8-NEW-06** (LOW) — Single-shot `build_blas` sets `ALLOW_COMPACTION` but never compacts; flag wasted.
- **REN-D8-NEW-07** (LOW) — All TLAS instances use mask `0xFF`; per-light-type mask buckets unused.
- **REN-D8-NEW-08** (LOW) — Skinned BLAS uses `PREFER_FAST_BUILD`; post-#679 600-frame rebuild threshold the math now favors `PREFER_FAST_TRACE`.
- **REN-D8-NEW-09** (LOW) — One-frame capacity-amortisation lag across cell-unload → cell-load.
- **REN-D8-NEW-10** (LOW) — TLAS `padded_count` query over-allocates; documented trade-off.
- **REN-D8-NEW-11** (LOW) — Column-major-`[f32;16]` → `VkTransformMatrixKHR` 3×4 conversion hand-unrolled; no unit test.
- **REN-D8-NEW-12** (LOW) — `frame_counter` shared across TLAS slots; cosmetic.
- **REN-D8-NEW-13** (INFO) — Empty-then-non-empty frame BUILDs twice; verified correct.
- **REN-D8-NEW-14** (LOW) — `missing_blas` warn doesn't identify offending draws.

## RT Ray Queries (Dim 9) — 1 LOW

- **REN-D9-NEW-04** (LOW) — Directional shadow jitter no longer matches "physical sun" comment (radius widened 4× in M-LIGHT v1); also `T·dx + B·dy` is a 2D disk on tangent plane, not a spherical cap.

## Denoiser & Composite (Dim 10) — 3 LOW

- **REN-D10-NEW-06** (LOW) — `screen_to_world_dir` uses `world.xyz / world.w` without guarding `w == 0`.
- **REN-D10-NEW-07** (LOW) — `compute_sky` reads `params.sun_dir.xyz` and re-normalizes despite host promising it's already normalized.
- **REN-D10-NEW-08** (LOW) — `composite.rs::recreate_on_resize` parameter list lost track of `volumetric_views` and `bloom_views` (signature parity with init).

## TAA (Dim 11) — 2 LOW + 1 INFO

- **REN-D11-NEW-03** (LOW) — Motion vector point-sampled with no 5-tap dilation — silhouette ghosting.
- **REN-D11-NEW-04** (LOW) — TAA descriptor binds `prev_mid` to the OTHER FIF slot; on session frame 0 that slot is UNDEFINED (currently safe via first_frame guard).
- **REN-D11-NEW-05** (INFO) — TAA pre-barrier `src_stage_mask` includes `FRAGMENT_SHADER` — over-spec.

## GPU Skinning (Dim 12) — 2 LOW

- **REN-D12-NEW-02** (LOW) — `bone_palette_overflow_tests` doc-comments cite pre-bump constants (`MAX_TOTAL_BONES = 4096` / 32-mesh ceiling); actual is 32768 / 256 meshes.
- **REN-D12-NEW-05** (LOW) — Inline-skinning + compute pre-skin coexist on same mesh today (Phase 2 RT-side only). Reserve `GpuInstance.flags::PRESKINNED_FLAG` bit now as guarded no-op for Phase 3.

## Caustics (Dim 13) — 6 LOW

- **REN-D13-NEW-02** through **REN-D13-NEW-07** — dead-code 32767 ceiling check after 15-bit mask; first-frame pre-clear barrier docstring drift; single-eta single-bounce undocumented assumption; hard-coded `tintLum 0.05` floor; LOD-0 literal pinned to 1-mip image; hard radius cliff for point/spot + no directional cosine.

## Material Table (Dim 14) — 3 LOW + 1 INFO

- **REN-D14-NEW-01** (LOW) — Stale `272 B` / `17 vec4` doc references in three sites (post-#804 layout is 260 B / 16 vec4).
- **REN-D14-NEW-02** (LOW) — No build-time grep guard for `GpuMaterial` GLSL field names; offset pin (#806) catches byte-position drift but not name renames.
- **REN-D14-NEW-03** (LOW) — `materials_unique` telemetry off-by-one from seeded slot is unflagged.
- **REN-D14-NEW-04** (INFO) — Seeded neutral re-uploads on the very first frame.

## Sky/Weather (Dim 15) — 3 LOW

- **REN-D15-NEW-04** (LOW) — `traceReflection` miss + glass-refraction miss return `fog * 0.5 + ambient * 0.5` — `fog` UBO is "unfogged HDR" after REN-D15-NEW-02 (#924) lands.
- **REN-D15-NEW-05** (LOW) — `weather_system` duplicates 22-line keys walker in transition branch; hoist to `pick_tod_pair` helper.
- **REN-D15-NEW-07** (LOW) — `transition_done` swap leaves `WeatherTransitionRes` resident with `duration_secs = INFINITY`; relies on float arithmetic as state machine.

---

## Suggested handling

These are opportunistic cleanup items. As individual files are touched for higher-priority work (HIGH/MEDIUM bundle, M-LIGHT future tier, M55 Phase 4, etc.), pull the relevant LOW into the same PR. Don't carve out a dedicated LOW-bundle PR — it dilutes review attention.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
