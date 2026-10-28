**HEAD**: `fa6be4a6b` · **Baseline**: `docs/audits/AUDIT_RENDERER_2026-10-24.md` (HEAD `d5d13edcc`; 186 commits since, 42 under `crates/renderer`) · **Audited**: Dims 1–8 (delta traced) · **Unchanged since baseline (skimmed)**: Dim 9 (test-only #6370), Dim 10 (rename + constant hoist), Dim 11 (constant hoist + allow reasons), Dim 12 (rename)

# Renderer Audit — 2026-10-27 (delta, suite run)

## Executive Summary

| Severity | NEW | Dup | Findings |
|---|---:|---:|---|
| CRITICAL | 0 | 0 | — |
| HIGH | 0 | 1 | REN-D5-01 (Dup of AUDIT_SAFETY_2026-10-27 SAFE-2026-10-27-01) |
| MEDIUM | 1 | 0 | REN-D2-01 |
| LOW | 4 | 0 | REN-D1-01, REN-D8-01, REN-D8-02, REN-D8-03 |

- **Guards**: renderer lib **1501 passed, 0 failed, 1 ignored** (the device-only `gpu_filter_preserves_constant_radiance_and_broadens_a_lobe`). Bin-crate renderer guards (24 selected: static-BLAS recovery, pool reserve, bench keys, bone-palette overflow, skin rollback, FSR default, `mat_set`, `every_exterior_spawner_inserts_a_boundary_material`) pass. `cross_game_translation_completeness` (ignored, run explicitly) passes in 3.18 s. `scripts/check-shader-artifacts.sh`: 38 shaders + the early-test variant match glslang byte for byte, so shader-side guards are trustworthy.
- **Every 2026-10-24 finding is closed**: REN-D5-01 → #6343 (`9be0666fd`), REN-D7-01 → #6344 (`8828dc6a5`), REN-D2-01 → #6345 (`94acf0bff`), REN-D2-02 → #6346 (`c3b84ec32`), REN-D3-01 → #6347 (`6d372a0af`), REN-D5-02 → #6348 (`ad04a66b2`), REN-D5-03 → #6349 (`d96fd8fe4`). The carried #6249 and #6250 are fixed (`0999490b9`, `bed162262`). Each fix was read; all are complete.
- **The one high-impact defect is a fix-introduced leak**: #6250 added two per-frame `moved_surface_buffers` that `SceneBuffers::destroy` never frees, so every shutdown since 2026-10-24 takes the #665 allocator leak-guard branch (debug: assert panic; release: device + instance leaked). `/audit-safety` filed it first; recorded here as a dup with the source-scan guard suggestion.
- **#6250's moved-surface set overflows in any populated scene** (REN-D2-01): the cap of 64 counts skinned mesh *parts*, so ~10 animated NPCs exceed it. The uploader then warns every frame and truncates in sort order.
- **New code in the window**, read end to end: the moved-surface set and blocker recording (#6250), the shadow-filter ownership gate (#6249), the sun-drift rework of the SVGF key (#6344), fail-closed layout walks (#6343), terrain LOD softening (`e71451d08`), geometry-pool caps and admission tiers (`3929a406c`, `ab9d46d18`, `bb97be77c`, `22d6e1a80`), the `build_and_upload_instances` / `build_pipelines_and_finish` splits (`3e6f6b66a`), the `#6369` image-info helper migration (per-site census: every layout preserved), and 13 water / underwater god-ray commits. Water content and god-ray sun/IOR handling were already covered by `/audit-exterior` 2026-10-26 (#6415, #6418, #6419, #6420) and are not re-filed.

## RT Pipeline Assessment

- **AS / SSBO indexing**: no commit under `acceleration/`. After the #6143 split, `build_and_upload_instances` still has `mesh_registry.get()` as the only reject before `gpu_instances.push`. The #2913 compaction assert still sits between the loop and the UI quad. The `push_or_extend_batch` helper's `return` replaces a trailing `continue`, so control flow is unchanged. The call order is correct: final grow → batch clamp → uploads. Three of the source pins guarding that order now pass on definition order alone (REN-D1-01).
- **Ray queries**: #6250's two-arm taint was traced end to end. The `occluderSurface` tag uses the same derivation as `GpuInstance.surface_id`; the shader re-clamps the count; `shadow_transport.glsl` records the blocker at all three zero or attenuating returns. #6345 and #6346 are complete across all three marchers.
- **Denoiser**: #6344 moves exactly the first directional light's 7 colour and direction lanes out of the exact fold, and every other lane stays exact. #6249's ownership compare is exact for opaque owners and rejects blended ones. The water residue is documented by the commit.

## GPU-Struct & Memory Assessment

- `GpuTerrainTile` @220 was renamed from `parallax_reserved` to `lod_soften_end` in Rust, GLSL and shader-pipeline.md, and pinned. `GpuWaterParams.optical.zw` reuses reserved lanes, so the size is unchanged. The water bed rides `Vertex.color`, and water is not in the TLAS. `MovedSurfaceSet` (binding 25) is sized `(1 + 64) × 4` B per FIF and its count is clamped on both sides.
- Geometry pool: the tier ceilings use `saturating_sub`, the reserve clamp cannot underflow, a refusal latches only its own tier, and all three latches reopen on compaction. Actors, statics and the three prop-LOD families are wrapped in their tiers; terrain and water stay `Essential`.
- **Teardown gap**: REN-D5-01. It is the only new allocation in the window (diff grep for `create_host_visible|create_device_local|create_host_readback|GpuImage::|clamp_sampler(`) and the only one not destroyed.

## Findings

### REN-D5-01: `SceneBuffers::destroy` never destroys the #6250 `moved_surface_buffers`, so every shutdown takes the #665 allocator leak-guard branch
- **Severity**: HIGH
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/scene_buffer/descriptors.rs` (`SceneBuffers::destroy`); field in `crates/renderer/src/vulkan/scene_buffer/buffers.rs` (`moved_surface_buffers`)
- **Status**: Dup of AUDIT_SAFETY_2026-10-27 SAFE-2026-10-27-01 (found independently here; do not re-file)
- **Description**: `bed162262` added `moved_surface_buffers: Vec<GpuBuffer>` (two host-visible buffers). The constructor seeds them and binding 25 binds them, but `SceneBuffers::destroy` has no loop for them. Each `GpuBuffer` from `create_bound` holds `allocator: Some(allocator.clone())`. So `Arc::try_unwrap(alloc_arc)` in `context/teardown.rs` fails and takes the leak-guard arm, which runs `log::error!` and `debug_assert!(false, "GPU allocator leaked: outstanding Arc references")` and returns before `destroy_device` / `destroy_surface` / `destroy_instance`. The buffers' later natural drop then trips `GpuBuffer::drop`'s `debug_assert!(false, "GpuBuffer leaked into Drop")`.
- **Impact**: Every clean exit. Debug builds panic in teardown. Release builds leak the device, surface, instance and messenger until process exit. The message also hides any real allocator leak.
- **Suggested Fix**: Destroy and clear `moved_surface_buffers` beside `selected_ray_probe_buffers`. Also add a source-scan test that every `GpuBuffer` / `Vec<GpuBuffer>` field of `SceneBuffers` appears in `destroy`, since this class will recur with the next per-frame buffer.

### REN-D2-01: The #6250 moved-surface set (cap 64) overflows in any populated scene, and `upload_moved_surface_ids` then warns on every frame
- **Severity**: MEDIUM
- **Dimension**: Ray Queries
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`upload_moved_surface_ids`); producer `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (`instance_flags`, `moved_surface_ids`); `RESTIR_MAX_MOVED_SURFACES` in `crates/renderer/src/shader_constants_data.rs`
- **Status**: NEW
- **Description**: One ID is pushed per moving draw *entity*, where moving means `previous_source != m || pose_dirty.contains(entity)`. `pose_dirty` is per `SkinnedMesh` entity, so each skinned part counts separately: body, head, hands, hair, eyes and armour. `byroredux/src/render/skinned.rs` (`try_mark_pose_dirty`) re-marks a part every frame its bone hash changes, which includes idle animations. About ten animated NPCs pass 64. The uploader truncates to the first 64 in draw-sort order and calls `log::warn!("moved-surface set over capacity: …")` unconditionally, so it warns every frame.
- **Evidence**: `let kept = moved_surface_ids.len().min(RESTIR_MAX_MOVED_SURFACES as usize); if moved_surface_ids.len() > kept { log::warn!(…) }` has no once-gate. The commit cites no census for the value 64.
- **Impact**: About 60 warn lines per second for as long as a crowd is on screen (Whiterun market, the Strip, Megaton). The prev-occluder arm (first-moving-frame and teleport taint) is silently lost for an order-dependent subset of movers, in exactly the scenes the arm was added for. Overflowed movers still taint through the hit arm, so the image cost is bounded.
- **Related**: #6250 (closed), `bed162262`.
- **Suggested Fix**: Warn once per overflow episode and re-arm when the count drops back under the cap. Expose the overflow count in `rt.integrity`. Size the cap from a populated-cell census, or collapse the per-part fan-out by keying both the set and `occluderSurface` on the actor.

### REN-D1-01: #6143's split turned the #4199 / #4726 instance-SSBO order pins into definition-order checks
- **Severity**: LOW
- **Dimension**: AS Correctness (test gap)
- **Location**: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`, tests `instance_buffers_grow_before_upload_and_rebind_the_caustic_set` and `the_clamp_runs_after_the_grow_and_before_the_indirect_upload`
- **Status**: NEW
- **Description**: Both tests compare text offsets in the whole file. Since `3e6f6b66a`, the right-hand needles (`.upload_instances(&self.device, frame, gpu_instances)` and `upload_indirect_draws(`) live in the helper `upload_instance_and_draw_buffers`. That helper is defined after `build_and_upload_instances`, so `call < upload` and `clamp < indirect` hold because of where the helper is defined, whatever the call order.
- **Impact**: Moving `self.upload_instance_and_draw_buffers(` above the final grow or the batch clamp would reintroduce #4199 (the upload clamps to the old capacity) or #4726 (batches reference past capacity), and both pins would stay green. The call order is correct today.
- **Suggested Fix**: Anchor on the call site `self.upload_instance_and_draw_buffers(` inside a `fn_body` cut of `build_and_upload_instances`, as `nothing_before_the_final_grow_binds_the_scene_set` already does. Pin separately that the helper body contains both uploads.

### REN-D8-01: The underwater god-ray focus pattern is sampled in render-origin-relative space, so it jumps at every 4096-unit render-origin snap
- **Severity**: LOW
- **Dimension**: Water / Camera-Relative Precision
- **Location**: `crates/renderer/shaders/composite.frag` (`underwaterGodRays`, `underwaterSurfaceFocus`); `crates/renderer/src/vulkan/context/frame_params.rs` (`build_composite_params`, `camera_pos` rebase)
- **Status**: NEW (not covered by EXT-D4/D5-2026-10-26)
- **Description**: The march computes `p = params.camera_pos.xyz + viewDir * t` and `entry = p.xz - travel.xz * sunPath`, then evaluates value noise at `entry / 70.0` and `entry / 37.0`. `CompositeParams.camera_pos` is deliberately render-origin-relative, and the origin snaps by `RENDER_ORIGIN_SNAP` (`EXTERIOR_CELL_UNITS`, 4096). Since 4096/70 and 4096/37 are not integers, the lattice shifts on the frame the origin re-snaps, while the water surface (which uses absolute positions in `water.vert`/`water.frag`) does not move. The hash is integer-exact, so this is an anchoring problem, not a precision one.
- **Impact**: Every shaft pops for one frame when an underwater camera crosses a cell-grid line in an exterior. Visual and transient.
- **Suggested Fix**: Ship the render origin's XZ, wrapped by a common period of both noise scales, in a spare lane (for example `UnderwaterView.surface.zw`) and evaluate the focus at `entry + originXZ`.

### REN-D8-02: The god-ray contact test takes a bilinearly filtered depth sample through the LINEAR `hdr_sampler`
- **Severity**: LOW
- **Dimension**: Water / Pipeline
- **Location**: `crates/renderer/shaders/composite.frag` (`underwaterSunVisibility`); `crates/renderer/src/vulkan/composite.rs` (binding 4 = `depth_read_only_image_info(partial.hdr_sampler, depth_view)`)
- **Status**: NEW (same class as closed #299, SSAO's bilinear depth read)
- **Description**: `underwaterSunVisibility` reads `linearViewDepth(uv, texture(depthTex, uv).r)`. Every other depth tap in the file is `texelFetch`, which ignores the sampler. This tap filters, with two consequences. (a) Across a silhouette it averages the near and far depth into a phantom depth in between, so the `6 < behind < 160` slab test misclassifies a 1-px halo around every occluder. (b) LINEAR filtering of `D32_SFLOAT` needs `SAMPLED_IMAGE_FILTER_LINEAR`, which Vulkan does not mandate for depth formats (VUID-vkCmdDraw-magFilter-04553), and `find_depth_format` checks only `DEPTH_STENCIL_ATTACHMENT`. RT-class desktop GPUs expose the feature, so (b) is a portability gap, not an error on the dev card.
- **Suggested Fix**: Use `texelFetch(depthTex, ivec2(uv * vec2(textureSize(depthTex, 0))), 0).r`, matching every other depth reader.

### REN-D8-03: `water.vert` still documents `inColor` as "unused on water — kept for VAO compat"
- **Severity**: LOW
- **Dimension**: Water (doc-rot)
- **Location**: `crates/renderer/shaders/water.vert` (`layout(location = 1) in vec4 inColor`)
- **Status**: NEW
- **Description**: Since `7dd294ee3`, the exterior water mesh carries the bed field in `inColor`: r = depth / `WATER_BED_DEPTH_MAX`, gb = shoreward direction, a = the `WATER_BED_VALID_ALPHA` sentinel. `main()` reads it. The declaration comment says the opposite, which invites a "cleanup" that drops the attribute.
- **Suggested Fix**: Rewrite the comment to name the bed encoding and `bed_vertex_color` (`byroredux/src/cell_loader/water.rs`).

### Existing / still present

- #6415, #6418, #6419, #6420 (exterior, 2026-10-26): underwater god-ray cloud shadow, IOR literal, shore literals, shore-direction parity. Not re-filed.
- #6341 (composite caustic taps / per-frame water accumulator clear), #6316 (cloud-shadow bake in interiors), #6315 (`fold_svgf_scene_inputs` re-hash), #5365 (all-slots fence wait), #6143 (pinned-unsplit drivers), #6371 (per-pass compute teardown), #6358 (gpu_timers verbosity), #6106 (refused full-detail slice never retried), #6248 (blade root colour): carried and unchanged.

### Checked, no finding

- **Dim 4**: #6369 per-site census keeps every sampler, view and layout. The #6249 mesh-ID input is in `SHADER_READ_ONLY_OPTIMAL` from the render-pass `final_layout`, the same way SVGF reads it. A #6343 failed resize leaves `swapchain.framebuffers` empty, so the extra `about_to_wait` frame winit delivers after `event_loop.exit()` takes `draw_frame`'s early return. `init_stages.rs`'s file-wide `too_many_arguments` allow carries its reason.
- **Dim 6**: #6396 removed only parameters the predicate never read. No per-game branch was added.
- **Dim 7**: covered above in the RT assessment. Composite order is still linear HDR: correction → fog/volumetric → god rays → precipitation → dither, with bloom downstream.
- **Dims 9–12**: skimmed. Only renames, constant hoists with preserved values (`PRESENTATION_RADIAL_TAPS` 6 / divisor 7, Fresnel 0.25/16/2, combustion floors) and test hygiene.

## Prioritized Fix Order

1. **REN-D5-01** (via SAFE-2026-10-27-01): a one-loop fix plus the field-coverage scan. It currently red-lines every debug shutdown and the lavapipe CI lane.
2. **REN-D2-01**: warn once per episode, add telemetry, then census the cap.
3. **REN-D1-01**: re-anchor the two order pins before the next edit to `build_and_upload_instances`.
4. **REN-D8-02**, **REN-D8-01**, **REN-D8-03**: one water/composite commit (depth `texelFetch`, world-anchored focus noise, comment).

## Needs-RenderDoc

1. #6249: on-device A/B (`r.shadowfilter on/off`) on emissive screens in front of lit walls, plus a `BYRO_VALIDATION=1` pass over the new sampled mesh-ID input (the commit itself lists both).
2. #6250: `render.debug restir_history`, with an NPC stepping out from behind a fence (the blocker-recording arm), and a teleport.
3. REN-D8-01: underwater camera crossing a 4096 grid line, to confirm the shaft pop.
4. #6343: still needs a fault-injection hook to prove the before and after VUID.
5. Carried: FSR layouts (reasoned only), the untested FP32 FSR permutation, egui `drop_without_applying_deltas` discard sites.

## Stale skill premises

For the next `/audit-sync`:

- **Dim 5**: add "every `GpuBuffer` field of `SceneBuffers` must appear in `SceneBuffers::destroy`" (REN-D5-01). The Scene Buffers ledger in memory-budget.md lists neither the moved-surface buffer nor the DALC or selected-ray-probe buffers. Either the table should say it is not exhaustive, or rows should be added.
- **Dim 2**: name the moved-surface overflow behaviour and the per-part (not per-actor) granularity of `INSTANCE_FLAG_MOVED_THIS_FRAME` / the moved set. #6250's `committedDistance` now means blocker distance (debug probe only).
- **Dim 1**: list `upload_instance_and_draw_buffers` / `update_scene_history_gates` / `instance_flags` as the post-#6143 homes of the upload, gate and flag code. Note that the two order pins became definition-order (REN-D1-01).
- **Dim 8**: add the bed field (`inColor` sentinel), the swell law in `water.vert`, and `composite.frag::underwaterGodRays` (relative-space inputs, filtered depth tap). Its sun/IOR items are `/audit-exterior`'s.
- **Dim 3**: `lod_soften_end` is already in the skill text. The `moved_surface_buffers` binding 25 row exists in shader-pipeline.md.
- **Delta scoping (zsh)**: run the `First step:` path lists through `bash`. zsh does not word-split an unquoted `$p`, and the loop silently reported zero commits for every dimension.
