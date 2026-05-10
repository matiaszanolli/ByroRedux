# Renderer Audit — 2026-05-09

**Mode**: full sweep, all 16 dimensions, deep depth.
**Auditor**: orchestrator + 16 renderer-specialist dimension agents.
**Baseline**: per-dim reports in `/tmp/audit/renderer/` (not committed); prior dim-focused audits in `docs/audits/AUDIT_RENDERER_2026-05-*.md`.
**Issues snapshot**: `gh issue list --limit 200` at audit start.

---

## Executive Summary

| Severity   | Total |
|------------|-------|
| CRITICAL   | 0     |
| HIGH       | 3     |
| MEDIUM     | 15    |
| LOW        | 33    |
| INFO       | 5     |
| **Total**  | **56** |

**Headline**: The renderer is correct in all the places that matter for current shipping content (sync, RT, tangent space, R1 material table, G-buffer, command recording). The three HIGH-severity items cluster on **resize-handling brittleness** plus one **BLAS-refit invariant**:

1. **REN-AUDIT-CROSS-01 (HIGH)** — Composite + bloom + volumetric resize chain has a 3-way gap (descriptor bindings 6/7 not rewritten, bloom/vol pipelines have no `recreate_on_resize`, composite holds dangling image-view bindings post-resize). One latent dangling-handle bug today, becomes a UAF the moment bloom resize lands.
2. **REN-D1-NEW-02 (HIGH)** — `render_finished[img]` per-image semaphore can be re-used before the present engine releases it under image-index aliasing; the `images_in_flight` fence guard covers GPU work, not present-engine processing.
3. **REN-D12-NEW-01 (HIGH)** — `refit_skinned_blas` accepts vertex/index counts re-derived from `mesh_registry` each frame; if `entity_id` ever remaps to a different `mesh_handle` (mod swap, future LOD), `mode = UPDATE` ships a `primitiveCount` mismatch — Vulkan VUID, undefined BVH state.

No CRITICAL findings. RT pipeline (BLAS/TLAS, ray queries, denoiser, composite reassembly), shader struct sync (R1 material table, GpuInstance, Vertex), and tangent-space / normal-map handling are all in excellent shape. Past-audit carry-overs (#820, #821, #671, #786, #787, #788, #795, #796, #804, #806, #897, #899, #903, #904) all verified shipped.

### RT Pipeline Assessment

- BLAS/TLAS correctness: PASS. Vertex format / stride (100 B post-M-NORMALS), UINT32 indices, OPAQUE for alpha-tested geometry per #449, scratch alignment, host→AS barriers gated on `copy_size > 0`, `instance_custom_index` ↔ SSBO mapping per #419, `TRIANGLE_FACING_CULL_DISABLE` per-`two_sided` gating per #416, empty-TLAS valid descriptor from frame 0, deferred destroy via #372 — all verified. Two MEDIUM findings (REN-D8-NEW-04 missing `debug_assert!` on `last_blas_addresses.len()`, REN-D8-NEW-05 single-shot `build_blas` lacks budget guard) plus 11 LOWs.
- Ray queries (5 sites in triangle.frag): PASS. Frisvad basis everywhere, every site gated on `rtEnabled`, `gl_RayFlagsTerminateOnFirstHitEXT` everywhere expected, `instance_custom_index` (not `InstanceId`), GI miss → cell ambient, glass IOR loop bounded. One MEDIUM (REN-D9-NEW-03 glass-IOR ray-budget under-counts the multi-passthru loop), one LOW (REN-D9-NEW-04 sun-jitter spherical-cap math + stale "physical sun" comment).
- Denoiser/SVGF: PASS. Reprojection math correct, mesh-ID #904 mask separation verified live, ping-pong ordering correct, history-recovery latch in place. Two HIGH/MEDIUM findings cluster on the resize-gap above.
- TAA: PASS. Halton(2,3) / un-jittered camera UBO / per-FIF history slot / mesh-ID disocclusion / first-frame guard / GENERAL layout — all correct. Three LOW findings (motion vector point-sample, prev_mid OTHER-FIF on session frame 0, src_stage_mask overspec).
- GPU skinning + BLAS refit: 1 HIGH (REN-D12-NEW-01 above), 2 MEDIUM (skinned-BLAS shares LRU budget with static; bone palette buffers not DEVICE_LOCAL with staging), 2 LOW.

### Rasterization Assessment

- Pipeline state: PASS modulo composite-resize gap (REN-D3-NEW-01/02 == REN-AUDIT-CROSS-01). Vertex input matches Vertex (100 B / 25 floats / 9 attrs), push constants match shader, dynamic state set per frame, all 6 G-buffer outputs wired.
- Render pass + G-buffer: ZERO findings — clean since 2026-05-06.
- Command recording: 4 findings, all MEDIUM/LOW. Begin/end balance and recording-phase ordering correct; the 4 findings are local hitches and redundant state setters.
- Resource lifecycle: 8 findings, 1 HIGH (REN-D7-NEW-04 == REN-AUDIT-CROSS-01), 1 MEDIUM (post-resize TAA jitter not reset), 6 LOW.
- Shader correctness (struct sync, descriptor layouts): 3 LOW only. R1 material table (260 B GpuMaterial, 65-offset pin via #806), all 3 user-facing shaders in lockstep, SVGF #904 mask, ACES tone mapping, all verified.
- Material table (R1): 4 findings, all LOW/INFO. Layout invariants pinned by tests, shader lockstep verified, dedup ratio telemetry exposed (#780).
- Sky/weather/exterior: 7 findings — distance fog silently disabled (REN-D15-NEW-02 MEDIUM, M55 Phase 3 side effect), worldspace cross-fade unreachable (REN-D15-NEW-01 MEDIUM, no caller exists), window portal sky hardcoded (REN-D15-NEW-03 MEDIUM).
- Tangent-space / normal maps: ZERO findings. All 3 import paths (Bethesda authored, FO4 inline, synthesized) verified consistent; perturbNormal default-on with DBG bypass; bit catalog clean.

### Verified-clean dimensions

- **Dim 4** (Render Pass / G-Buffer): 0 findings, unchanged since 2026-05-06.
- **Dim 16** (Tangent-Space / M-NORMALS): 0 findings.

---

## Cross-Cutting Finding (consolidated from Dims 3, 7, 10)

### REN-AUDIT-CROSS-01 (HIGH) — Composite + bloom + volumetric resize chain has a 3-way gap

**Dimensions**: Pipeline State + Resource Lifecycle + Denoiser/Composite (3 agents reported the same root cause from different angles)
**Locations**:
- `crates/renderer/src/vulkan/composite.rs::recreate_on_resize` (writes only 6 of 8 descriptor bindings; binding 6 `volumetric_view`, binding 7 `bloom_view` are skipped)
- `crates/renderer/src/vulkan/bloom.rs` (no `recreate_on_resize` method; `BloomPipeline::destroy` exists at ~676 but no companion recreate)
- `crates/renderer/src/vulkan/volumetric.rs` (same — no `recreate_on_resize`)
- `crates/renderer/src/vulkan/context/resize.rs:213-253` (calls SSAO destroy + new on resize but skips bloom + volumetric)
- Source agent IDs: REN-D3-NEW-01 (HIGH), REN-D3-NEW-02, REN-D7-NEW-04 (HIGH), REN-D10-NEW-01 (HIGH), REN-D10-NEW-02 (MEDIUM), REN-D10-NEW-08 (LOW)

**Status**: NEW

**Why it's a bug**:
- Bloom's mip chain is sized from `screen_extent / 2` at construction. After window resize, composite's binding 7 still points at original-extent mips → bloom additive contribution sampled at wrong resolution. Visibly drifts off bright surfaces during live resize (per REN-D7-NEW-04).
- Volumetric pipeline is currently gated off in composite (`vol.rgb * 0.0` keep-alive at composite.frag:362 — see REN-D6-NEW-02), so the binding 6 issue does not surface today. But the moment volumetric is re-enabled (M-LIGHT future tier), the same resize gap activates immediately.
- The composite descriptor still holds the original-extent image views, so there's no UAF *today*. But once REN-D10-NEW-02 ships (bloom/vol pipelines gain `recreate_on_resize` and destroy the old views), the descriptor becomes a dangling-handle UAF unless REN-D10-NEW-01 ships in the same change.

**Fix sketch** (atomic — must land together):
1. Add `BloomPipeline::recreate_on_resize` and `VolumetricsPipeline::recreate_on_resize` mirroring the SSAO pattern (destroy + re-construct with new extent).
2. Wire both into `recreate_swapchain` after the SSAO recreate at `resize.rs:213-253`.
3. Extend `composite.rs::recreate_on_resize` to rewrite all 8 bindings (currently only 0-5).
4. Add a `const_assert!` or runtime check that asserts the binding count matches `composite.rs::DESCRIPTOR_COUNT` when growing.

**Repro**: live window resize during gameplay (RenderDoc / VK_LAYER_KHRONOS_validation). Per `feedback_speculative_vulkan_fixes.md`, do NOT ship without RenderDoc validation — failure modes are invisible to `cargo test`.

---

## Findings — Grouped by Severity

### HIGH (3)

#### REN-AUDIT-CROSS-01 — Composite + bloom + volumetric resize chain gap
*See Cross-Cutting section above.*

#### REN-D1-NEW-02 (HIGH) — `render_finished[img]` per-image semaphore can be re-used before present engine releases it

- **Dimension**: Vulkan Sync
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1936,1959` + `crates/renderer/src/vulkan/sync.rs:79-89`
- **Status**: NEW
- **Why it's a bug**: The `images_in_flight` fence guard covers GPU work completion, not present-engine processing. With multiple swapchain images and frame-pacing variance, a per-image `render_finished` semaphore can be signaled by frame N+1 while the present engine is still waiting on it from frame N. Canonical fix per Khronos sample: size `render_finished` per **frame-in-flight**, not per swapchain image, and key the present wait off `frame`. The `sync.rs:43-46` doc comment claims the opposite of what's true.
- **Fix sketch**: Resize `render_finished` from swapchain-image-count to `MAX_FRAMES_IN_FLIGHT` and update both submit and present sites to index by `frame` instead of `image_index`. Update sync.rs:43-46 doc.

#### REN-D12-NEW-01 (HIGH) — `refit_skinned_blas` accepts vertex/index counts re-derived per frame; entity remap → primitiveCount VUID

- **Dimension**: GPU Skinning
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:814-991, 1016-1116`, `crates/renderer/src/vulkan/context/draw.rs:646-666`
- **Status**: NEW
- **Why it's a bug**: `refit_skinned_blas` re-derives `vertex_count` / `index_count` from `mesh_registry` each frame using `entity_id` → `mesh_handle`. If that mapping ever changes (mod swap, future LOD switch, mesh hot-reload), `mode = UPDATE` ships a `primitiveCount` that mismatches the original BUILD. Vulkan VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03667 — driver behavior is undefined; on NVIDIA the BVH silently corrupts.
- **Fix sketch**: Store `built_vertex_count` / `built_index_count` on `BlasEntry` at BUILD time and `debug_assert_eq!` against the refit's derived counts. If they ever diverge, force a fresh BUILD instead of UPDATE.

### MEDIUM (15)

#### Vulkan Sync (Dim 1)

- **REN-D1-NEW-01** — `current_frame = 0` reset after resize can submit on a fence that may not have been waited on. `device_wait_idle` does NOT transition UNSIGNALED fences back to SIGNALED; the both-fences wait pattern at `draw.rs:108-120` can deadlock on a slot whose last `reset_fences` was issued mid-recording before resize. *Fix*: re-create or explicitly re-signal the in_flight fences after the wait_idle.
- **REN-D1-NEW-03** — Composite UBO host visibility uses a separate isolated barrier 750 lines away from the bulk host barrier. Fold into the existing instance_barrier at `draw.rs:1146` for canonical ordering.

#### Command Recording (Dim 5)

- **REN-D5-NEW-01** — `acquire_next_image` early-return on `OUT_OF_DATE` cleanly, but `?`-propagated errors *between* a successful acquire and `queue_submit` leak the `image_available[frame]` semaphore signal — next acquire on same slot trips validation. *Fix*: either stub-submit or re-create the semaphore on early-error path.
- **REN-D5-NEW-02** — First-sight skin compute prime + sync BLAS BUILD use a separate one-time cmd buffer + `transfer_fence` host-wait, stalling `draw_frame` per newly-visible NPC. Recording into the per-frame `cmd` would eliminate the hitch.
- **REN-D5-NEW-03** — `cmd_set_cull_mode(BACK)` at `draw.rs:1287` is unconditional — wasted when first batch is two-sided. Switch to the `Option<CullModeFlags>` sentinel pattern already used for `last_render_layer`/`last_z_function`.

#### Resource Lifecycle (Dim 7)

- **REN-D7-NEW-07** — `recreate_swapchain` resets `current_frame = 0` but NOT `frame_counter`, which feeds the Halton TAA jitter sequence. One frame of mis-aligned reprojection after every resize. *Fix*: reset `frame_counter` alongside `current_frame`, OR call `signal_temporal_discontinuity` to bump the SVGF/TAA recovery window.

#### Acceleration Structures (Dim 8)

- **REN-D8-NEW-04** — `decide_use_update` correctly length-checks `cached_addresses` vs `current_addresses`, but no `debug_assert_eq!` pins `last_blas_addresses.len() == instance_count` after the swap. A future "skip empty tail instances" optimization could silently desync them and trigger a `primitiveCount`-mismatch on the next UPDATE-mode build.
- **REN-D8-NEW-05** — `build_blas_batched` runs `evict_unused_blas` pre-batch and every 64 iterations, but the single-shot `build_blas` path never does. A streaming refactor that promotes single-shot to the hot path would silently bypass budget enforcement on smaller-VRAM GPUs.

#### RT Ray Queries (Dim 9)

- **REN-D9-NEW-03** — Glass IOR ray-budget under-counts the multi-passthru loop. `atomicAdd 2` for "1 reflection + 1 refraction", but `REFRACT_PASSTHRU_BUDGET = 2` means 3 iterations per refraction → worst-case 4 rays per fragment. Real GPU cost is ~2× the documented ceiling at the documented density target. *Fix options*: doc-only, or `atomicAdd 4`, or per-iteration accounting.

#### Denoiser & Composite (Dim 10)

- **REN-D10-NEW-03** — `frames_since_creation` increments at command-recording time, not GPU-completion time. Self-heals through the `svgf_failed` latch + resize cycle; defence-in-depth concern for a future hot-recovery path.
- **REN-D10-NEW-04** — Ping-pong arithmetic assumes `MAX_FRAMES_IN_FLIGHT >= 2`. Add `const_assert!` to pin the invariant at compile time.
- **REN-D10-NEW-05** — Composite's bindless cloud sample uses `texture()` (driver-derivative LOD) for layers 1/2/3 even though layer 0 explicitly uses `textureLod`. Mismatched LOD selection produces visible cloud-layer aliasing on rotation.

#### GPU Skinning (Dim 12)

- **REN-D12-NEW-03** — `evict_unused_blas` (`acceleration.rs:2336-2397`) gates on `total_blas_bytes` which includes skinned-BLAS bytes (added at `:973`), but the loop only walks `self.blas_entries` (static slots) for eviction candidates. With many concurrent NPCs (post-M41 spawning) the budget can sit permanently over-budget and LRU-thrash static BLAS every frame. *Fix*: separate `static_blas_bytes` accumulator from skinned.
- **REN-D12-NEW-04** — Bone palette buffers are `HOST_VISIBLE | STORAGE_BUFFER` only at `scene_buffer.rs:475-480`, not the audit-checklist DEVICE_LOCAL with HOST_VISIBLE staging. 6 MB of host-visible mapped storage read every frame by every skinned vertex on PCIe; should mirror the `terrain_tile_buffer` staging pattern at `:508-513`.

#### Sky/Weather (Dim 15)

- **REN-D15-NEW-01** — M40 worldspace cross-fade is dead code — `apply_worldspace_weather` invoked once at bootstrap; the cross-fade machinery is correct but no Phase 2 caller exists. *Fix*: wire the M40 streaming entry point.
- **REN-D15-NEW-02** — Composite distance fog mix removed (M55 Phase 3, 2026-05-09); volumetric replacement gated OFF (`vol.rgb * 0.0`) — **distance fog silently disabled across all cells**. Either restore the mix or commit to volumetric as the replacement.
- **REN-D15-NEW-03** — Window portal `skyColor = vec3(0.6, 0.75, 1.0)` hardcoded at `triangle.frag:1415` — interior windows ignore TOD/weather palette. Pull from active TOD palette instead.

### LOW (33)

For brevity, LOW findings are listed by ID with a one-line description. See per-dim reports for full evidence + fix sketches.

#### GPU Memory (Dim 2)
- **REN-D2-NEW-01** — `gbuffer::Attachment` and `svgf::HistorySlot` lack `GpuBuffer`-style Drop safety net. (LOW; not a current leak.)
- **REN-D2-NEW-02** — TLAS instance-buffer 8192 floor wastes ~1 MB BAR on cells with <100 instances. (LOW; trade-off documented.)

#### Pipeline State (Dim 3)
*Both Dim 3 findings consolidated into REN-AUDIT-CROSS-01.*

#### Command Recording (Dim 5)
- **REN-D5-NEW-04** — UI-overlay defensive viewport/scissor re-set at `draw.rs:1573-1586` is redundant — same values already set at render-pass entry.

#### Shader Correctness (Dim 6)
- **REN-D6-NEW-01** — `triangle.frag` uses raw `* 25` literals in 3 RT hit-fetch sites for vertex stride; should be a named const.
- **REN-D6-NEW-02** — `composite.frag:362`'s `vol.rgb * 0.0` keep-alive for binding 6 doesn't pin a tracking issue number for the M-LIGHT re-enable.
- **REN-D6-NEW-03** — `cluster_cull.comp` `MAX_LIGHTS_PER_CLUSTER = 32` lives shader-side only; not pinned to a Rust const (fold into existing #636).

#### Resource Lifecycle (Dim 7) — 6 LOW
- **REN-D7-NEW-01** — `pipeline_ui` shares the raster `pipeline_layout`; layout destroyed before subsystem pipeline layouts (handle survives, ordering brittle).
- **REN-D7-NEW-02** — `pipeline_cache` saved to disk AFTER subsystem `destroy()` calls — late pipeline creation in destroy paths gets dropped.
- **REN-D7-NEW-03** — `failed_skin_slots: HashSet<EntityId>` not cleared in Drop / no contract that it must outlive `skin_slots`.
- **REN-D7-NEW-05** — Drop pass calls `accel_manager.destroy()` without first calling `tick_deferred_destroy` — relies on `drain_pending_destroys` inside destroy.
- **REN-D7-NEW-06** — `terrain_tile_scratch` Vec held on `self` but never drained in Drop (fine as plain CPU memory; no comment block ties the cluster together).
- **REN-D7-NEW-08** — `recreate_swapchain` rebuilds SSAO via destroy + new but keeps the same `pipeline_cache` handle (cosmetic; driver behaviour well-defined).

#### Acceleration Structures (Dim 8) — 11 LOW
- **REN-D8-NEW-01** — `geometry.flags = OPAQUE` on `INSTANCES` geometry is spec-meaningless; drop or migrate to per-instance `FORCE_OPAQUE`.
- **REN-D8-NEW-02** — Host→device copy relies on `write_mapped`'s implicit flush; document or assert.
- **REN-D8-NEW-03** — Empty-TLAS `mem::take`/restore is harmless; documented.
- **REN-D8-NEW-06** — Single-shot `build_blas` sets `ALLOW_COMPACTION` but never compacts; flag wasted.
- **REN-D8-NEW-07** — All TLAS instances use mask `0xFF`; per-light-type mask buckets unused.
- **REN-D8-NEW-08** — Skinned BLAS uses `PREFER_FAST_BUILD`; post-#679 600-frame rebuild threshold the math now favors `PREFER_FAST_TRACE`.
- **REN-D8-NEW-09** — One-frame capacity-amortisation lag across cell-unload → cell-load.
- **REN-D8-NEW-10** — TLAS `padded_count` query over-allocates; documented trade-off.
- **REN-D8-NEW-11** — Column-major-`[f32;16]` → `VkTransformMatrixKHR` 3×4 conversion hand-unrolled with no unit test pinning it.
- **REN-D8-NEW-12** — `frame_counter` shared across TLAS slots; cosmetic.
- **REN-D8-NEW-14** — `missing_blas` warn doesn't identify offending draws; UI/particle-quad with `in_tlas=true` would silently increment.

#### RT Ray Queries (Dim 9)
- **REN-D9-NEW-04** — Directional shadow jitter no longer matches the "physical sun" comment (radius widened 4× in M-LIGHT v1); also `T·dx + B·dy` is a 2D disk on tangent plane, not a spherical cap (geometric bias <0.02% at current 0.020 rad — not visibly wrong, but math should be cap sample for future PCSS).

#### Denoiser & Composite (Dim 10)
- **REN-D10-NEW-06** — `screen_to_world_dir` uses `world.xyz / world.w` without guarding `w == 0`.
- **REN-D10-NEW-07** — `compute_sky` reads `params.sun_dir.xyz` and re-normalizes despite host promising it's already normalized.
- **REN-D10-NEW-08** — `composite.rs::recreate_on_resize` parameter list lost track of `volumetric_views` and `bloom_views` (signature parity with init).

#### TAA (Dim 11)
- **REN-D11-NEW-03** — Motion vector point-sampled with no 5-tap dilation — silhouette ghosting (audit checklist explicitly asks for dilation).
- **REN-D11-NEW-04** — TAA descriptor binds `prev_mid` to the OTHER FIF slot; on session frame 0 that slot is UNDEFINED. Currently safe (first_frame guard returns before the sample) but the layout-vs-descriptor mismatch is fragile latent risk.

#### GPU Skinning (Dim 12)
- **REN-D12-NEW-02** — `bone_palette_overflow_tests` doc-comments at `byroredux/src/render.rs:1739-1748, 1793, 1814` and `:24, :32` cite pre-bump `MAX_TOTAL_BONES = 4096` / 32-mesh ceiling; actual constant is 32768 / 256 meshes.
- **REN-D12-NEW-05** — Inline-skinning (`triangle.vert:147-204`) and compute pre-skin coexist on same mesh today (Phase 2 is RT-side only). Reserve a `GpuInstance.flags` bit (`PRESKINNED_FLAG`) now as guarded no-op so Phase 3 lands as per-instance flag flip.

#### Caustics (Dim 13)
- **REN-D13-NEW-02..07** — 6 LOW: dead-code 32767 ceiling check after 15-bit mask; first-frame pre-clear barrier docstring drift; single-eta single-bounce undocumented assumption; hard-coded `tintLum 0.05` floor; LOD-0 literal pinned to 1-mip image; hard radius cliff for point/spot + no directional cosine. (REN-D13-NEW-01 MEDIUM also flagged: caustic-source CPU gate over-includes hair / foliage / particle quads / decals / FX cards.)

#### Material Table (Dim 14)
- **REN-D14-NEW-01** — Stale `272 B` / `17 vec4` doc references in three sites (post-#804 the layout is 260 B / 16 vec4).
- **REN-D14-NEW-02** — No build-time grep guard for `GpuMaterial` GLSL field names; offset pin (#806) catches byte-position drift but not name renames.
- **REN-D14-NEW-03** — `materials_unique` telemetry off-by-one from seeded slot is unflagged.

#### Sky/Weather (Dim 15)
- **REN-D15-NEW-04** — `traceReflection` miss + glass-refraction miss return `fog * 0.5 + ambient * 0.5` — `fog` UBO is "unfogged HDR" after REN-D15-NEW-02 lands (semantically orphaned).
- **REN-D15-NEW-05** — `weather_system` duplicates 22-line keys walker in transition branch; hoist to `pick_tod_pair` helper.
- **REN-D15-NEW-07** — `transition_done` swap leaves `WeatherTransitionRes` resident with `duration_secs = INFINITY`; relies on float arithmetic as state machine.
- **REN-D15-NEW-06** — Sun south tilt `z = -0.15` actually north in RH-Y-up. **DUPLICATE-OF #802** (already filed).

### INFO (5)

- **REN-D2-NEW-03** — `NON_COHERENT_ATOM_SIZE = 256` is the worst-case fallback; could be replaced with the queried device value to halve flush ranges on desktop GPUs.
- **REN-D2-NEW-04** — TLAS resize destroys + recreates synchronously; relies on caller fence-wait but doesn't `device_wait_idle` defensively.
- **REN-D8-NEW-13** — Empty-then-non-empty frame BUILDs twice; verified correct, no action.
- **REN-D11-NEW-05** — TAA pre-barrier `src_stage_mask` includes `FRAGMENT_SHADER` — over-spec since the per-FIF fence already covers composite's read of this slot from the previous frame. Cosmetic.
- **REN-D14-NEW-04** — Seeded neutral re-uploads on the very first frame (which is what the dirty-gate is supposed to skip).

---

## Cross-References to Prior Audits

**Verified shipped since prior audits**:
- #786 (Bethesda authored tangent handedness) — verified at `mesh.rs:48-161`.
- #787 / #788 (perturbNormal default-on with DBG_BYPASS_NORMAL_MAP opt-out) — verified at `triangle.frag:917-921`.
- #795 / #796 (FO4 inline tangent decode) — verified at `tri_shape.rs:609-741`.
- #804 (GpuMaterial 260 B by dropping unread `avg_albedo_r/g/b`) — verified by `gpu_material_size_is_260_bytes`.
- #806 (per-field offset pin: 65 named-field offsets across 16 vec4 slots) — verified by `gpu_material_field_offsets_match_shader_contract`.
- #820 / REN-D9-NEW-01 (Frisvad basis on glass IOR roughness spread) — verified at `triangle.frag:1540`.
- #821 / REN-D9-NEW-02 (window-portal raw-N bias documentation) — verified at `triangle.frag:1382-1391`.
- #897 (fog/palette drift) — fixed.
- #899 (cloud layer multipliers) — fixed.
- #903 (TAA NaN guard) — verified at `taa.comp:165-167`.
- #904 (SVGF mesh-ID 15-bit mask separates from alpha-blend bit 15) — verified at `svgf_temporal.comp:142` and `taa.comp:116`.
- #671 (GI miss → per-cell ambient) — verified at `triangle.frag:2378`.
- #789 (glass IOR same-texture passthru identity skip + 8192 budget + cell-ambient miss fallback) — verified.

**Open issues confirmed still open** (re-cited, not refiled):
- #661 (skin compute → BLAS refit barrier uses ACCELERATION_STRUCTURE_READ_KHR for vertex buffer reads, should be SHADER_READ).
- #802 (Sun arc tilts north — REN-D15-NEW-06 is the same root cause).

---

## Prioritized Fix Order

1. **REN-AUDIT-CROSS-01** (HIGH, latent UAF) — Composite + bloom + volumetric resize chain. Atomic 4-step fix (recreate methods on bloom + vol, wire into resize, extend composite descriptor rewrite, add binding-count assert). **MUST land as one PR** to avoid creating the UAF window.
2. **REN-D12-NEW-01** (HIGH, BVH UB) — `refit_skinned_blas` invariant. Add `built_vertex_count` / `built_index_count` to `BlasEntry`, debug-assert at refit, force fresh BUILD on mismatch.
3. **REN-D1-NEW-02** (HIGH, validation error under image aliasing) — Resize `render_finished` from per-image to per-frame, update both submit and present sites, fix sync.rs:43-46 doc.
4. **REN-D1-NEW-01** (MEDIUM, deadlock potential after resize) — Re-create or re-signal in_flight fences after `device_wait_idle`.
5. **REN-D5-NEW-01** (MEDIUM, semaphore leak in error path) — Stub-submit or recreate `image_available[frame]` on error path between acquire and submit.
6. **REN-D7-NEW-07** (MEDIUM, stale TAA jitter post-resize) — Reset `frame_counter` in `recreate_swapchain` OR call `signal_temporal_discontinuity`.
7. **REN-D15-NEW-02** (MEDIUM, distance fog silently disabled) — Decide: restore composite fog mix, or commit to volumetric as replacement and re-enable.
8. **REN-D15-NEW-01** (MEDIUM, dead M40 cross-fade code) — Wire `apply_worldspace_weather` into the M40 streaming entry point.
9. **REN-D15-NEW-03** (MEDIUM, window portals ignore TOD) — Pull `skyColor` from active TOD palette instead of hardcoded `vec3(0.6, 0.75, 1.0)`.
10. **REN-D12-NEW-03** (MEDIUM, LRU thrash on NPC-heavy scenes) — Separate `static_blas_bytes` from skinned in `evict_unused_blas`.
11. **REN-D12-NEW-04** (MEDIUM, PCIe bandwidth waste) — Migrate bone palette to DEVICE_LOCAL + HOST_VISIBLE staging.
12. **REN-D9-NEW-03** (MEDIUM, RT budget telemetry off by 2×) — Glass IOR ray-budget under-counting; pick option 1 or 2.
13. **REN-D5-NEW-02** (MEDIUM, NPC spawn hitch) — Move first-sight skin compute into per-frame cmd buffer.
14. **REN-D10-NEW-05** (MEDIUM, cloud aliasing) — Use `textureLod` for cloud layers 1/2/3 to match layer 0.
15. **Remaining MEDIUM + all LOW**: sweep into a follow-up bundle issue, address as time permits.

---

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_RENDERER_2026-05-09.md
```

Filing recommendation: 3 HIGH + 13 MEDIUM as individual issues; the 33 LOW + 5 INFO into a bundle issue (`REN-LOW-BUNDLE-2026-05-09`). REN-D15-NEW-06 is **DUPLICATE-OF #802** (do not refile).

Per `feedback_speculative_vulkan_fixes.md`, **REN-AUDIT-CROSS-01 must NOT ship without RenderDoc verification** — failure modes are invisible to `cargo test` and `cargo run` without live window resize.
