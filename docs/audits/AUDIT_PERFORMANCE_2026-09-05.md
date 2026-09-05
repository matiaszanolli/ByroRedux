# Performance Audit — 2026-09-05

- **Scope**: `/audit-performance --focus 1,3` (Dimension 1 — CPU Per-Frame
  Allocations & Hot Paths; Dimension 3 — GPU Memory Pressure & Eviction
  Thrash), `--depth deep`. Run as part of `/audit-suite volumetrics-deep`,
  motivated by recent volumetric-lighting work — the froxel grid
  injection/integration compute passes (`volumetrics_inject.comp`,
  `volumetrics_integrate.comp`) — so both dimension agents were given an
  explicit special-focus brief on that subsystem's CPU dispatch/upload path
  (Dim 1) and GPU memory footprint/eviction interaction (Dim 3), in addition
  to their standard checklists.
- **Orchestration**: two dimension Task agents (`renderer-specialist`) run
  concurrently, each writing `/tmp/audit/performance/dim_{1,3}.md`,
  consolidated here per Phase 3 of the skill.
- **HEAD**: `6fba2b0a`, branch `main`, 2026-09-05.
- **Dedup baseline**: `gh issue list --state all` dump at
  `/tmp/audit/performance/issues.json` (65 open, cross-checked against
  closed issues in each dimension's own search), plus the prior
  `AUDIT_PERFORMANCE_2026-08-30.md` full-sweep report for guard/finding
  continuity.
- **Method**: static analysis + read-only `git`/`grep`/source reading. No
  engine process was launched (*feedback_no_parallel_engine_launch*) and no
  bench was run this session — see the delta section below.
- **Dimensions not run**: 2, 4–9 are out of scope for this `--focus 1,3`
  invocation and are not covered here. Do not infer clean/dirty status for
  them from this report.

---

## Observed-vs-ROADMAP bench delta

**Not measured this session.** ROADMAP's current Bench-of-record is the
stepped-camera refresh at HEAD `2da754e7` (2026-09-03), explicitly current
and non-stale (unlike the earlier `R6a-stale-*` block it superseded), so a
future full-dimension sweep should diff against it directly. This run's
scope (Dimensions 1 and 3 only, static analysis) does not produce a
comparable FPS/frame-time number, and none of the findings below change
steady-state GPU pass cost enough to be separable from that matrix's own
run-to-run noise — the highest-impact finding here (`PERF-D3-01`) is a
budget-accounting gap that fails *open* on the 12 GB dev card (no observed
regression today) rather than a measured slowdown. No FPS figure is
manufactured.

---

## Executive Summary

**9 findings, all NEW: 0 CRITICAL, 0 HIGH, 3 MEDIUM, 6 LOW.**

| Dimension | Findings | Severity breakdown |
|---|---|---|
| 1 — CPU Hot Paths | 5 | 1 MEDIUM, 4 LOW |
| 3 — GPU Memory Pressure | 4 | 2 MEDIUM, 2 LOW |

No cross-dimension duplicates — the two agents worked disjoint files/lines.
All Session 46/75/76 regression guards checked under Dimension 1's remit are
**intact**; all four Dimension 3 prior-audit items from 2026-08-30 were
re-verified, and three of the four are now **closed** (fixed since that
sweep) — only the `shrink_tlas_to_fit` stale-comment item is still open, and
is re-reported here with exact current line numbers.

**Volumetrics special focus verdict**: the froxel grid's own allocation
lifecycle (sizing formula, create/destroy ordering on resize, no per-frame
reallocation, doc/code lockstep tests) is clean on both the CPU and GPU
sides — no leak, no thrash, no per-frame recompute. The two real defects
the special focus surfaced are less about the grid itself and more about
what it does to *neighboring* subsystems: (1) the CPU-side per-frame upload
of its companion fog-cluster buffers is enormously over-broad relative to
the data that changes (`PERF-D1-01`), and (2) the static BLAS eviction
budget has no way to see the grid's resolution-scaled VRAM footprint at all
(`PERF-D3-01`) — the most severe finding in this report.

---

## Hot Path Analysis

| Site | Per-frame cost | Class | Finding |
|---|---|---|---|
| `VolumetricsPipeline::dispatch` fog cluster/index/volume upload | ~176 KB uncached WC writes (fog-bearing frame); ~12 KB (fog-free frame) for <1 KB of meaningful data | CPU→GPU upload amplification | PERF-D1-01 (MEDIUM) |
| `append_combustion_surface_lights` moment decode | 256-bin decode (2,048 word reads) + 8,192 B memset, unconditional, discarded when `!had_grid` | Wasted CPU work | PERF-D1-02 (LOW) |
| `scene_has_effect_soft_material` | O(all `Material`) + O(all `ParticleEmitter`) scan, every `build_render_data` call | Ungated per-frame scene scan | PERF-D1-03 (LOW) |
| `frame_lights_scratch` mem::take | 0→`MAX_LIGHTS` regrow, only on 4 rare error-exit paths | Capacity-amortization break (error path only) | PERF-D1-04 (LOW) |
| `scene_trigger_actor_approach_system` | 1 `Vec` clone + 2 fresh `HashSet`s per running `SCEN`, per frame | Per-frame allocation | PERF-D1-05 (LOW) |
| BLAS residency budget vs. volumetrics/fixed-floor VRAM | Budget derived once at init from `heap/3`; never re-derived against the ~1.10–2.32 GB resolution-scaled floor (froxel grid ~183–730 MB of it) or a later resize | Structural budget blind spot | PERF-D3-01 (MEDIUM) |
| Mid-batch BLAS eviction accounting | `static_blas_bytes` decremented at queue-push, real free lags 2 frames behind and only ticks inside `draw_frame`, not the streaming (`about_to_wait`) path that runs `build_blas_batched` | Eviction accounting error, not a leak | PERF-D3-02 (MEDIUM) |
| `shrink_tlas_to_fit` docs | Doc-only drift, pre-#2929 "destroys the slot" claim survives in 2 places | Doc rot (safety-adjacent) | PERF-D3-03 (LOW) |
| `compute_blas_budget` doc placement | Doc comment orphaned onto `build_instance_map`; "VRAM / 3" phrasing pre-dates #3043's heap-selection fix | Doc rot | PERF-D3-04 (LOW) |

Volumetrics froxel-grid allocation lifecycle itself (sizing, create/destroy
ordering, resize behavior, doc/code lockstep tests, leak surface) produced
**no findings** — see the per-dimension "special focus" sections below for
the full checked-clean list.

---

## Findings

### Dimension 1 — CPU Per-Frame Allocations & Hot Paths

#### PERF-D1-2026-09-05-01: `VolumetricsPipeline::dispatch` re-uploads the full 176 KB fog cluster/index/volume staging set to write-combined memory every frame, to convey a few hundred meaningful bytes
- **Severity**: MEDIUM
- **Dimension**: CPU Hot Paths
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:2211-2225` (writes), `:1196-1227` (buffer creation), `:456-545` (`build_fog_volume_clusters`)
- **Status**: NEW
- **Description**: Every frame in which the scene has at least one local fog
  volume, `dispatch` performs three whole-array `write_mapped` calls into
  `MemoryLocation::CpuToGpu` buffers (write-combined, per `buffer.rs:998-1004`'s
  own doc comment):

  | Buffer | Bytes written per frame | Bytes actually meaningful |
  |---|---|---|
  | `fog_volume_buffers[frame]` (`GpuFogVolumeUpload`) | `16 + 128 × 96` = **12,304** | `16 + volume_count × 96` |
  | `fog_cluster_buffers[frame]` (`[GpuFogClusterEntry; 4096]`) | `4096 × 8` = **32,768** | 8 × number of clusters a volume touched |
  | `fog_cluster_index_buffers[frame]` (`[u32; 32768]`) | `32768 × 4` = **131,072** | 4 × `sum(entry.count)` |
  | **total** | **176,144 B/frame** | typically **< 1 KB** |

  The index buffer is the pathological one. `build_fog_volume_clusters`
  populates only `indices[entry.offset + i]` for `i < entry.count` — the doc
  comment at `:472-482` correctly argues the rest never needs *resetting* —
  but the code then uploads the whole 128 KB array regardless. With
  `FOG_VOLUME_CLUSTER_DIM = 16` over a `2 × grid_far_meters = 256 m` grid, one
  cluster cell is 16 m on a side, so a typical metre-scale flame/smoke volume
  touches 1–8 clusters — a 4,000–32,000× write amplification on the index
  buffer alone.
- **Evidence**:
```rust
// volumetrics.rs:2216-2225
if !fog_volumes.is_empty() {
    self.fog_cluster_buffers[frame].write_mapped(
        device,
        std::slice::from_ref(self.fog_cluster_entries.as_ref()),   // 32,768 B
    )?;
    self.fog_cluster_index_buffers[frame].write_mapped(
        device,
        std::slice::from_ref(self.fog_cluster_indices.as_ref()),   // 131,072 B
    )?;
}
```
  `write_mapped` (`buffer.rs:1256-1281`) is an unconditional
  `mapped[..len].copy_from_slice(&bytes[..len])` — no dirty-range notion. The
  same file's own #301 comment ("The instance SSBO is 1.28 MB but a typical
  frame writes only a few KB — flushing the full range wastes bandwidth")
  already establishes partial-range discipline as a recognised requirement;
  the volumetrics call sites just don't supply a bounded range. The
  unconditional 12,304-byte `fog_volume_buffers` write at `:2212-2215` also
  runs on the *empty* branch, where only the 16-byte `count` header is read
  by the shader (`fogVolumeCount == 0u` early-out, `:2183-2191`).
- **Impact**: ~176 KB of uncached/write-combined host writes per frame in any
  cell with fire, smoke, steam or an authored fog box; ~12 KB per frame
  everywhere else. At ~1.5–4 GB/s WC store throughput on a discrete part,
  the fog-bearing case is ~45–120 µs of pure `memcpy` per frame (0.3–0.7% of
  a 16.6 ms budget) for data that is almost entirely zeros — not a
  frame-killer today, but exactly the class of avoidable CPU cost this
  dimension targets on a 16-core part, and it scales cubically with any
  future `FOG_VOLUME_CLUSTER_DIM` bump.
- **Related**: #3133 (the `offset`-seeding fix that removed the per-frame
  *recompute* but left the per-frame *upload*), #301 (partial flush ranges),
  #2242 (the empty-branch `fogVolumeCount` invariant that makes the skip
  safe).
- **Suggested Fix**: Have `build_fog_volume_clusters` track the touched
  cluster-index extent (running min/max) and pass that range to a new
  `GpuBuffer::write_mapped_range`, so only the touched slice of `indices`/
  `entries` crosses the bus; bound the `GpuFogVolumeUpload` write to
  `16 + volume_count * size_of::<GpuFogVolume>()` at the same time.
- **Confidence**: High — static read of the write call sites and the buffer
  memory-location doc; no engine run needed to confirm the shape of the
  waste, though the exact µs figure is estimated from typical WC bandwidth,
  not measured.

#### PERF-D1-2026-09-05-02: `append_combustion_surface_lights` decodes and zeroes the whole 8 KB moment buffer before its own `had_grid` early-out, so every frame in every fog-free scene pays for it
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:2484-2505`; call site `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs:89-98`
- **Status**: NEW
- **Description**: The call site is unconditional — invoked whenever
  `self.volumetrics.is_some()`, i.e. every frame of every scene once the
  pipeline constructs, independent of whether `record_volumetrics_pass`
  actually dispatched. Inside, the function runs `invalidate_if_needed` →
  256-bin decode (`decode_combustion_light_moment`, 2,048 word reads) →
  8,192-byte `fill(0)` → `flush_if_needed`, and only *then* checks
  `if !had_grid { return Ok(0); }`. When no volumetrics dispatch ran for this
  slot, the decoded `moments` array is discarded unread and the buffer it
  just zeroed was already zero.
- **Evidence**:
```rust
// volumetrics.rs:2485-2505 — early-out at :2503, after the work
let had_grid = std::mem::take(&mut self.combustion_light_grid_valid[frame]);
let buffer = &mut self.combustion_light_moment_buffers[frame];
buffer.invalidate_if_needed(device)?;
let mut moments = [GpuCombustionLightMoment::default(); COMBUSTION_LIGHT_GRID_COUNT];
{
    let bytes = buffer.mapped_slice_mut()?;
    for (index, moment) in moments.iter_mut().enumerate() {   // 256 iterations
        *moment = decode_combustion_light_moment(&bytes[start..start + stride]);
    }
    bytes[..stride * COMBUSTION_LIGHT_GRID_COUNT].fill(0);    // 8,192 B
}
buffer.flush_if_needed(device)?;
if !had_grid {
    return Ok(0);
}
```
- **Impact**: A few microseconds per frame of decode + memset (the buffer is
  `HOST_CACHED` `GpuToCpu`, so reads are cheap — not a WC-read stall) plus up
  to two `vkInvalidateMappedMemoryRanges`/`vkFlushMappedMemoryRanges` calls,
  all for a discarded result. Small in absolute terms; flagged because it is
  unconditional across every game and every cell, and the fix is a two-line
  reorder.
- **Related**: PERF-D1-2026-09-05-01 (same file, same "unconditional
  whole-array traversal for a mostly-empty payload" shape).
- **Suggested Fix**: Hoist `if !had_grid { return Ok(0); }` above the
  invalidate/decode/fill block — the buffer's zero state is already
  maintained by construction and by every drain that did run.
- **Confidence**: High.

#### PERF-D1-2026-09-05-03: `scene_has_effect_soft_material` runs an ungated O(all `Material`) + O(all `ParticleEmitter`) scan at the head of every `build_render_data`
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/render/mod.rs:34-53`, called unconditionally at the top of `build_render_data`
- **Status**: NEW
- **Description**: Answers a scene-wide `bool` ("does any material or emitter
  carry `EFFECT_SOFT`?") by iterating every `Material` component and, if none
  matched, every `ParticleEmitter` component, every frame. `.any()`
  short-circuits on the *first* match, so the expensive case is the common
  one — a scene with none — which walks the full set to conclude `false`. On
  the FO4 InstituteBioScience baseline (~3,949 draw commands) that is a few
  thousand bitmask tests per frame plus two storage read-lock acquisitions,
  for a value that only changes when content loads/unloads.
- **Evidence**:
```rust
// render/mod.rs:34-53
fn scene_has_effect_soft_material(world: &World) -> bool {
    let mesh_materials_have_soft_effect = world.query::<Material>().is_some_and(|materials| {
        materials.iter().any(|(_, material)| {
            material.effect_shader_flags & ...EFFECT_SOFT != 0
        })
    });
    if mesh_materials_have_soft_effect { return true; }
    world.query::<ParticleEmitter>().is_some_and(|emitters| { ... })
}
```
  Called unconditionally from `build_render_data` before the scratch
  `clear()` block, with no caching and no dirty gate.
- **Impact**: A few tens of microseconds per frame in a dense cell; more
  significantly it's O(scene size) work inside a function whose stated design
  premise is caller-owned amortised scratch, and it scales with exactly what
  exterior streaming grows.
- **Related**: `byroredux/src/systems/bounds.rs:157-173` already demonstrates
  the correct pattern in this codebase (a `structural_generation()` key,
  full recompute only when it moves); #3477/#3475/#3142 are the same
  "rescan-every-tick to answer a rarely-changing question" family.
- **Suggested Fix**: Cache the flag against
  `(Material::structural_generation(), ParticleEmitter::structural_generation())`,
  recomputing only when a material/emitter is added or removed.
- **Confidence**: High.

#### PERF-D1-2026-09-05-04: the `frame_lights_scratch` `mem::take` round-trip has four error-path exits that leave the scratch at zero capacity
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs:86` (take) → `crates/renderer/src/vulkan/context/draw.rs:2154` (restore)
- **Status**: NEW
- **Description**: `assemble_camera_and_lights` does
  `let mut frame_lights = std::mem::take(&mut self.frame_lights_scratch);`,
  leaving the field a zero-capacity `Vec` until it's handed back ~2,000 lines
  later. Four `return Err(...)` sites sit between the two:
  `assemble_camera_and_lights.rs:235` (FSR frame-parameter failure) and
  `draw.rs:1945`/`:1997`/`:2036` (`end_command_buffer` / `reset_fences` /
  submit). On any of those the taken `Vec` drops and amortised capacity is
  lost, forcing a 0→`MAX_LIGHTS` regrow next frame — the exact `mem::take`
  capacity-churn pattern this dimension's checklist names, on error paths
  rather than the steady state.
- **Evidence**: `grep -n "return Err" crates/renderer/src/vulkan/context/draw.rs`
  → 1945, 1997, 2036, all between the take (`draw.rs:1724`) and the restore
  (`draw.rs:2154`); plus the `return Err(e)` at
  `assemble_camera_and_lights.rs:235`.
- **Impact**: Bounded and rare — one regrow on the frame after a
  submit/fence/FSR error. The three `draw.rs` sites are exactly the paths
  #910 already hardened for a semaphore leak, so they're known-reachable in
  practice (swapchain churn), not theoretical.
- **Related**: #910 (same three recovery sites), #3694 (`ScratchTelemetry`,
  which reports `frame_lights_scratch` len/capacity and would make this
  observable).
- **Suggested Fix**: Replace the `mem::take` with a split-borrow
  (`let Self { frame_lights_scratch, volumetrics, .. } = self;`) so the field
  is never vacated, removing the invariant instead of documenting it.
- **Confidence**: High on the code shape; impact is intentionally scoped as
  minor since it only fires on already-rare error paths.

#### PERF-D1-2026-09-05-05: `scene_trigger_actor_approach_system` deep-clones every `ScenePlayer` into a fresh `Vec` each frame
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/cinematic.rs:414-431`; registered unconditionally at `byroredux/src/boot.rs:1020`
- **Status**: NEW
- **Description**: Opens with
  `players.iter().map(|(_, player)| player.clone()).collect()` into a fresh
  `Vec<ScenePlayer>`, then builds two fresh `HashSet`s
  (`HashSet<(u32,u16)>`, `HashSet<u32>`) from it — all per frame, discarded at
  tick end. Registered unconditionally (not env-gated like the M42 AI
  procedures), so it runs in every game/cell; it early-returns when the
  `ScenePlayer` storage doesn't exist, which is the saving grace on
  non-quest-scene content.
- **Evidence**:
```rust
// cinematic.rs:419-424
let players: Vec<byroredux_scripting::ScenePlayer> = {
    let Some(players) = world.query::<byroredux_scripting::ScenePlayer>() else {
        return;
    };
    players.iter().map(|(_, player)| player.clone()).collect()
};
```
  The clone-then-collect exists to release the storage read lock before
  taking `SceneRegistry` — legitimate reason for the copy, not for the fresh
  allocation: the same shape was already fixed with a persistent scratch for
  the AI-package systems under #2033/#3269/#3353.
- **Impact**: One `Vec` allocation + deep clone per running scene per frame,
  plus two `HashSet` allocations, on any cell where a SCEN has ever played.
  Zero cost on content without scenes. The two `HashSet`s are keyed on form
  ids (not a per-entity keyspace), so the #2923 std-hashing rule doesn't
  apply here — this is an allocation finding, not a hashing one.
- **Related**: #2033, #3269, #3353 — same family, same fix pattern.
- **Suggested Fix**: Hoist the `Vec<ScenePlayer>` and the two sets into a
  `make_scene_trigger_actor_approach_system()` closure (the `make_animation_system`
  #1372 pattern), reused via `clear()` + `extend`.
- **Confidence**: High.

### Dimension 3 — GPU Memory Pressure & Eviction Thrash

#### PERF-D3-2026-09-05-01: BLAS residency budget is a fixed fraction of the whole DEVICE_LOCAL heap, blind to the ~1.1–2.3 GB resolution-scaled floor the volumetric grid now dominates
- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs:694-741`, `crates/renderer/src/vulkan/acceleration/mod.rs:270-320`
- **Status**: NEW
- **Description**: `compute_blas_budget` probes the DEVICE_LOCAL heap that
  will back a BLAS result buffer and returns
  `blas_budget_for_heap(heap) = (heap / 3).max(MIN_BLAS_BUDGET_BYTES)`. It is
  called **once**, from `AccelerationManager::new`; `blas_budget_bytes` is
  never re-derived after that. The `/3` divisor is the entire model of
  "leave room for everything else" — a model written when "everything else"
  meant textures, vertex/index pools and a framebuffer. It now competes with
  a fixed, resolution-scaled floor `docs/engine/memory-budget.md` puts at
  **~1.10 GB at 1080p native and ~2.32 GB at native 4K**, of which the
  froxel grid alone is **~183 MB / ~730 MB** — that page's own words, "still
  the largest resolution-scaled allocation in the engine". On a 12 GB card
  the BLAS budget is ~4 GB; 4 GB (BLAS) + 2.32 GB (fixed floor) + peak
  textures + the vertex-pool cap over-subscribes the heap with no subsystem
  positioned to notice, because each measures against its own private
  ceiling. Worse for this run's motivation: the froxel grid **grows at
  runtime** (a window resize to a larger extent quadruples it), while
  `blas_budget_bytes` stays frozen at its init value — the engine can move
  ~550 MB into the froxel grid mid-session and the BLAS eviction threshold
  will not move a byte.
- **Evidence**:
```rust
// predicates.rs:697-700
pub(super) fn blas_budget_for_heap(heap_bytes: vk::DeviceSize) -> vk::DeviceSize {
    (heap_bytes / 3).max(MIN_BLAS_BUDGET_BYTES)
}
```
  `mod.rs:273` — `let derived_budget_bytes = compute_blas_budget(instance, device, physical_device)?;`
  is the sole call site; `blas_budget_bytes` is thereafter only read
  (`blas_static.rs:1029`, `1074`). Contrast `context/resize.rs:819-866`,
  which reallocates the froxel grid at the new render extent with no
  corresponding budget re-derivation anywhere in `recreate_swapchain`.
- **Impact**: On a 6 GB RT-minimum card at 1080p the arithmetic is 2 GB BLAS
  budget + 1.10 GB fixed floor + textures — the BLAS manager will happily
  fill its 2 GB and let the allocator OOM rather than evict, because from its
  own view it is under budget. Surfaces as an allocation error inside
  `build_blas_batched` (degrading to a missing BLAS, which #1793 already
  documents as having no recovery path) or a driver-side host-memory
  fallback plus a frame-time cliff. Not observable on the 12 GB dev GPU,
  which is why it has stayed invisible.
- **Related**: `docs/engine/memory-budget.md` "Volumetrics (M55)" +
  "VRAM Rough Budget"; #3117 (grid growth that never reached the ledger);
  #387 (the original `VRAM/3` rationale); #3043 (the heap-probe correction —
  fixed *which* heap is measured, not *what else* claims it).
- **Suggested Fix**: Subtract a computed fixed-floor reservation from
  `heap_bytes` before the `/3` — the resolution-scaled passes already know
  their own sizes (`FROXEL_BYTES_PER_SLOT`, `SVGF_BYTES_PER_PIXEL`,
  `CAUSTIC_BYTES_PER_PIXEL`, `RESERVOIR_STRIDE`) — and re-derive
  `blas_budget_bytes` at the end of `recreate_swapchain` so a resolution
  change moves the eviction threshold with it.
- **Confidence**: High on the code shape and arithmetic; the failure mode is
  inferred from the RT-minimum-hardware budget math, not reproduced on a
  6 GB card (not available in this environment) — flagged per the
  Speculative-Vulkan-caveat posture as a real but unreproduced-here risk.

#### PERF-D3-2026-09-05-02: mid-batch BLAS eviction credits itself bytes that are still resident — `pending_destroy_blas` only drains inside `draw_frame`, and `build_blas_batched` runs before it
- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs:1012-1114` (esp. `1071-1095`), `:355-440`; `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs:204-224`
- **Status**: NEW
- **Description**: `evict_unused_blas` decrements `static_blas_bytes` (and
  `total_blas_bytes`) the moment it moves a `BlasEntry` onto
  `pending_destroy_blas`, but the actual destroy + allocator free happens
  `DEFAULT_COUNTDOWN` (2) frames later in `tick_deferred_destroy`. That tick
  has exactly one caller — `sync_and_acquire_frame`, inside `draw_frame`,
  after the fence wait. `build_blas_batched` runs from the streaming path in
  `about_to_wait`, **before** the next `draw_frame` — so within one batch
  there is no tick at all. The eviction loop's own stop condition
  (`blas_over_budget(static_blas_bytes, pending_bytes, budget)`, line 1071)
  therefore evaluates against a number that has already deducted memory the
  GPU still holds, and the batch resumes allocating against phantom
  headroom. The deferral itself is correct and load-bearing (#1449 —
  freeing earlier would free memory an in-flight TLAS still references); the
  defect is the **accounting**, not the lifetime. There is no
  `pending_destroy_bytes` counter anywhere — `pending_destroy_blas_count()`
  (`blas_static.rs:164`) exposes an entry *count* only, though `BlasEntry`
  carries `size_bytes` right there, unqueried.
- **Evidence**:
```rust
// blas_static.rs:1078-1094
if let Some(entry) = self.blas_entries[idx].take() {
    self.total_blas_bytes = self.total_blas_bytes.saturating_sub(entry.size_bytes);
    self.static_blas_bytes = self.static_blas_bytes.saturating_sub(entry.size_bytes);
    ...
    self.pending_destroy_blas.push(entry, DEFAULT_COUNTDOWN);   // still resident
}
```
  Only tick site: `accel.tick_deferred_destroy(&self.device, alloc);` at
  `sync_and_acquire_frame.rs:224`, reached only through `draw_frame`.
- **Impact**: Worst case within a single mid-batch eviction cycle, true
  resident static-BLAS VRAM approaches `2 × blas_budget_bytes`: evict a full
  previous-cell set on paper, build a fresh set to budget, with neither
  generation freed until the next frame. The 90% mid-batch trigger
  (`should_evict_mid_batch`) makes this reachable only on genuinely large
  multi-cell bursts — the same synchronous-burst regime #1793 already flags,
  so this compounds a known-bad path rather than opening a new one. It also
  makes the `BLAS eviction: freed N entries (X MB)` log line overstate what
  was actually reclaimed at that instant.
- **Related**: #1449 (the deferral this sits on top of), #1792 (the
  `pending_bytes` fold — verified intact, not regressed, at
  `blas_static.rs:1026-1032`/`1071-1077`), #1793 (synchronous multi-cell
  burst, documented-not-fixed), PERF-D3-2026-09-05-01.
- **Suggested Fix**: Track a `pending_destroy_bytes` running total alongside
  the queue (incremented on push, decremented in the tick destroyer) and
  fold it into `blas_over_budget`'s first argument, so a batch cannot spend
  headroom it hasn't actually reclaimed yet. Surface it next to
  `pending_destroy_blas_count()` for `ctx.scratch`.
- **Confidence**: High — verified both call sites and the single-tick-owner
  shape by reading the source directly.

#### PERF-D3-2026-09-05-03: `shrink_tlas_to_fit` still carries the pre-#2929 "the slot is destroyed outright" prose, contradicted by its own body 25 lines later
- **Severity**: LOW
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/memory.rs:155-158`, `:321-323`
- **Status**: NEW (re-confirmation of prior-audit item `PERF-D3-2026-08-30-04`; still open, no tracking issue found)
- **Description**: #2929 changed `shrink_tlas_to_fit` from destroy-now to
  record-intent (`tlas_shrink_pending[slot_index] = true`, actually performed
  later by `ensure_tlas_state`'s allocate-then-swap). Two comments still
  describe the old behaviour, one inside the *same doc block* as the
  correction:
  - `memory.rs:155-158`: "The slot is destroyed outright; the next
    [`Self::build_tlas`] call sees `tlas[slot_index].is_none()`..." —
    contradicted by lines 180-185 and 205-231 of the same function.
  - `memory.rs:321-323`: "Slot was destroyed (e.g. by `shrink_tlas_to_fit` on
    the previous tick)" — explicitly corrected by `shrink_tlas_scratch_to_fit`'s
    own doc at lines 262-266 ("**Not** produced by `shrink_tlas_to_fit` since
    #2929").
  Verified against live code: the function returns `true` after only setting
  the pending flag and logging; it never `take()`s the slot. Reserve floors
  are intact — `WORKING_SET_FLOOR` (8192) clamps the shrink target at
  `memory.rs:199`, `MIN_TLAS_INSTANCE_RESERVE` still pads the build path.
- **Evidence**: `memory.rs:232-242` — `let old_max = slot.max_instances;
  self.tlas_shrink_pending[slot_index] = true;` then `true`. No `take()`, no
  destroy.
- **Impact**: Documentation only, but on a `# Safety`-adjacent doc block
  governing TLAS lifetime (an `unsafe fn`). A maintainer trusting the stale
  text could conclude `tlas[slot]` can be `None` after a shrink and
  reintroduce the exact dangling-descriptor hazard #2929 removed (scene
  set-1 binding 2 naming a destroyed `VkAccelerationStructureKHR`, not
  `PARTIALLY_BOUND`, statically used by `triangle.frag`).
- **Related**: #2929 / CON-D1-01; #2915 / REN-D1-03; prior-audit
  `PERF-D3-2026-08-30-04`.
- **Suggested Fix**: Delete the two stale sentences; the #2929 block at
  `memory.rs:205-231` already states the real contract.
- **Confidence**: High.

#### PERF-D3-2026-09-05-04: the `compute_blas_budget` doc comment is orphaned onto `build_instance_map`, and its "`VRAM / 3`" phrasing survived #3043
- **Severity**: LOW
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs:271-314`, `:477`
- **Status**: NEW
- **Description**: Lines 271-276 open with "Compute the BLAS memory budget
  as `VRAM / 3` with a 256 MB floor. … See #387." — but line 277 continues
  the *same* `///` run with "Build the shared `draw_idx → ssbo_idx`
  mapping…", so the whole block is one doc comment attached to
  `build_instance_map` (line 298). The real `compute_blas_budget` sits 430
  lines further down at 707 with a different, correct doc. Separately, the
  phrasing is now inaccurate: #3043 deliberately changed the derivation from
  "sum of device-local heaps" to "the specific DEVICE_LOCAL heap backing a
  BLAS-usage buffer, selected by `memory_type_bits`", precisely to avoid
  summing aliased heaps or mistaking a small BAR aperture for main VRAM.
  "`VRAM / 3`" (line 271) and "the budget itself is VRAM/3" (line 477, in
  `should_evict_mid_batch`) both re-assert the superseded model.
- **Evidence**: `predicates.rs:271` `/// Compute the BLAS memory budget as
  \`VRAM / 3\`…` immediately followed at `:277` by `/// Build the shared
  \`draw_idx → ssbo_idx\` mapping that`, with no intervening item. First
  `pub fn` after the block is `build_instance_map` at `:298`.
- **Impact**: Documentation only, but it's the class of drift the project's
  own path/symbol gate exists to catch, and it puts a wrong VRAM model in
  front of the exact audience reading the eviction predicates for
  PERF-D3-2026-09-05-01.
- **Related**: #3043, #387, #3824 (`STATIC_BLAS_FLAGS` doc naming a deleted
  function — same file family, same drift class).
- **Suggested Fix**: Move lines 271-276 down to `compute_blas_budget` at
  `:707` (merging with the doc already there); reword both it and `:477` to
  "one third of the BLAS-capable DEVICE_LOCAL heap (#3043)".
- **Confidence**: High.

---

## Regression-Guard Verification

### Dimension 1 — all INTACT, no re-proposals

| Guard | Issue | Status |
|---|---|---|
| `PackedStorage::drain_dirty_into` (transform + world-bound propagation) | #1371 | INTACT |
| `make_animation_system` persistent scratches | #1372 | INTACT |
| `make_billboard_system` `last_cam` early-out | #1374 | INTACT |
| `build_debug_ui_snapshot` visibility gate | #1376 | INTACT |
| `SkinSlotPool::next_slot` idle-sweep contraction | #1379 | INTACT |
| `bone_world` steady-state reuse (no per-frame clear) | #1794 | INTACT |
| `emit_particles` dead `GlobalTransform` probe removal | #1803 | INTACT |
| Hot-path `FxHash` rule across render + skinning | #2923/#3051/#3045 | HOLDING |

The four LOW findings from the prior 2026-08-30 sweep are all **fixed and
holding**: animation-path hashmaps → `FxHashMap` (#3677), `reemit_water_planes`
early-out (#3678), `apply_cell_region_ambient` caching (#3679), lock-tracker
`held_others` ordering (#3680).

### Dimension 3 — 3 of 4 prior items now closed

| Prior finding | Status now |
|---|---|
| `PERF-D3-2026-08-30-01` — `MorphSlot::delta_buffer` per-entity, no cap | **CLOSED** — fixed by #3661 (shared `Arc<MorphDelta>` cache, idle-slot eviction, telemetry, ledger row) |
| `PERF-D3-2026-08-30-02` — 80% DEVICE_LOCAL warning has one caller | **CLOSED** — now 4 call sites (init + 3 streaming/transition points), `Once` latch moved into renderer context |
| `PERF-D3-2026-08-30-03` — memory-budget.md doc rot (MAX_LIGHTS, stride, pool multiplicity) | **CLOSED** — all three now verified matching code, one pinned by a `const` assert |
| `PERF-D3-2026-08-30-04` — stale `shrink_tlas_to_fit` "destroys the slot" comments | **STILL OPEN** — re-reported as `PERF-D3-2026-09-05-03` |

`#1792` (mid-batch eviction `pending_bytes` fold), `#1793` (two
documented-not-fixed gaps), LRU victim ordering, scratch-shrink reserve
floors, `MeshRegistry` pool caps, BGSM/BGEM half-eviction, `NifImportRegistry`
LRU, and the deferred-destroy countdown were all independently re-verified
clean this session — no CRITICAL early-free of in-flight GPU memory exists.

---

## Prioritized Fix Order

**Tier 1 — the volumetrics-motivated findings (do these first, per this run's brief)**

1. **Re-derive `blas_budget_bytes` against the resolution-scaled fixed floor**
   (`PERF-D3-2026-09-05-01`) — subtract froxel/SVGF/caustic/reservoir sizes
   from the heap before the `/3`, and recompute at the end of
   `recreate_swapchain` so a resize moves the threshold with it. This is the
   only MEDIUM-or-above finding with a real (if unreproduced on the 12 GB
   dev card) failure mode.
2. **Bound the fog cluster/index/volume uploads to their touched range**
   (`PERF-D1-2026-09-05-01`) — track min/max touched cluster index in
   `build_fog_volume_clusters`, add `GpuBuffer::write_mapped_range`, skip the
   12 KB `fog_volume_buffers` write's dead bytes on the empty branch.

**Tier 2 — cheap, local, measurable**

3. Track `pending_destroy_bytes` and fold it into `blas_over_budget`
   (`PERF-D3-2026-09-05-02`) — a running counter alongside an existing queue,
   no new call sites.
4. Hoist the `had_grid` early-out above the combustion-moment decode/memset
   (`PERF-D1-2026-09-05-02`) — two-line reorder.
5. Cache `scene_has_effect_soft_material` against a structural-generation key
   (`PERF-D1-2026-09-05-03`) — same pattern already used by
   `make_world_bound_propagation_system`.
6. Split-borrow `frame_lights_scratch` instead of `mem::take` across the
   error-path window (`PERF-D1-2026-09-05-04`).
7. Hoist `scene_trigger_actor_approach_system`'s per-frame `Vec`/`HashSet`s
   into closure-captured scratch (`PERF-D1-2026-09-05-05`).

**Tier 3 — doc hygiene (cheap, and these are the premises the next audit will trust)**

8. Delete the two stale `shrink_tlas_to_fit` "destroys the slot" sentences
   (`PERF-D3-2026-09-05-03`).
9. Move the misattached `compute_blas_budget` doc block off
   `build_instance_map` and correct its "VRAM / 3" phrasing
   (`PERF-D3-2026-09-05-04`).

---

## Appendix — Volumetrics Special-Focus Checklist (full detail)

**CPU side (Dimension 1) — checked, clean apart from findings 01/02 above**:
- Dispatch sizing matches the live config: `froxel_extent`
  (`volumetrics.rs:600-612`) derives the grid from the *render* extent with
  `div_ceil(config.froxel_xy_divisor).max(1)` and `config.froxel_z_slices`;
  `dispatch` sizes both `cmd_dispatch` calls from the same cached
  `self.extent`. `VolumetricsConfig::DEFAULT`
  (`crates/renderer/src/vulkan/upscaling.rs:125-149`) is
  `{froxel_xy_divisor: 8, froxel_z_slices: 64, grid_far_meters: 128}`, and
  `DEFAULT_GRID_FAR_METERS` derives from it rather than repeating a literal
  (#3611 holds). No stale-constant divergence.
- No per-frame recomputation of froxel extents/config — `froxel_extent` has
  exactly two call sites, both construction-time.
- No `Vec`/`HashMap`/`String` allocation inside `dispatch` — all CPU staging
  is preallocated `Box`ed fixed arrays; `combustion_light_candidates` is a
  persistent field cleared/refilled with `sort_unstable_by`.
- No O(mesh-count) work in the volumetrics CPU path — every loop is
  O(volumes ≤ 128), O(clusters = 4096), O(bins = 256) or O(lights).
- `collect_fog_volumes` uses caller-owned scratch, `sort_unstable_by`,
  covered by `ScratchTelemetry` (#3694). No finding.

**GPU memory side (Dimension 3) — checked, clean apart from finding 01 above**:
- Sizing formula keys on `frame_extents.render`, not output — matches the
  doc's explicit FSR-overspend warning.
- `FROXEL_VOLUMES_PER_SLOT = 6`, `FROXEL_BYTES_PER_SLOT = 44`, allocated once
  per `MAX_FRAMES_IN_FLIGHT`; matches `docs/engine/memory-budget.md`'s
  ledger row exactly, pinned by a doc-reading regression test
  (`froxel_grid_cost_matches_the_memory_budget_doc`).
- No per-frame reallocation — `create_volume` has exactly 8 call sites, all
  inside `new_inner`.
- Resize destroys the old pipeline before constructing the new one, under
  `device_wait_idle` — no transient double residency of a 183–730 MB
  structure.
- `VolumetricsConfig` is CLI-only (no console/settings mutator), so there's
  no repeated-realloc path; an upscaler-preset switch routes through
  `recreate_swapchain`, so the grid can't go stale-sized against a changed
  render extent.
- `destroy` drains every volume vector, both noise volumes, frees every
  `gpu-allocator` allocation, destroys all buffers/pipelines/pools/samplers.
  Cell transitions don't touch volumetrics at all — no leak found.
- Every froxel image goes through `allocator.lock().allocate(...)` with
  `GpuOnly`, so it *is* visible to `generate_report()` and the 80%
  DEVICE_LOCAL warning.
