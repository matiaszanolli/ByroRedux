# Batch 5212-5221 (renderer-audit LOWs, fetched 2026-10-04, ALL 10 RESOLVED in 8 commits a355d3673..d5e6add93)

## #5212 — REN-D7-2026-10-03-02: the scene-static signal's coverage of renderer-appended combustion lights rests on an unguarded variable shadow in `draw_frame`
State: OPEN | Labels: bug, renderer, low, test-gap

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` — `draw_frame`, `let lights = frame_lights.as_slice();` immediately before `self.build_and_upload_instances(…, lights, …)`; consumer `build_and_upload_instances.rs` (`caustic_scene_key` light-rig fold → `caustic_scene_static` → `next_svgf_temporal_alpha` and `scene_static_last_build`)
- **Status**: NEW (test gap)
- **Description**: `next_svgf_temporal_alpha`'s `caustic_history_valid` input, and ReSTIR's parked mode 2 through `scene_static_last_build`, see light changes only through the `caustic_scene_key` fold over the `lights` slice handed to `build_and_upload_instances`. That slice is correct today: `draw_frame` shadows the app's `FrameInputs.lights` with `frame_lights`, the merged list after `append_combustion_surface_lights` and the #5055 priority re-sort. So advected/cooling combustion surface lights do break parked accumulation, as they should. Nothing pins this, though. `svgf_temporal_alpha_is_fed_the_combined_camera_and_light_rig_signal` and `scene_static_signal_sees_rigid_instance_set_changes` scan only `build_and_upload_instances.rs`, and no test references `frame_lights` outside its producer. Passing the un-shadowed app slice (an easy slip: #5055 just threaded a parallel `light_ids` through the same call chain) would compile and pass every test. It would also silently drop renderer-derived fire lights from the key, so SVGF would keep its ~1/256 parked α and ReSTIR mode 2 over GI and direct lighting that a burning field keeps changing.
- **Evidence**: `grep -rn "frame_lights" crates/renderer/src` hits only `draw.rs`, `assemble_camera_and_lights.rs`, `mod.rs`, `init.rs`, `telemetry.rs` and `shrink_frame_scratch.rs` production code; there are zero test needles. The key fold is `for light in lights { … position_radius … color_type … direction_angle … params }` in `build_and_upload_instances`.
- **Impact**: No live defect. This is a regression path that no `cargo test` can catch, landing on the HIGH-adjacent SVGF/ReSTIR history signals (parked ghosting = MEDIUM floor).
- **Related**: #4046, #4943, #4942 (closed); #5055 (this window).
- **Suggested Fix**: Add a `production_text` scan of `draw.rs` asserting that the `build_and_upload_instances(` argument list passes the `frame_lights`-derived slice (or rename the shadow, e.g. `merged_lights`, and pin the name). Alternatively, return the merged slice through `CameraAssemblyOutput` and use it directly.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix



---
## #5213 — REN-D7-2026-10-03-03: denoise/resolve doc residue that #4874 and its predecessors missed — retracted Halton "LCM-6 period" rationale in `renderer.md`, and a phantom bindless STORAGE_IMAGE row in `shader-pipeline.md`
State: OPEN | Labels: documentation, renderer, low, doc-rot
- **Description**:
  1. `renderer.md` still says the jitter has "period 16 — #1093 — chosen as the nearest power of two above the natural LCM-6 period". `taa_jitter`'s own doc in `frame_params.rs` carries the 2026-08-31 correction: Halton sequences are aperiodic, so there is no LCM-6 period and the real motivation for 16 is an open question. #3606 fixed the two code sites. #4874 rewrote steps 2–3 of this same paragraph but left step 1.
  2. The descriptor table lists `0 | 1 | STORAGE_IMAGE (bindless) | Per-pass read/write images | bloom, svgf, taa`. The bindless layout (`build_bindless_descripto
- **Impact**: Audit and onboarding only. This is the table the skills tell auditors to trust instead of re-deriving descriptor facts.
- **Suggested Fix**: Replace step 1's rationale with "period 16 (#1093; motivation open, see `taa_jitter`)". Change row `0|1` to `COMBINED_IMAGE_SAMPLER` (bindless `samplerCube` array) used by `triangle` (and any other `bindings.glsl` includer).

---
## #5214 — REN-D8-2026-10-03-01: `shader-pipeline.md`'s inject binding table stops at 23; the live shader declares 26 bindings (0–25)
State: OPEN | Labels: documentation, renderer, low, shaders, doc-rot
- **Description**: c705c310d (#4784) added `layout(std430, set = 0, binding = 24) buffer CombustionOccupancyOut` and `binding = 25 readonly buffer CombustionOccupancyIn`. Both are in the descriptor-set layout and are rotated per FIF (out = slot `f`, in = slot `previous`). The same commit edited `shader-pipeline.md`, but only the `GpuLight` rows. The inject table still says "24 bindings", lists 0–23, and has no row for either occupancy buffer or its per-FIF rotation. Its own header warns: "verify against the source before relying on this table for a new binding".
- **Impact**: Documentation only. The next binding added to the inject set will be numbered against a stale table, which is how #3830 started.
- **Suggested Fix**: Add rows 24/25 (STORAGE_BUFFER, 16³ `u32` mask on the fog-cluster grid; 24 = this slot's atomicOr marks, 25 = previous slot's mask as the dilated skip gate). Update the count to 26.

---
## #5215 — REN-D8-2026-10-03-02: `combustion_occupancy_buffers` depends on the all-slots fence wait, but the `sync.rs` rider list does not name it; its previous-slot RAW is covered only by an unrelated barrier
State: OPEN | Labels: bug, renderer, low, sync
- **Description**: Frame N, slot `f`, host-zeroes and seeds `combustion_occupancy_buffers[f]`. That buffer was last read on the GPU by frame N‑1 (slot `1-f`) through binding 25, so slot `f`'s own fence does not retire that read. Only the top-of-frame all-slots wait does. The field doc and the commit body both say so ("the all-slots fence wait at the top of `draw_frame` has retired the previous reader"). The resource is still missing from the `sync.rs` list that the #4601/#3643 rule calls load-bearing and that #5117 relies on before any wait narrowing.
  - The read-after-write half has a similar 
- **Impact**: Nothing breaks today. If the cluster-cull barrier is narrowed to a buffer barrier, the RAW loses its only ordering. If the fence wait is narrowed per slot (#5117), the host write races frame N‑1's read of the mask.
  - Either change can produce a false-negative occupancy bit. A false negative skips the RK2 transport block where combustion actually sits, so a plume freezes in place.
  - `cargo test` cannot see either failure.
- **Suggested Fix**: Add the occupancy mask to the `sync.rs` rider list and its pinning test. Name the previous-slot mask in Stage B's barrier set, or document there that the cluster-cull barrier covers it. Stop at that: per the skill, any barrier edit is "needs syncval" (below).

---
## #5216 — REN-D8-2026-10-03-03: two of the #4784 guards cannot fail on the regressions they name
State: OPEN | Labels: bug, renderer, low, test-gap
- **Description**:
  1. `occupancy_marks_reset_between_builds` claims to pin "a fresh build must not inherit the previous frame's marks". However, `build_marks` allocates a new zeroed `Box<[u32; FOG_VOLUME_CLUSTER_COUNT]>` on every call, so the second build always starts from zero. The test also never reaches production's empty-volume arm, `self.fog_cluster_occupancy.fill(0)` in `dispatch`, because that arm does not call `build_fog_volume_clusters`. It stays green if either `occupancy.fill(0)` (in `build_fog_volume_clusters`) or the empty-branch `fill(0)` is deleted.
  2. The call-site check in 
- **Impact**: Test gap only. A missing reset fails conservatively: stale marks keep transport running wherever fire ever burned, which costs performance but not correctness. A literal `true` at the call site silently undoes the #4784 performance fix.
- **Suggested Fix**: Reuse one occupancy buffer across both `build_fog_volume_clusters` calls with a non-empty second volume list, and separately assert the `dispatch` empty branch's `fill(0)` via `production_text`. Drop the fallback needle, or match the call's argument list exactly.

---
## #5217 — REN-D9-2026-10-03-04: Doc rot in the skinning lane (bundle)
State: OPEN | Labels: documentation, renderer, low, doc-rot
- **Description**:
  - **(a)** The comment says the slot ceiling matches "`MAX_TOTAL_BONES / MAX_BONES_PER_MESH = 32` skinned meshes". The value is `SKIN_MAX_SLOTS = (196608 / 144) - 1 = 1364`, and the module-level const doc in `context/mod.rs` says so. 32 was the pre-#900 value.
  - **(b)** The rustdoc says the stamp is "bumped every frame this entity appears in `draw_commands` (including skip-path entries)". Since #4294 it is stamped from entity liveness by `refresh_morph_slot_lru` → `refresh_live_slot_stamps`, so that a non-recreatable MorphSlot is never reaped from a live entity. The dispatc
- **Impact**: (b) points a future fixer back toward the draw-list stamping that #4294 removed as a bug. (a) misstates a capacity by about 40×.
- **Suggested Fix**: (a) Replace "= 32" with a pointer to `SKIN_MAX_SLOTS`. (b) Restate the doc as "stamped from entity liveness each frame (`VulkanContext::refresh_morph_slot_lru`, #4294); `0` is the never-stamped sentinel (`skin_lru_stamp` never writes it, #4969)".

---
## #5218 — REN-D11-2026-10-03-02: `exposure_meter.comp` falls outside both the single-source constant net and the mirror pins, so the #5158 clamp-then-compensate order is guarded only on the Rust side
State: OPEN | Labels: bug, renderer, low, shaders, test-gap
- **Description**:
  - **Claim (a)**: "metering shader, chroma compress and this module cannot disagree".
    - The `EXPOSURE_CONSTANT` doc says it "Resolves to the shared `EXPOSURE_METER_NEUTRAL` (#5154) so the metering shader, the presentation chroma compress and this module cannot disagree". The #5154 commit message makes the same single-source claim.
    - `exposure_meter.comp` has no `#include "include/shader_constants.glsl"`. It hand-types `1.2 * exp2(-ev100)`, `* 8.0` (S/K) and the Rec.709 luma weights `vec3(0.2126, 0.7152, 0.0722)`.
    - A change to `EXPOSURE_METER_NEUTRAL` would move p
- **Impact**: Today there is no behaviour change, because the values agree. This is a latent drift path on the default render path's metering, and a code comment states a guarantee the code does not provide.
- **Suggested Fix**:
  - Include `shader_constants.glsl` in the meter and use `EXPOSURE_METER_NEUTRAL` and `LUMA_REC709`.
  - Add a source-order pin in `exposure_meter.rs`: clamp before the compensation multiply.

---
## #5219 — REN-D11-2026-10-03-03: presentation and exposure docs went stale after #5154 and #5158 (the "16x clamp" text and `tonemap(graded * exposure)`)
State: OPEN | Labels: documentation, renderer, low, doc-rot
- **Description**:
  - Since 7d99ba7f0, `MAX_AUTO_EXPOSURE` is 2.0, not 16. The maximum auto-mode lift is now about 0.74 stops, which gives chroma of about 0.88, not "halved". The updated test comment in `tonemap.rs` already says this; the three prose sites were not updated.
  - Since 4bf2ec3a4, presentation's tonemapper input is `compressed * exposure`, the luma-preserving chroma compress. The engine docs omit that stage.
  - Auto exposure has been the boot default since a070baaad and 546e7fbc7, so "Fixed mode (the default)" in the meter gate comment is wrong.
- **Impact**: Readers are misled about how strong the #5154 compress is under the envelope, and about the presentation stage list. There is no runtime effect.
- **Suggested Fix**:
  - Restate the three comments as "about 0.88 chroma at the 2× envelope cap; `ev` compensation can lift further".
  - Add the chroma-compress step to the two engine docs.
  - Change "Fixed mode (the default)" to "fixed mode".

---
## #5220 — REN-D12-2026-10-03-01: `DebugStats::groundcover_model_{demanded,emitted}` (#4920) are written every frame but nothing reads them
State: OPEN | Labels: bug, renderer, low, terrain-exterior
- **Description**: 4dfe97f3e added the two fields, and the commit message says the truncation is "reported as DebugStats::groundcover_model_{demanded, emitted}". The `app_events.rs` comment says it is "visible without `--bench-*`". A whole-tree grep finds the fields only at their definition, their `Default`, and that one writer. No surface reads them:
  - The `stats` console command (`commands/world_info.rs`) does not.
  - The debug server's `eval_stats` (`DebugResponse::Stats`) does not.
  - `log_stats_system` does not.
  - No debug-ui panel does.

  The visibility that does exist comes from th
- **Impact**: The fields are dead telemetry. Someone triaging missing plants through `byro-dbg` has no way to see the truncation counts #4920 meant to expose, short of a bench run or catching the one-time warn. If a reader is added later, it will show a stale exterior count in interiors. No render impact.
- **Suggested Fix**: Either surface the pair (one `stats` line, or a `DebugResponse::Stats` field), zero or invalidate `stats` on a frame where `prepare` finds nothing to place, and fix the field doc; or delete the fields and leave the warn and bench line as the documented surfaces.

---
## #5221 — REN-D12-2026-10-03-02: `gpu_timers.rs` module header has two inaccuracies that the #4981 refresh missed
State: OPEN | Labels: documentation, renderer, low, doc-rot
- **Description**:
  - (a) "The original four brackets (skin dispatch / skin palette / BLAS refit / TAA) shipped with the #1194 perf-bisect work." The skin-palette bracket was added by #3676 (`6a605e7fc`, slot 28 / `BIT_SKIN_PALETTE = 0x4000`). That commit rewrote the sentence from "The original three brackets (skin / BLAS refit / TAA)" and moved its own bracket into the #1194 history.
  - (b) The no-timestamp section says "`DeviceCapabilities::timestamp_supported == false` skips creation entirely; `last_snapshot()` returns zeroed values." The real gate is `DeviceCapabilities::gpu_timers_support
- **Impact**: Doc only. (b) is the more useful fix. A timestamp-capable GPU without `hostQueryReset` gets no timers, and the header points a reader at the wrong predicate. That is the same misreading #1478 fixed in code.
- **Suggested Fix**: Restore "three original brackets (skin dispatch / BLAS refit / TAA)" and credit skin palette to #3676. In the no-timestamp section, name `gpu_timers_supported()` and say that consumers zero the stats and clear every `_active` flag.


#### Existing #5172: addendum (sites the issue does not list)
#5172 (OPEN, `GpuTerrainTile` 160 → 176 B doc drift) does not list these sites:

---


## Resolution map
- #5212 draw.rs merged_lights rename + draw_frame-body source pin — a355d3673
- #5213 + #5214 renderer.md LCM rationale, descriptor row 0|1, inject table rows 24/25 + declaration-order note — 77696ec75
- #5219 16x-clamp prose x3, chroma-compress stage in both engine docs, Fixed-mode-default comment — 2f70d408c
- #5215 sync.rs rider 15 + contract-test entry + Stage B dependency note (no barrier edit, per needs-syncval) / #5216 shared-buffer reset test + dispatch empty-branch pin + no-fallback call-site needle — 3b7e25b91
- #5217 SKIN_MAX_SLOTS comment, MorphSlot::last_used_frame rustdoc — adcca3eff
- #5218 exposure_meter.comp includes shader_constants.glsl (EXPOSURE_METER_NEUTRAL + LUMA_REC709), spv recompiled (parity 36/36 green), clamp-before-compensation + shared-constant pins — fc73e0a66
- #5220 stats command GCModels line, prepare un-latches stats on nothing-to-place paths, docs updated, source pin — da9405d09
- #5221 gpu_timers header (three original brackets; gpu_timers_supported gate) + exal-groundcover.md 176 B residue (from #5172 close d61d06da0, folded per #5221 addendum) — d5e6add93
- Verified: renderer lib 1379 pass, core inspect pass, FULL workspace (incl. launcher) zero failed, zero warnings, check-shader-artifacts 36 shaders + early variant byte-identical
- Note: issue #5219 cited a070baaad for the auto-default; that hash is a player-body commit — the real one is 546e7fbc7 (verified via git log).
