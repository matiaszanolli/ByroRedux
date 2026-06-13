# Concurrency & Synchronization Audit — 2026-06-11

- **Scope**: `--focus 2,3,5` — Dimension 2 (Vulkan Synchronization), Dimension 3 (Resource Lifecycle), Dimension 5 (Compute → AS → Fragment Chains). Depth: deep.
- **Baseline**: `main` @ `1e8a25ab` (post camera-relative-rendering cascade, PR #1485).
- **Dedup pool**: `gh issue list` snapshot (35 OPEN issues) + targeted `gh issue view` checks for closed issues referenced by code comments.
- **Result**: **0 new findings.** All three dimensions verified clean against current code. Two seed open-questions and three seed open-items from the prior (aborted) attempt were resolved — all in favor of the existing code. One stale-premise triage note on an OPEN issue (#1387).

---

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 0 |
| LOW      | 0 |
| **Total**| **0** |

This is a verification (negative-result) audit: the renderer's sync and lifecycle
surface is at audit saturation in these dimensions — nearly every barrier,
fence, destroy path, and ping-pong slot in the audited files carries a
prior-finding annotation (#282, #418, #639, #654, #908–#911, #917/#918, #931,
#952, #962/#964, #983, #1003, #1031, #1105, #1138, #1195/#1196, #1211, #1227,
#1255, #1297/#1298, #1436…), and re-derivation from the Vulkan spec confirmed
each one still holds on current `main`.

---

## Resolved open questions (from the seed / failed prior attempt)

### Q1 (Dim 2) — COMPUTE→AS_BUILD dst_access: `ACCELERATION_STRUCTURE_READ_KHR` vs build-input read
**Resolved: tracked and closed; not a regression; no action.**
The barrier at `crates/renderer/src/vulkan/context/draw.rs:1274-1281` uses
`dst = ACCELERATION_STRUCTURE_BUILD_KHR / ACCELERATION_STRUCTURE_READ_KHR` for the
skinned-vertex-buffer → BLAS-build-input handoff. The sync1-limitation comment at
`draw.rs:954-962` documents the choice; it was adjudicated in **#1436 (VKC-007, CLOSED)**
and **#661 (SY-4, CLOSED)**: the sync1 form is aliased on every shipping driver, and the
switch to the more-specific access flag is pinned to the future sync2
(`cmd_pipeline_barrier2`) migration. The execution dependency (COMPUTE →
AS_BUILD stage) is unconditionally correct; only the access-mask pedantry is
deferred. Per the speculative-Vulkan-fix policy, no change proposed.

### Q2 (Dim 3) — Do composite/SVGF/TAA recreate paths handle a swapchain image-count change?
**Resolved: yes.**
- Per-FIF resources (SVGF/TAA history, G-buffer attachments, bloom mips, caustic
  accumulators, volumetrics froxel volumes, composite HDR images) are sized by
  `MAX_FRAMES_IN_FLIGHT`, not image count (`composite.rs:244-249` debug_asserts) —
  count-independent by construction.
- Per-swapchain-image resources are rebuilt from the fresh
  `swapchain_state.image_views`: composite framebuffers (`composite.rs:918-927` destroy,
  `:1071-1084` rebuild per view), egui framebuffers (`context/resize.rs:582-588`),
  texture-registry descriptor sets (`resize.rs:253-254`), and
  `FrameSync::recreate_for_swapchain` (`sync.rs:198-238`) which re-sizes
  `images_in_flight` and recreates one `render_finished` semaphore per image.

### Q3 (Dim 3) — Do SVGF/TAA `destroy()` free all per-FIF history images?
**Resolved: yes, completely.**
`svgf.rs:1114-1166` drains both `indirect_history` and `moments_history`
(view + image + allocation each) plus all pipeline objects; `taa.rs:901-941`
drains the `history` Vec the same way. The resize paths self-destroy old slots
before reallocating (`svgf.rs:1024-1046`, `gbuffer.rs:391-409`) per the #1031
self-contained-recreate pattern, and `bloom.rs:631-640` drains `down_mips` +
`up_mips` for every frame slot.

### Q4 (Dim 5) — Raster-path consumption of the skinned SSBO (relation to #1387)
**Resolved: not consumed; chain correct as-is.**
`triangle.vert:82-160` skins **inline** from the bone-palette SSBO (set 1
binding 3 `bones[]` + binding 12 `bones_prev[]`); it never reads the skin
compute output buffer. That consumption is exactly what the palette barrier at
`draw.rs:917-934` (COMPUTE→COMPUTE|VERTEX) covers. The skin output buffer
(`skin_compute.rs:420-426`) carries `STORAGE_BUFFER | SHADER_DEVICE_ADDRESS |
ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR` and is consumed only by BLAS
build/refit — covered by the COMPUTE→AS_BUILD barrier. No COMPUTE→VERTEX_INPUT
barrier is needed until the deferred raster-from-SSBO path (M29.3 Phase 3) ships.

### Q5 (Dim 5) — Camera-relative `render_origin` (commits 36f66493 / bccf06f0) barrier coverage
**Resolved: fully covered.**
`render_origin` rides the per-frame camera UBO (`upload_camera`, `draw.rs:791`)
under the bulk HOST→VERTEX|FRAGMENT|COMPUTE|DRAW_INDIRECT barrier
(`draw.rs:2223-2234`); cluster-cull's earlier HOST→COMPUTE (`draw.rs:1601-1608`)
covers its camera read; volumetrics carries `render_origin` in its own UBO behind
its own HOST→COMPUTE (`volumetrics.rs:820-830`); SSAO receives the
camera-relative position as a dispatch argument. `patch_camera_rt_flag`
(`draw.rs:1572-1580`) is a record-time host-coherent write covered by the
implicit `vkQueueSubmit` host-visibility guarantee. No new un-slotted
host-written buffer was introduced by the camera-relative work.

---

## Dimension 2 — Vulkan Synchronization (verified clean)

| Chain | Verdict | Evidence |
|---|---|---|
| FIF fence wait before cmd reuse | OK | `draw.rs:289-301` waits BOTH slots (#282); cmd reset at `:414-426` after wait; `reset_fences` immediately before `queue_submit` (`:3267-3284`, #952); submit-failure arm recreates semaphore + fence (`:3297-3322`, #910/#952). |
| Acquire → render → present semaphores | OK | `image_available[frame]` waited at COLOR_ATTACHMENT_OUTPUT; `render_finished` is per-swapchain-image (`:3240-3252`, post-#906); present waits the same per-image semaphore; all fallible calls between acquire and submit recover the pending signal (#910). |
| bone_world staging → palette compute | OK | `scene_buffer/upload.rs:213-253` — copy + TRANSFER_WRITE→SHADER_READ, TRANSFER→COMPUTE. |
| Palette → skin/raster consumers | OK | `draw.rs:917-934` — COMPUTE→COMPUTE\|VERTEX, SHADER_WRITE→SHADER_READ. |
| Skin compute → BLAS build/refit | OK | `draw.rs:1274-1281`; sync1 access alias documented (`:954-962`), closed #1436/#661 (see Q1). |
| First-sight BLAS BUILD batch | OK | On-cmd batched builder (#911) primed by the same-frame dispatch; scratch-serialise AS_WRITE→AS_WRITE self-emitted per refit (`blas_skinned.rs`, #983) covers BUILD→refit and refit→refit (#642/#644). |
| Refit → TLAS → ray-query consumers | OK | `draw.rs:1424-1431` AS_BUILD→AS_BUILD; `:1541-1549` AS_BUILD→FRAGMENT\|COMPUTE (#415 widening). |
| Cluster cull | OK | HOST→COMPUTE (`:1601-1608`) + COMPUTE→FRAGMENT (`:1619-1626`). |
| Bulk host-write barrier | OK | `draw.rs:2223-2234` (#909/#961 fold) covers camera/light/instance/material/composite/SVGF/TAA/bloom UBO+SSBO host writes. |
| SVGF dispatch | OK | `svgf.rs:871-970` — WAR pre-barrier, COMPUTE→FRAGMENT\|COMPUTE publish (#653); prev-slot ping-pong + MFIF≥2 static assert (#918); submit-gated history counters (#917/#964). |
| TAA dispatch | OK | `taa.rs` mirrors SVGF (same asserts, same barrier shape). |
| Caustic CLEAR→COMPUTE→FRAGMENT | OK | `caustic.rs:774-885` — WAR→TRANSFER, clear, TRANSFER→COMPUTE, COMPUTE→FRAGMENT; TLAS-gated dispatch (#640). |
| Volumetrics inject→integrate→composite | OK | `volumetrics.rs:795-945` — HOST→COMPUTE, pre-inject WAR, COMPUTE→COMPUTE, COMPUTE→FRAGMENT; `tlas_written` latch + debug_assert (#1105). Dormant behind `VOLUMETRIC_OUTPUT_CONSUMED == false` (#928). |
| Bloom per-mip RAW chain | OK | `bloom.rs:504-608` — #931 post-barrier accounting; final up-mip publishes to FRAGMENT. |
| Water-caustic accumulator | OK | Clear pre-render-pass (`draw.rs:2246`); `barrier_post_render_pass` (`:2846-2848`) sequences `imageAtomicAdd` → composite read (#1255). |
| Descriptor update timing | OK | Per-FIF sets written only for the just-fenced slot (bloom `bloom.rs:493-500`, caustic/volumetrics `write_tlas`); resize rebinds behind `device_wait_idle`. |
| Queue Mutex discipline | OK | Guards bound across `queue_submit` / `queue_present` / egui internal submit (`draw.rs:3187-3210`, `:3286-3323`; CONC-D2-NEW-01 pattern). |
| Post-submit TLAS shrink of other slot | OK | `draw.rs:3396-3429` — target slot fenced at frame start; just-submitted cmd references only `tlas[frame]`. |

## Dimension 3 — Resource Lifecycle (verified clean)

| Area | Verdict | Evidence |
|---|---|---|
| Reverse-order Drop teardown | OK | `context/mod.rs:2650-2893` — wait_idle → egui → screenshot → sync → pools → framebuffers → registries/passes → depth → pipelines → meshes → cache → render pass → swapchain → allocator → device → surface → instance. |
| Allocator freed last / leak guard | OK | `Arc::try_unwrap` at `:2846-2884`; #665 leak-don't-UAF early-return on outstanding refs (see existing #1426). Allocator only becomes `None` inside Drop → the allocator-Some gates earlier in Drop never skip mid-life; existing **#1483 stays LOW** (premise re-confirmed, not inflated). |
| BLAS/TLAS shutdown cleanup | OK | `acceleration/mod.rs:244-311` — pending-destroy drain (#639/#732), static BLAS, all TLAS slots (accel + buffer + both instance buffers), skinned BLAS drain (#1138), per-slot scratch + shared BLAS scratch. |
| SkinSlot lifetime | OK | Drop: slots destroyed before pipeline (descriptor-pool ordering, `:2695-2700`); runtime: LRU eviction after MFIF+1 idle frames (#643), despawn drain post-fence (#1003), capacity-stale recreate post-both-fence (#1297/#1298). |
| Swapchain recreate | OK | `resize.rs` — wait_idle first; #654/LIFE-M1 view-destroy ordering pinned by unit test; format-stable fast path (#576) keeps pipelines; nulled handles make partial-failure Drop safe; `draw_frame` empty-framebuffers guard (#1211). |
| Per-pass recreate cleanup | OK | SSAO/bloom destroy+new; G-buffer/SVGF/TAA self-contained destroy-then-recreate with internal layout walks (#1031); caustic/water-caustic fail-closed disable; composite framebuffers rebuilt per new view list; failure latches reset (#479). See Q2/Q3 above for image-count + completeness. |
| scene_buffer cleanup | OK | `scene_buffer/descriptors.rs:156-196+` — all per-FIF buffer Vecs drained including the R1 **MaterialBuffer SSBO** (`:186-189`), bone-world/staging/bind-inverse pairs, DALC, indirect. |
| Registries | OK | Texture/mesh registry destroys drain their deferred queues; per-frame `tick_deferred_destroy` runs post-fence (`draw.rs:399-408`, #418). |
| EguiPass teardown | OK | Taken + destroyed first in Drop; framebuffers recreated on resize. pending_free flush gap = existing OPEN #1427 (LOW). |

## Dimension 5 — Compute → AS → Fragment Chains (verified clean)

| Invariant | Verdict | Evidence |
|---|---|---|
| Full palette→skin→refit→TLAS→ray-query chain | OK | See Dim 2 rows; pose-dirty skip gates (#1195/#1196) share one `pose_dirty` set so dispatch/refit decisions cannot diverge; first-sight slots dispatch unconditionally (`has_populated_output`). |
| SVGF/TAA read prev slot, write current | OK | `prev = (f+1) % MFIF` descriptor wiring (`svgf.rs:667-707`, `taa.rs:522-562`); MFIF≥2 compile-time asserts (#918); both-slots fence wait retires the read target before recording. |
| Caustic cleared before splat, splat before composite | OK | `caustic.rs:774-885`, same-frame chain. |
| Volumetrics ordering + `tlas_written` latch | OK | `write_tlas` immediately precedes `dispatch` (`draw.rs:2981/3035`), same `tlas_handle(frame)` gate; debug_assert + per-dispatch latch reset (#1105). |
| Bloom RAW pyramid + final publish | OK | #931 accounting verified; no missing publish on the last up-mip (dst FRAGMENT at `bloom.rs:594-598`). |
| MaterialBuffer HOST_WRITE → VS/FS read | OK | `upload_materials` (`draw.rs:1921`) recorded before the bulk barrier (`:2223`); upload has not moved into a compute path. |
| Raster consumption of skin output buffer | OK | Not consumed (see Q4); inline vertex-shader skinning is the live raster path. |

---

## Findings

**None.** No NEW, no Regression findings in dimensions 2, 3, 5.

### Triage note (not a finding)

- **#1387 premise is stale**: the issue title says the skin output buffer's
  deliberately-omitted `VERTEX_BUFFER` usage flag has "no tracking comment at
  creation site" — `skin_compute.rs:413-415` now carries exactly that comment.
  Recommend closing #1387 or re-scoping it to the actual deferred Phase 3
  raster-from-SSBO work (which will need both the usage flag and a
  COMPUTE→VERTEX_INPUT barrier when it ships).

### Existing-issue overlaps observed and skipped (dedup)

| Issue | State | Relation |
|---|---|---|
| #1436 / #661 | CLOSED | sync1 AS-build-input access alias — fix (doc comment + sync2 pin) in place, not regressed. |
| #1387 | OPEN | Skin output buffer usage flag — premise partially stale (see triage note). |
| #1481 | OPEN | SVGF firefly clamp 1-frame gap — shader-side, unchanged. |
| #1438 | OPEN | Ray budget atomicAdd — shader-side, out of barrier scope. |
| #1404 | OPEN | R32_UINT storage-image atomic format-feature query. |
| #1433 | OPEN | egui render pass external subpass dependency. |
| #1427 | OPEN | EguiPass pending_free flush before Renderer drop. |
| #1426 | OPEN | Allocator-leak early-return skips device_wait_idle. |
| #1483 | OPEN | GPU-timer pools inside allocator-Some Drop gate — re-confirmed LOW (gate unreachable-false today). |

---

## Method notes

- Per the speculative-Vulkan-fix policy, no barrier/stage-mask changes are
  proposed anywhere in this report; the only access-mask pedantry found (Q1) is
  already adjudicated and pinned to the sync2 migration.
- Dimension scratch files: `/tmp/audit/concurrency/dim_2.md`, `dim_3.md`, `dim_5.md`.

Next step (optional — nothing to publish): `/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-06-11.md` would be a no-op; consider instead triaging #1387 per the note above.
