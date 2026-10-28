**HEAD**: `fa6be4a6b` · **Baseline**: `docs/audits/AUDIT_PERFORMANCE_2026-10-24.md` (HEAD `df33c84e5`, 196 commits since) · **Audited**: Dims 1, 2, 3, 4, 5, 7 (delta read plus guard lanes), Dim 8 (guard lanes, telemetry inventory, harness provenance) · **Unchanged since baseline (skimmed)**: Dim 6 (only 594c843e2, a test-only source-cut change)

# Performance Audit — 2026-10-27

**Command**: `/audit-performance` inside `/audit-suite --preset comprehensive`. Per the suite rules there were no
sub-agents and no engine launch, and the build used the shared repo `target/`. Scratch: `/tmp/audit/performance/dim_{1..8}.md`.

**Mode**: static analysis, guard-test lanes, `scripts/check-shader-artifacts.sh` and
`scripts/check-bench-harness-provenance.sh`. **No GPU or CPU frame time is claimed.** Every cost below is labelled
*derived* (code plus a cited input). Anything that needs live telemetry is deferred to the runtime audit.

## Executive Summary

| Severity | NEW | Existing / tracked (still present) |
|---|---:|---:|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 0 | 2 (#6248 amplified, #5959) |
| LOW | 3 | 4 (#6313, #6315, #6316, #6341) |

- **No guard has eroded.** Every `Guard:` symbol in all eight dimensions resolves (`git grep`, 66 names). The lanes are:
  bin `cargo test -p byroredux --bin byroredux` 3350 passed / 1 failed, renderer `--lib` 1501 passed, core
  `--features inspect --lib` 882 passed, and the physics `water_scan_latches_are_not_siphash`, debug-server
  `metrics_response_mirrors_core_snapshot` and plugin `locate_refr_*` lanes are green. The one bin failure is
  `per_frame_driver_functions_stay_within_their_size_pins` (render_one_frame 728 vs 716), which is a
  **Dup of AUDIT_REGRESSION_2026-10-27 REG-01**.
- **All three NEW findings are in code added in this window**: the VATS targeting scan (#6350), the #6250 ReSTIR
  mover list, and the underwater god-ray march. All are LOW. None runs at a sustained per-frame scale that is
  measured to matter, and each sits on a site that another 10-27 audit already flags for a correctness reason, so
  the fixes can be made together.
- **The #6143 driver split moved code verbatim** (`collect_static_mesh_draws` 1031 → 154 lines,
  `build_and_upload_instances` 1339 → 551). It added no per-frame allocation, and the static-mesh loop still probes
  `GlobalTransform` before its siblings (#1377).
- **Observed vs ROADMAP: no delta can be stated.** No bench was run. The harness is byte-stable against the live
  record `d9a201c71`. R6a-regress-23 is still open and cannot be attributed statically. Two changes in this window
  could move GPU time and were not benched: ground-cover blades got ~1.69× larger (c9433d501, which feeds #6248),
  and the submerged composite gained 144 depth taps per pixel (PERF-D5-01). Both should be checked at the next
  bench-of-record.

## Hot Path Analysis

### Per-pass GPU cost
Not measured, because no engine was launched. Inventory check: `GpuTimerSnapshot` has 30 `_ms` brackets (25 named
+ 5 main-render sub-phases), `QUERIES_PER_FRAME` is 60 and `BENCH_GPU_KEYS` has 24 entries. No new `cmd_dispatch` /
`cmd_draw*` / copy / clear / fill / blit call was added under `crates/renderer/src/vulkan` since the baseline, so
the inventory needs no new row. The two new per-pixel costs both sit inside existing brackets: the god-ray contact
taps are in `composite_ms`, and the mover-set scan is in `main_render_ms`. RT quality tier: not applicable, since no
frame was rendered.

### Per-frame / per-event CPU (derived)

| Site | Cost shape | Cadence |
|---|---|---|
| `vats_scan` → `visible_fraction` (PERF-D1-01) | actors_in_range × parts × all `RapierHandles` lookups + one SipHash set per part. About 600 k lookups at 20 actors × 10 parts × 3 k bodies, roughly tens of ms | Once per VATS entry and per `vats.scan` (the world is frozen) |
| #6250 mover list (PERF-D4-01) | O(movers²) `contains` + 0→N regrowth. About 11 k compares at 150 movers and 125 k at 500 | Every frame with an animated actor or a moving rigid body |
| `set_terrain_lod_soften_end` (e71451d08) | O(terrain tiles) compare; dirties only on change | Every frame |
| `step_interior_forced_weather` (#6309) | Two resource reads while settled | Every interior frame |

### Harness provenance
`scripts/check-bench-harness-provenance.sh d9a201c71` → "harness byte-stable since this record". Neither harness
file changed in the window. `scripts/check-shader-artifacts.sh` → "38 shaders + opaque early-test variant match
glslang 11:16.4.0". `ENABLE_LEGACY_WRS` is 0.

## Findings

### Eroded guards
None.

### New issues

### PERF-D1-01: `vats_scan` rebuilds the target's collider-body set over every physics entity once per limb part
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/vats.rs` — `visible_fraction` (≈450-491), called per part from `vats_scan` (≈373-425)
- **Status**: NEW
- **Description**: For every part of every in-range actor, `visible_fraction` walks the whole `RapierHandles` storage
  and calls `world.get::<ActorColliderOwner>` per entry to build a std `HashSet` of the target's own bodies. The
  physics sync attaches `RapierHandles` to every registered collision body, statics included. The set depends only
  on the target, but it is rebuilt for each of its BPTD parts (FNV/FO3 limb tables carry ~6-14).
- **Evidence**: `let target_bodies: HashSet<_> = world.query::<RapierHandles>().map(|handles| handles.iter().filter(|(owner, _)| world.get::<ActorColliderOwner>(*owner).is_some_and(|o| o.0 == target)) ...).collect()`, inside the per-part loop of `vats_scan`.
- **Impact**: A one-frame hitch on VATS entry and on each `vats.scan` console call. Derived at 20 actors × 10 parts
  × 3,000 bodies: ≈ 600 k storage lookups, tens of ms. VATS freezes the world, so the hitch is cosmetic, not
  sustained. No quantitative guard exists for this site.
- **Related**: AUDIT_ECS_2026-10-27 ECS-02 / AUDIT_CONCURRENCY_2026-10-27 CONC-01 report the lock-order cycle in the
  same loop and the same `visible_fraction` guards. Fix them together.
- **Suggested Fix**: Before the actor loop, build `owner → bodies` once per scan from the sparse
  `ActorColliderOwner` storage (it covers ragdoll bodies only), then pass the target's slice into
  `visible_fraction`. This also drops the per-part SipHash set.

### PERF-D4-01: the #6250 mover list is a fresh per-frame `Vec`, deduplicated by linear `contains`, and keeps growing past the 64 entries it can upload
- **Severity**: LOW
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` — `let mut moved_surface_ids: Vec<u32> = Vec::new();` in `build_and_upload_instances` (≈393), filled in `instance_flags` (≈972-976). The consumer is `SceneBuffers::upload_moved_surface_ids` (`scene_buffer/upload.rs`), which keeps `RESTIR_MAX_MOVED_SURFACES` (64).
- **Status**: NEW
- **Description**: Every draw with `instance_moved` runs `moved_surface_ids.contains(&sid)` and then `push`.
  `instance_moved` is true when the rigid model changed, or when the entity is in `pose_dirty`, which covers every
  animated skinned part on every frame. The other buffers in the instance loop are persistent `self.scratch.*`, but
  this one is rebuilt from empty each frame and regrows every time. Dedup is O(movers²). Every entry past the 64th
  is collected and compared only to be dropped by the upload. It also has no `ScratchTelemetry` (`ctx.scratch`) row.
- **Evidence**: `if !moved_surface_ids.contains(&moved_sid) { moved_surface_ids.push(moved_sid); }` (`instance_flags`); `let kept = moved_surface_ids.len().min(RESTIR_MAX_MOVED_SURFACES as usize);` (`upload_moved_surface_ids`).
- **Impact**: Render-thread CPU every frame with movers. Derived: ~150 movers (30 animated NPCs × ~5 parts) cost
  ≈ 11 k compares + ~6 reallocations. ~500 movers (a dense city, where REN-D2-01 already overflows the cap) cost
  ≈ 125 k compares, tens of µs. No quantitative guard exists for this site.
- **Related**: AUDIT_RENDERER_2026-10-27 REN-D2-01 (the 64 cap overflows at ~10 NPCs, plus a per-frame warn). This is
  the same producer, and REN-D2-01's capacity decision fixes the bound. SAFE-2026-10-27-01 (the buffers are not
  destroyed).
- **Suggested Fix**: Move the list onto `self.scratch` as a persistent buffer that is cleared per build, deduplicated
  through an `FxHashSet<u32>` scratch or a sort+dedup before upload, and capped at whatever capacity REN-D2-01
  settles on. Add a `ScratchTelemetry` row.

### PERF-D5-01: the underwater god-ray march inverts `inv_view_proj` per pixel, and the march grew from ALU-only to 144 depth taps per pixel without a measurement
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/composite.frag` — `underwaterGodRays` (`mat4 viewProj = inverse(params.inv_view_proj);`) and `underwaterSunVisibility`, added by 99475136b / baf2b78fd
- **Status**: NEW
- **Description**: The skill names a shader-side per-invocation `inverse()` of the camera matrix as the regression
  (`inv_vp` is computed once on the CPU). This is now the only such call in the shader tree; the others are
  `transpose(inverse(m3))` normal transforms and a volumetrics basis. The composite UBO carries only
  `inv_view_proj`, so every submerged pixel recomputes a 4×4 inverse (~150 flops). The larger, unmeasured cost is
  structural: each of the 24 march steps now calls `underwaterSunVisibility`, and each call does 6 ×
  (`viewProj * vec4` + one `depthTex` fetch). That is 144 depth fetches + 144 mat4·vec4 per submerged pixel, on top
  of the two value-noise evaluations per step.
- **Evidence**: `composite.frag` `underwaterGodRays`: `mat4 viewProj = inverse(params.inv_view_proj); … for (int i = 0; i < steps; ++i) { … sum += focus * underwaterSunVisibility(p, toSun, sunDirection, sunPath, viewProj) * k * dt; }` with `const int steps = 24;`, and `underwaterSunVisibility`'s `for (int i = 0; i < 6; ++i)` taps. Gated on `params.depth_params.x > 0.5 && params.underwater.w > 0.0 && params.sun_dir.w > 0.0`.
- **Impact**: Zero above water. Below the surface, derived: 1440p native ≈ 3.7 M px × 144 ≈ 530 M depth fetches per
  frame. The skill's inventory row says this march is unmeasured and that `composite_ms` must be quantified below
  the surface before its cost is filed. **That cost half is deferred to the runtime audit**; only the per-pixel
  `inverse()` is filed here as statically certain.
- **Related**: AUDIT_RENDERER_2026-10-27 REN-D8-01 (focus-noise pop) and REN-D8-02 (depth through a linear sampler)
  concern the same function. #6415 and #6418 are also open on it.
- **Suggested Fix**: Carry the forward `view_proj` in the composite params (the CPU already has it) and drop the
  `inverse`. Then measure `composite_ms` submerged (FNV / Skyrim lake, `--rt-test-ray-quality-tier` pinned), and
  only after that decide whether the 6-tap contact test needs fewer taps, early-out on the first blocked tap, or a
  half-resolution march.

### Existing / still present
- **#6248** (MEDIUM): each grass-blade fragment recomputes the root ground colour (~18 bindless fetches).
  **Amplified this window**: c9433d501 made the temperate species' blades 1.25× taller and 1.35× wider (~1.69×
  projected area, with the same candidate budget), so the fragment count that #6248's per-fragment cost multiplies
  grows by about that factor. The commit records no `groundcover_blade_draw_ms` A/B. Append this to #6248 rather
  than filing a new issue.
- **#5959** (MEDIUM): the Whiterun shadow-filter cost is still open.
- **#6313**, **#6315**, **#6316**, **#6341** (LOW): still present in code, not re-filed.

### Checked, no finding
- `apply_pending_group_swaps` / `sequence_player`: no allocation on a quiet frame. While a swap is pending, the walk
  over all `AnimationPlayer`s per pending object is small and transient. The lock-order cycle there is ECS-01.
- Physics water `collect_water_surfaces` clones `WaterBedField` per plane per step; it is an `Arc<[f32]>`, so this is cheap.
- Geometry admission tiers: every scene-geometry upload site in `byroredux/src` picks the right class. Object, tree
  and placement LOD and `spawn/mesh_instance.rs` (which includes FO4 precombines) are `Prop`, actor bodies in
  `scene/nif_loader.rs` are `Actor`, and terrain, terrain LOD and water are `Essential`.
- #6309 `step_interior_forced_weather`: both install arms go through `install_environment`, so the pending check
  settles after one install and the environment is not re-installed every frame.
- In `triangle.frag`, the #6250 `movedSurfaceSetContains` scan is ≤ 64 warp-uniform words and runs only for pixels
  that had a previous occluder and no hit-arm taint. It is unmeasured and inside `main_render_ms`.
- The exterior water mesh now follows the 128-unit LAND lattice (4a56fd9c7), about 4× the vertices per cell. Negligible.
- #6367 `SubReader` decode: a refactor whose author verified byte-identical output on Skyrim.esm NVNM. No allocation was added.
- Origin: `rebase_model_matrix` runs only in the O(instances) loop and the frame-params camera pair.
  `origin_corrected_prev_view_proj` is present.

## Prioritized Fix Order
1. **PERF-D4-01** together with REN-D2-01 (same producer). Make the list a persistent scratch, add bounded dedup, add a telemetry row. Small, local change.
2. **PERF-D1-01** together with ECS-02 / CONC-01 (same loop). Build the owner→bodies map once per scan, which also shortens the guarded region.
3. **PERF-D5-01**. Pass the forward VP in the composite UBO (this needs the shader-struct lockstep), then run the submerged `composite_ms` A/B in the runtime audit.
4. **#6248** re-measure. Run `groundcover_blade_draw_ms` before and after c9433d501 on a Skyrim exterior.

## Stale skill premises
- The Dim 5 / inventory text says a shader-side `inverse()` of the camera matrix is the regression. It is now
  present in `composite.frag` `underwaterGodRays` (PERF-D5-01). The composite row's "24-step per-pixel underwater
  god-ray march" undercounts: each step does a 6-tap depth contact test (`underwaterSunVisibility`), so the march
  is 144 depth fetches per pixel.
- The Dim 4 checklist does not mention the #6250 moved-surface buffer (scene set binding 25,
  `upload_moved_surface_ids`, 260 B per FIF, rewritten every frame). It should get a line and a `memory-budget.md` row check.
- Other numbers are confirmed: `VERTEX_POOL_HARD_CAP` 7 M / `INDEX_POOL_HARD_CAP` 28 M,
  `DRAW_SORT_PARALLEL_THRESHOLD` 3000, `SKINNED_BLAS_REFIT_THRESHOLD` 600 + jitter 60, `QUERIES_PER_FRAME` 60,
  30 brackets, `BENCH_GPU_KEYS` 24.

Next step: `/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-10-27.md`
