# Performance Audit — 2026-08-20

**Scope**: `/audit-performance` — all 9 dimensions, `--depth deep`, run as part of
the `comprehensive` 25-audit sweep.

**Repo state**: HEAD `bb0b92f2`, branch `main`. 335 commits since the previous
sweep's baseline `85b77371` (2026-08-16), overwhelmingly session-70 WATAL water,
volumetric combustion transport, terrain-LOD streaming, and SpeedTree wind
sharing. Dedup baseline: the 400-issue all-state fetch at `/tmp/audit/issues.json`
plus `docs/audits/AUDIT_PERFORMANCE_2026-08-16.md` and its predecessors.

| Dim | Area | Findings |
|---|---|---|
| 0 | Skill-text / bench hygiene | 1 LOW |
| 1 | CPU Per-Frame Allocations & Hot Paths | 1 MEDIUM · 2 LOW |
| 2 | Draw & Instancing | 1 LOW |
| 3 | GPU Memory Pressure | **1 HIGH** |
| 4 | SSBO Sizing & Per-Frame Upload | 1 MEDIUM |
| 5 | GPU Pipeline & Pass Efficiency | 1 MEDIUM |
| 6 | Skinning & BLAS Cost | **clean** (guards intact; #3061 carried) |
| 7 | Streaming & Cells | 1 LOW |
| 8 | NIF Parse | **clean** (#3062 carried) |
| 9 | Telemetry & Origin Cost | **clean** |

**0 CRITICAL · 1 HIGH · 3 MEDIUM · 5 LOW.**

---

## Executive Summary

**Every regression guard this skill names is still intact.** All 26 guards
re-verified at their cited symbol (list at the end). No 08-16 finding regressed;
three of the six (#3058, #3059, #3060) are CLOSED and their fixes are present,
three (#3061, #3062, #3063) remain OPEN and are carried, not re-reported.

**The delta's cost is concentrated in two places, and only one of them is water.**
Session 70's water work is largely well-engineered on the hot path — `param_scratch`
is clear+extend, the water UBO upload is O(live draws), `MAX_WATER_DRAWS`
truncation is `Once`-gated, and the froxel/caustic dispatches are correctly
O(froxels)/O(pixels) and correctly gated (`requires_dispatch`, the caustic
`meshIdRaw & 0x80000000` early-out). The real cost sits in the **volumetric
combustion field** that landed alongside it: four new full-froxel-grid 3-D images
per frame slot that the authoritative VRAM ledger does not count, and a per-froxel
fluid-transport stencil with no scene-level "is anything burning?" gate.

**The single HIGH is a ledger error, not a leak.** `docs/engine/memory-budget.md`
— which this skill mandates as the sole authority for VRAM ceilings — models the
volumetrics grid as **2** RGBA16F volumes per FIF slot. The code creates **6**
(the pipeline's own log line says so: `5×RGBA16F + R32F`). Actual is 44 B/froxel
per slot, not 16. At the default `froxel_xy_divisor: 4` that is **730 MB at
1080p** against 265 MB documented, and **2.92 GB at 4K** against 1.06 GB. The
budget's own summary ledger row is worse still — it still carries the
pre-Session-62 fixed-grid figure (29.5 MB / 118 MB), contradicting the section
two hundred lines above it.

**No bench was run**, deliberately (suite rule 4 + `feedback_no_parallel_engine_launch.md`).

### Observed-vs-ROADMAP delta

None to report, and that is unchanged from 08-16: the LIVE Bench-of-record is now
**369+ commits** past its own 30-commit gate (it was 34 at the last sweep). That
is **#3063**, already filed and OPEN — not re-reported here. Every magnitude in
this report is derived from checked-in constants, struct layouts and dispatch
sizing, never from a manufactured FPS delta. No absolute FPS figure appears
anywhere below.

### Hot-path cost table (derived from checked-in constants, not sampled)

| Signal | Source | Value |
|---|---|---|
| `WaterMaterial` size | `crates/core/src/ecs/components/water.rs` | 63 fields / 433 B raw → **436 B** (was 18 fields / 104 B at `85b77371`) |
| `WaterContact` size | same, `:564` | ≈480 B (embeds `Option<WaterMaterial>`) |
| Froxel grid @1080p | `froxel_extent` × `VolumetricsConfig::default` (`upscaling.rs:115`) | 480×270×64 = **8,294,400** froxels |
| Froxel bytes/slot | 5×RGBA16F + R32F (`volumetrics.rs:532,538,543`) | 44 B/froxel → **365 MB**; ×2 FIF = **730 MB** |
| Fog cluster grid | `FOG_VOLUME_CLUSTER_DIM = 16` (`shader_constants_data.rs:438`) | 4096 entries (32 KB) + 32768 indices (128 KB), re-uploaded per frame |
| `MAX_WATER_DRAWS` | `crates/renderer/src/vulkan/water.rs:172` | 186 (UBO-range-derived, not an observed count) |
| `DRAW_SORT_PARALLEL_THRESHOLD` | `byroredux/src/render/mod.rs:561` | 3000 — unchanged, still correctly placed vs the baselines |
| `STREAMING_APPLY_BUDGET` | `byroredux/src/app_step.rs:33` | **16 ms** (was 4 ms at 08-16 — deliberate, see PERF-D0-01) |

Per-pass GPU cost was **not** sampled: `gpu_timers.rs` / `ScratchTelemetry` are
runtime-only and need a live Vulkan device plus on-disk game data. Where a
finding below concerns GPU work it states the *structure* of the waste (which
invocations pay, and why) and explicitly flags that the `gpu_timers` bracket was
not read.

---

## Findings

### PERF-D3-01: `memory-budget.md` counts 2 of the volumetrics pipeline's 6 froxel volumes — real VRAM is 2.75× the documented figure, and the summary ledger row is a further 9× low

- **Severity**: HIGH
- **Dimension**: GPU Memory Pressure
- **Location**: `docs/engine/memory-budget.md:228-256` (section) and `:467`
  (summary ledger row); ground truth at
  `crates/renderer/src/vulkan/volumetrics.rs:740-796` (six `Vec<FroxelSlot>`
  fields), `:905-990` (all six pushed once per `MAX_FRAMES_IN_FLIGHT`),
  `:532/538/543` (formats), `:1601-1612` (the pipeline's own log line)
- **Status**: NEW
- **Description**: The Volumetrics section states *"Two volumes per frame
  (lighting + integrated) × 2 FIF"* and derives its whole table from
  `… × 8 B × 2 volumes × 2 FIF`. The pipeline actually allocates **six**
  full-extent 3-D images per FIF slot: `lighting_volumes`,
  `integrated_volumes`, `combustion_state_volumes`,
  `combustion_dynamics_volumes`, `combustion_optical_volumes` (all
  `R16G16B16A16_SFLOAT`, 8 B/froxel) and `emission_history_volumes`
  (`R32_SFLOAT`, 4 B/froxel) — **44 B/froxel/slot**, not 16. The four
  uncounted volumes are the combustion-transport field; the last three landed
  in this delta (`0ff7b537` → `4a35819e`).

  Separately, the **summary ledger** at `:467` still reads
  `| Volumetrics froxel grid (2 volumes, 2 FIF) | ~29.5 MB (1080p) | ~118 MB (4K) |`
  — the pre-Session-62 *fixed* 160×90×128 grid figure. The document therefore
  contradicts itself by ~9× between its own section and its own ledger, and both
  numbers are below the truth.
- **Evidence**: `volumetrics.rs:1601` logs
  `"… {} MiB / slot (5×RGBA16F + R32F) …"` computing
  `w*h*d*44 / (1024*1024)` — the code already knows the correct per-slot figure
  and prints it at boot. Recomputed against `VolumetricsConfig::default`
  (`crates/renderer/src/vulkan/upscaling.rs:113-118`,
  `froxel_xy_divisor: 4`, `froxel_z_slices: 64`):

  | Render res | Grid | Documented (§Volumetrics) | Ledger row `:467` | **Actual (44 B × 2 FIF)** |
  |---|---|---:|---:|---:|
  | 1920×1080 | 480×270×64 | 265.4 MB | 29.5 MB | **730 MB** |
  | 2560×1440 | 640×360×64 | 471.9 MB | — | **1.30 GB** |
  | 3840×2160 | 960×540×64 | 1061.7 MB | 118 MB | **2.92 GB** |

  The pipeline is created unconditionally (`context/mod.rs:2447`, failure only on
  a Vulkan error), so this is resident in every session, not an opt-in feature.
- **Impact**: The `**Estimated total** | **~1.59 GB**` row at `:475` is short by
  ~700 MB at 1080p — the real typical figure is ~2.29 GB, not 1.59 GB. At 4K the
  volumetrics grid alone (2.92 GB) consumes ~73% of the stated `< 4 GB target`
  before textures, BLAS, or the vertex pools are counted, so the peak column no
  longer describes a reachable configuration. Because this skill's Dimension 3
  explicitly forbids re-deriving ceilings (*"Do NOT re-derive memory ceilings —
  `docs/engine/memory-budget.md` is the authoritative source"*), every future
  audit and every sizing decision that cites the doc inherits the error. FSR
  presets shrink the render extent and therefore the grid (Performance ≈ ¼ the
  froxels), which is the mitigation the doc should state — it currently states
  the *undercount* instead.
- **Related**: #2679 (`PERF-D3-03`, CLOSED) — same class of ledger omission,
  32 B/px, rated MEDIUM; this one is 28 B/froxel across a 8.3 M-froxel grid.
  #2242 (`REN-D16-04`, CLOSED) touched the same file's fog-volume path.
- **Suggested Fix**: Rewrite `memory-budget.md:228-256` to enumerate all six
  volumes with their formats and derive from 44 B/froxel/slot; update the
  ledger row at `:467` and the estimated-total row at `:475`; state the FSR
  render-extent mitigation explicitly. Then consider whether the three
  combustion fields need to be allocated at all when no `FogVolume` with a
  transport profile has ever existed in the session — a lazily-created
  combustion sub-group would return ~400 MB at 1080p to scenes that never see
  fire (see PERF-D5-01, which is the runtime half of the same observation).

---

### PERF-D5-01: `volumetrics_inject.comp` runs the full combustion transport stencil on every froxel with no scene-level "combustion present" gate — and its most expensive branch is gated on *low* activity, so the quiet majority pays it

- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline & Pass Efficiency
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp:2324-2334`
  (unconditional `transportCombustion` call in `main`), `:1729-1760`
  (`transportCombustion`'s `hadHistory && dt > 0.0` branch), `:1666-1720`
  (`incomingDynamicsFromNeighbors`), `:1392-1412` (`samplePreviousTransport`);
  CPU side `crates/renderer/src/vulkan/volumetrics.rs:2038-2040`
  (`frame_params.fog_reference[3] = simulation_dt`), `:2449-2467`
  (`requires_dispatch`)
- **Status**: NEW
- **Description**: `main()` calls `transportCombustion` for **every** froxel,
  unconditionally. Inside, once temporal history is valid (steady state), the
  RK2 advection block runs, and its first act is:

  ```glsl
  if (combustionActivity(probeChemistry, probeOptical) < 0.08) {
      ... incomingDynamicsFromNeighbors(worldPos, stepX, stepY, stepZ, ...)
  }
  ```

  `incomingDynamicsFromNeighbors` loops six neighbours, each calling
  `samplePreviousTransport` = **three trilinear `texture()` fetches on RGBA16F
  3-D images**. So the 18-fetch neighbour gather fires precisely on the froxels
  with *no* combustion activity — which, in a scene with no fire at all, is
  100% of them. Adding the destination probe and the midpoint/source probes,
  a quiet froxel pays ~21 3-D texture fetches plus 3 `imageStore`s for a field
  that is uniformly zero.

  There is no CPU-supplied "combustion is active in this scene" signal in
  `VolumetricsParams`, even though the CPU already computes exactly that
  predicate: `requires_dispatch` evaluates
  `has_transport_emitter(fog_volumes)` and maintains
  `combustion_active_until_seconds`. The pass itself is correctly gated
  (`has_global_medium || !fog_volumes.is_empty() || linger`), but
  `has_global_medium` is true for any cell with authored fog — i.e. the common
  case — so the dispatch runs and the combustion stencil runs with it.
- **Evidence**: at the default `froxel_xy_divisor: 4` / `froxel_z_slices: 64`,
  a 1920×1080 render extent gives 480×270×64 = **8,294,400** froxels. 21
  trilinear RGBA16F 3-D fetches each is ~1.7×10⁸ fetches/frame, against three
  8-B `imageStore`s per froxel (~199 MB of writes/frame) — all for
  `chemistry == 0`. `carriesCombustion(...)` rejects each neighbour *after* its
  three samples have already been issued (`:1686-1693`), so the early-out saves
  the TLAS query but not the bandwidth.
- **Impact**: Wasted 3-D texture bandwidth and L2 pressure on every frame of
  every fog-bearing cell in every game, scaling linearly with render resolution
  (4K = 33.2 M froxels, 4× the above). It is not a correctness problem and it
  does not compound, but it is paid in the frames the project most cares about
  (dense exteriors), and the gate that would remove it already exists on the CPU.
- **Confidence / limits**: the *structure* above is read directly from the
  shipped GLSL and is not in doubt. The *magnitude* is arithmetic from the
  froxel count and fetch count — the `volumetrics` `gpu_timers` bracket was
  **not** read (runtime-only, no engine instance spawned per
  `feedback_no_parallel_engine_launch.md`). Quantify with
  `bench-stats --break-down` before and after any fix rather than trusting the
  estimate.
- **Related**: PERF-D3-01 (the VRAM half of the same subsystem); #2242.
- **Suggested Fix**: One line on the CPU: in
  `VolumetricsPipeline::dispatch`, set `frame_params.fog_reference[3]` (the
  shader's `simulationDt`) to `0.0` when
  `!has_transport_emitter(fog_volumes) && now > self.combustion_active_until_seconds`.
  `transportCombustion` already gates its entire RK2 block on `dt > 0.0`
  (`:1750`), so the neighbour gather, the midpoint probe, the source probe and
  the differential all fall away with no shader change. Pin it with a unit test
  on the predicate.

---

### PERF-D4-01: the local-fog cluster rebuild is O(grid capacity), not O(live volumes) — 160 KB zero-filled, re-offset and re-uploaded every frame for as little as one volume

- **Severity**: MEDIUM
- **Dimension**: SSBO Sizing & Per-Frame Upload
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:380-401`
  (`build_fog_volume_clusters` prologue), `:2079-2088` (the two uploads);
  consumer `crates/renderer/shaders/volumetrics_inject.comp:519-536, 612-622`
- **Status**: NEW
- **Description**: Every frame in which at least one `GpuFogVolume` is visible,
  `build_fog_volume_clusters` unconditionally does:

  ```rust
  entries.fill(GpuFogClusterEntry::default());   // 4096 × 8 B  = 32 KB memset
  indices.fill(0);                                // 32768 × 4 B = 128 KB memset
  for (cluster_index, entry) in entries.iter_mut().enumerate() {
      entry.offset = (cluster_index * MAX_FOG_VOLUMES_PER_CLUSTER) as u32;
  }                                               // 4096-iteration loop
  ```

  and the caller then `write_mapped`s **both whole arrays** (160 KB) into
  `CpuToGpu` host-visible memory, regardless of how many volumes were actually
  clustered. Two of the three steps are provably dead work:

  1. `entry.offset` is a pure function of `cluster_index` and never varies. It
     could be written once at pipeline construction and never touched again.
  2. `indices.fill(0)` is unnecessary: the shader reads
     `fogClusterIndices[cluster.offset + i]` only for
     `i < min(cluster.count, MAX_FOG_VOLUMES_PER_CLUSTER)`
     (`volumetrics_inject.comp:615-618`), and additionally rejects
     `volumeIndex >= fogVolumeCount` (`:619`). Slots past `count` are never
     observed.
- **Evidence**: `FOG_VOLUME_CLUSTER_DIM = 16`
  (`crates/renderer/src/shader_constants_data.rs:438`) →
  `FOG_VOLUME_CLUSTER_COUNT = 4096`; `MAX_FOG_VOLUMES_PER_CLUSTER = 8`
  (`:441`) → `FOG_VOLUME_INDEX_COUNT = 32768`. Both staging arrays are `Box`ed
  fixed-size arrays (`volumetrics.rs:784-785`) — correctly *allocated* once, but
  fully *rewritten and re-uploaded* per frame. Per frame that is ~160 KB of
  memset plus ~160 KB of write-combined PCIe traffic plus a 4096-iteration
  scalar loop, for a `volume_count` that `MAX_GPU_FOG_VOLUMES` already bounds
  well below the grid capacity. Fog volumes are *not* rare: `fog.rs:317`
  (`fog_volume_from_particle`), `:420` (`explosion_volume_from_particle`),
  `:474` (`fire_volume_from_particle`) and `:616` (`fog_volume_from_mesh`)
  classify ordinary torch / lantern / campfire / smoke emitters, so the
  non-empty branch is the normal case in a lit interior.
- **Impact**: A fixed per-frame CPU + upload tax proportional to grid capacity
  rather than scene content. Not a leak and not a hitch, but it is exactly the
  pattern Dimension 4's checklist names ("a full-capacity memcpy of a near-empty
  buffer is the waste"), and unlike most such cases here the read bound is
  already enforced shader-side, so the fix cannot change rendering.
- **Related**: #2242 (`REN-D16-04`, CLOSED) documented why the *empty* branch may
  skip these uploads; this finding is the non-empty branch's mirror image.
- **Suggested Fix**: (a) initialise `entry.offset` once in
  `VolumetricsPipeline::new` and drop the per-frame loop; (b) delete
  `indices.fill(0)`; (c) touch only the clusters a volume's AABB actually
  intersects — reset `count` for those via the same range walk that fills them,
  or keep a small `Vec<u32>` of touched cluster indices from the previous frame
  and clear just those. (a) and (b) are safe, mechanical, and independently
  testable.

---

### PERF-D1-01: `apply_buoyancy`'s quiesced-scene fast path is unreachable in any cell containing water — the authored-wave gate is true for the default 0.05 amplitude

- **Severity**: MEDIUM
- **Dimension**: CPU Hot Paths
- **Location**: `crates/physics/src/water.rs:484-511` (the `waves_active`
  computation and the fast path it disarms), `:551-575` (the
  O(all-rigid-bodies) `targets` build it guards); default at
  `crates/core/src/ecs/components/water.rs:347` (`wave_amplitude: 0.05`)
- **Status**: NEW (introduced by `6b960349`, 2026-08-20, in this delta)
- **Description**: `apply_buoyancy` carries a deliberately-engineered
  quiesced-scene fast path — the WATAL §0 "exterior freeze" contract — whose
  own comment says *"With nothing awake, nothing pending, and no newcomer this
  frame, no body moved since the last buoyancy eval, so the per-body scan is
  pure waste."* `6b960349` added a fourth term:

  ```rust
  let waves_active = time_secs.is_some()
      && surfaces.iter().any(|s| s.material.wave_amplitude.abs() > 1.0e-4);
  ...
  if pw.awake_counts().0 == 0 && !pw.pending_wake() && !had_newcomers && !waves_active {
      return;
  }
  ```

  `WaterMaterial::default().wave_amplitude` is **0.05**, four orders of
  magnitude above the `1.0e-4` epsilon, and the WATAL sentinel comment at
  `water.rs:344-346` states it is deliberately the value *"a record that omits
  wave data resolves to … across all games"*. So `waves_active` is true for
  effectively every water surface ever spawned, and the fast path is dead code
  in exactly the scenes it was written for.
- **Evidence**: with the gate disarmed, every frame in a water cell runs the
  full body of `apply_buoyancy`: `collect_water_surfaces` allocates a fresh
  `Vec<WaterSurface>` whose element embeds a 436 B `WaterMaterial` by value
  (`:355-375`); `collect_water_current_volumes` allocates a second fresh Vec
  (`:378-383`); then `targets: Vec<Target> = Vec::new()` is built by walking
  **every** `RapierHandles` entity with a `RigidBodyData` probe and a
  `WaterContact` probe each (`:551-575`) — the full rigid-body set, not the wet
  subset. None of the three Vecs is a reused scratch.
- **Impact**: The static-scene step fast-path in `PhysicsWorld::step` still
  engages, so the *solver* stays cheap; what is lost is the buoyancy phase's own
  O(all bodies) prologue, every frame, in every water-bearing cell, including a
  fully settled one where the correctness motivation (a wave crest wetting a
  body at the waterline) cannot apply to any body not already adjacent to the
  surface. There is **no quantitative guard for this site** — per this skill's
  Regression-Guard Posture, `dhat` is a process singleton and the live engine
  loop has no allocation-bound coverage.
- **Related**: PERF-D1-03 (the `WaterContact` half); #2871 (`PHYS-D6-02`, OPEN,
  the same function's wake gate); #2880 (`PHYS-D3-05`, OPEN, phase-2.5 docs).
  The suite's ECS audit surfaced the `WaterContact` allocation independently;
  this is the CPU-cost framing of the same call.
- **Suggested Fix**: Narrow the term rather than deleting it. A wave crest can
  only change a body's wetness if the body is within one wave amplitude of a
  surface, and every such body was wet or borderline last frame — so gate on
  *"any resident `WaterContact` with `submerged_fraction > 0` or a prior depth
  inside the crest band"* instead of *"any surface has waves"*, and hoist
  `surfaces` / `current_volumes` / `targets` into persistent scratch on
  `PhysicsWaterConstants` (or a small `BuoyancyScratch` resource) reused via
  clear+extend, matching the `AnimScratch` (#1372) pattern.

---

### PERF-D1-02: the delta's new per-frame water and vegetation collections landed on std SipHash and re-allocate every frame

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/water.rs:10, 356, 367-369, 435`
  (`make_water_interaction_system`); `byroredux/src/systems/billboard.rs:9, 36,
  144, 153` (`make_billboard_system`'s `geometry_bases`)
- **Status**: NEW — both sites landed in this delta (`5959bbb8`, 2026-08-19;
  `6096f19f`, 2026-08-20)
- **Description**: `_audit-common.md`'s hot-path hashing rule (#2923) requires
  the per-frame render/skinning path to be `FxHashMap`/`FxHashSet` end-to-end.
  The guard it names —
  `pose_dirty_crosses_the_crate_boundary_without_siphash`
  (`crates/renderer/src/vulkan/context/mod.rs:4402-4423`) — **is intact**, and
  every `SkinSlotPool` collection, `FrameInputs.pose_dirty`, and the
  `skin_offsets` map in `byroredux/src/render/` is still Fx. The two collections
  added this session did not follow the rule:

  | Site | Collection | Per-frame behaviour |
  |---|---|---|
  | `systems/water.rs:356/367` | `wet_last_frame` / `wet_now`: `std HashSet<EntityId>` | `wet_now` is built fresh each frame and then `wet_last_frame = wet_now` (`:435`) — the prior set's capacity is dropped, so a scene with wet bodies regrows 0→N every frame. This is the `mem::take` capacity-churn shape #1371 fixed for `PackedStorage`. |
  | `systems/water.rs:369` | `ripple_by_surface`: `std HashMap<EntityId, …>` | fresh `HashMap::new()` per frame. |
  | `systems/water.rs:368` | `entries: Vec::new()` | fresh per frame. |
  | `systems/billboard.rs:36` | `geometry_bases: std HashMap<u32, Quat>` | one SipHashed `.entry()` per SpeedTree geometry entity per frame, **plus** an unconditional `retain` (`:153`) that walks the whole map and issues two sparse-set `contains` probes per live tree, every frame. |
- **Evidence**: `byroredux/src/systems/water.rs:10` —
  `use std::collections::{HashMap, HashSet};`; `billboard.rs:9` — same for
  `HashMap`. Both crates already depend on `rustc-hash`
  (`_audit-common.md` §Hot-path hashing). Note that `billboard.rs`'s camera-motion
  gate (#1374, `:90`) is `last_cam == … && !wind_active && !wind_state_changed`
  — `wind_active` is true whenever `WindField.speed > 1.0e-4`, so in any weathered
  exterior the gate never fires and the `retain` is paid unconditionally.
- **Impact**: Small in absolute terms and bounded by live-tree / wet-body counts,
  and neither set is DoS-facing. The cost is (a) SipHash on a per-frame
  per-entity keyspace, (b) an allocation-per-frame shape in the water set, and
  (c) an O(live trees) prune that only ever needs to run on despawn. The larger
  cost is epistemic, exactly as #3061 argued: the #2923 guard makes the per-frame
  path *read* as Fx-hashed while newly added siblings are not.
- **Related**: #3061 (`PERF-D6-01`, OPEN) — the renderer-side siblings, explicitly
  scoped to the skinning path and therefore not covering these; #3045
  (`REN-D9-01`, OPEN); #1374 (the billboard gate, intact but bypassed by wind).
- **Suggested Fix**: Substitute `FxHashMap`/`FxHashSet` at all four sites; in
  `make_water_interaction_system` swap the two sets rather than move-assigning
  (`std::mem::swap(&mut wet_last_frame, &mut wet_now); wet_now.clear();`) so
  capacity survives; hoist `entries` / `ripple_by_surface` into closure-captured
  scratch reused via clear+extend; and drive `geometry_bases` pruning off cell
  unload (or run the `retain` only when the map's length exceeds the live
  `SpeedTreeWind` count) instead of every frame.

---

### PERF-D1-03: `WaterContact` grew 4.2× in the delta and is still round-tripped through a freshly-allocated `Vec` every frame per wet body

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `crates/core/src/ecs/components/water.rs:88-287` (`WaterMaterial`),
  `:564-590` (`WaterContact`); `crates/physics/src/water.rs:581` (the
  `writes` Vec), `:722-734` / `:745` / `:759` (the pushes), `:802-810` (the drain)
- **Status**: NEW (quantification of a lead surfaced independently by this
  suite's ECS audit — verified against HEAD here from the performance angle)
- **Description**: `WaterMaterial` went from **18 fields / 104 B** at
  `85b77371` to **63 fields / 433 B raw → 436 B** at HEAD. It is `Copy` and
  embedded **by value** in three places on the hot path:
  `WaterPlane.material` (the ECS component), `WaterSurface.material` (the
  per-frame physics snapshot), and `WaterContact.material` as
  `Option<WaterMaterial>` — which makes `WaterContact` ≈480 B, up from ≈150 B.

  `apply_buoyancy` collects contacts into
  `let mut writes: Vec<(EntityId, WaterContact)> = Vec::new();` (`:581`),
  pushes a full ≈480 B value per wet-or-transitioning body (`:722`, `:745`,
  `:759`), then drains it into the storage after the `PhysicsWorld` write lock
  drops (`:802-808`). The Vec is freshly allocated every frame and dropped at
  the end, so a scene with N wet bodies pays one allocation plus 2×N×480 B of
  copy traffic per frame (once into the Vec, once into the storage).
- **Evidence**: field count and byte sum computed directly from the struct at
  HEAD (63 fields, 433 B raw, 4-byte alignment → 436 B) and at `85b77371`
  (18 fields, 104 B). The lock-ordering reason for the deferral is real and
  documented (`:579-580`) — this finding is about the *buffer*, not the
  deferral. `WaterMaterial` is also copied wholesale in
  `byroredux/src/render/water.rs:179` (`let mut mat = plane.material;`, per
  plane per frame, to apply the TOD blend) and compared/stored in
  `submersion_system`'s `best: Option<(f32, WaterMaterial)>`
  (`byroredux/src/systems/water.rs:147`).
- **Impact**: Bounded by the number of wet bodies, which is small in every cell
  measured so far — this is not a present-tense hot-path problem. It is a
  *trajectory* problem: the struct grew 4.2× in one session while remaining a
  by-value payload in a per-frame `Vec` and in a `SparseSetStorage` row, and
  there is no guard pinning its size the way `gpu_instance_layout_tests.rs`
  pins `GpuInstance` at 128 B. No quantitative allocation guard exists for this
  site.
- **Related**: PERF-D1-01 (same function, the O(all bodies) prologue); #2887
  (`PHYS-D6-04`, OPEN, `WaterContact::depth` semantics).
- **Suggested Fix**: Two independent moves, either sufficient. (1) Reuse the
  buffer: hoist `writes` (and `surfaces` / `current_volumes`) into persistent
  scratch cleared per frame. (2) Stop embedding: replace
  `WaterContact.material: Option<WaterMaterial>` with the `surface_entity` that
  is *already on the struct* plus a lookup, or with a small handle — the
  material is per-plane, not per-contact, and every consumer already has the
  plane entity in hand. Add a `size_of::<WaterMaterial>()` assertion so the
  next growth is a deliberate decision rather than a diff artifact.

---

### PERF-D2-01: `reemit_water_planes`' O(N_draws × W) linear scan rests on a "≤ ~3 water planes per cell" premise that mesh-bound water invalidated this session

- **Severity**: LOW
- **Dimension**: Draw & Instancing
- **Location**: `byroredux/src/render/water.rs:61-67` (the doc comment),
  `:147-152` (the scan); the invalidating spawn path at
  `byroredux/src/cell_loader/spawn/mesh_instance.rs:691, 724-748`
- **Status**: NEW
- **Description**: `reemit_water_planes` finds each water entity's already-emitted
  draw with `draw_commands.iter().position(|c| c.entity_id == entity)` and its
  doc justifies the O(N×W) cost as *"typical N is ~thousands of draws and W is
  ≤ ~3 water planes per cell, so this is well under a microsecond. A
  `HashMap<EntityId, usize>` would be premature for the expected scale."*

  That premise held while water came only from CELL `XCLW`/`XCWT`. It no longer
  does: `mesh_instance.rs:724-748` now spawns a `WaterPlane` (with its own
  436 B `WaterMaterial`) for **every mesh sub-shape whose material carries
  `is_water_shader`**, so a Skyrim/FO4 exterior with authored rivers,
  waterfalls and pond meshes can hold considerably more than three. The
  `LodWaterPlane` annulus (`streaming.rs:438`) adds one more.
- **Evidence**: `MAX_WATER_DRAWS = 186`
  (`crates/renderer/src/vulkan/water.rs:172`) is derived from the portable
  `maxUniformBufferRange` floor rather than an observed count, so it does not by
  itself prove W is large — but it is the ceiling the pass is built to tolerate,
  and the scan is O(N × W) up to it. Against the FO4 baseline's
  `bench_draws_cmds = 3440` (`.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv`),
  W = 40 would be ~138 k `entity_id` comparisons per frame.
- **Impact**: Sub-millisecond in every configuration reachable today, so this is
  a stale-justification finding rather than a measured regression. Reported
  because the comment now asserts a bound the code no longer enforces, and the
  next reader will trust it.
- **Related**: #1026 / F-WAT-05 (the no-resort contract this function depends on —
  intact, `water_commands_match_draw_slots` debug assert still in place).
- **Suggested Fix**: Either correct the comment to state the real bound
  (`MAX_WATER_DRAWS`, and that mesh water contributes), or — cheaper than a map —
  set `is_water` and capture the index during the existing static-mesh emit loop,
  which already visits every entity once, and have `reemit_water_planes` consume
  a small `Vec<(EntityId, u32)>` instead of rescanning.

---

### PERF-D7-01: `resident_vwd_refr_cells` takes a fresh storage read-lock per VWD entity inside the LOD reconcile loop

- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/streaming_helpers.rs:215-225`; called from
  `update_lod_coverage` (`:174`), which runs on every `reconcile_lod_rings`
  call including zero-budget ones (`:136-139`)
- **Status**: NEW
- **Description**: The helper holds a `world.query::<VisibleWhenDistant>()`
  handle but then reads the transform with
  `world.get::<byroredux_core::ecs::GlobalTransform>(entity)` **inside** the
  loop. `World::get` (`crates/core/src/ecs/world.rs:333-351`) is not a cheap
  probe: it does a `TypeId` map lookup, constructs a `lock_tracker::TrackedRead`
  scope guard, acquires and releases the storage `RwLock`, and unwinds the
  tracker — per entity. The accumulator is also a `std::collections::HashSet`
  built fresh and then `.into_iter().collect()`ed into a `Vec`.

  The function's own doc justifies the shape correctly ("querying \[the sparse
  marker\] first and looking up `GlobalTransform` per hit is cheaper than a
  joint query") — that reasoning is about *which* set to iterate, and is sound.
  It does not justify re-acquiring the `GlobalTransform` lock per hit rather
  than once outside the loop.
- **Evidence**: `world.get::<T>()` vs. the surrounding code's own convention —
  every sibling in `streaming_helpers.rs` and in `render/static_meshes.rs`
  acquires a query handle once and calls `.get(entity)` on it (see the #1377
  hoist at `static_meshes.rs:163`, verified intact).
- **Impact**: Confined to LOD-reconcile frames — `lod_reconcile_budget_for_frame`
  returns `None` once `lod_reconcile_pending` clears (`:44-46`), so this is
  **not** a steady-state per-frame cost. But reconcile frames are precisely the
  boundary-crossing frames whose hitch the streaming budget exists to cap, and
  VWD-flagged placements can number in the hundreds in a dense exterior.
- **Related**: #1377 / #1805 (the same hoist, applied on the render side).
- **Suggested Fix**: Hoist `let gq = world.query::<GlobalTransform>();` above the
  loop and use `gq.get(entity)`; make the accumulator an `FxHashSet` (this is a
  streaming-path, not a load-time parser, and the keyspace is entity-derived).

---

### PERF-D0-01: two of this skill's own Dimension checklists now cite superseded constants

- **Severity**: LOW
- **Dimension**: Telemetry & Origin Cost / skill hygiene
- **Location**: `.claude/commands/audit-performance/SKILL.md` Dimension 7
  (`STREAMING_APPLY_BUDGET` "4 ms") and Dimension 5 (`froxel_xy_divisor`
  "default 12"); ground truth `byroredux/src/app_step.rs:33`
  (`Duration::from_millis(16)`) and
  `crates/renderer/src/vulkan/upscaling.rs:115` (`froxel_xy_divisor: 4`)
- **Status**: NEW
- **Description**: Two numeric claims in this skill's dimension text no longer
  match the code an auditor is told to "verify intact":
  - Dimension 7 states `STREAMING_APPLY_BUDGET` is **4 ms**. `687e0a67`
    (2026-08-16, inside this delta) raised it to **16 ms**, deliberately and
    with a documented rationale at `app_step.rs:22-32` ("Four milliseconds
    proved counterproductive in the FO4 boundary gate"). The 08-16 audit
    verified 4 ms correctly; the change landed immediately after.
  - Dimension 5 states the volumetrics `froxel_xy_divisor` default is **12**.
    It is **4** (`VolumetricsConfig::default`), and has been since Session 62 —
    a 9× difference in froxel count, which is what made the Dimension 3 ledger
    error (PERF-D3-01) easy to under-weight.
- **Evidence**: both values read directly at HEAD; the 16 ms change is a
  documented design decision, not drift in the code.
- **Impact**: An auditor following Dimension 7 literally would report the 16 ms
  budget as a regression (it is not), and an auditor following Dimension 5 would
  size the froxel grid 9× low. This is the same failure mode #2691 (`PERF-DOC`,
  CLOSED) was filed for.
- **Related**: #2691 (CLOSED); #3063 (`PERF-D0-01` 08-16, OPEN — the
  bench-of-record staleness, a separate item, now 369+ commits out of gate).
- **Suggested Fix**: Update both figures in `audit-performance/SKILL.md` and
  re-run `.claude/commands/_audit-validate.sh`. Prefer phrasing that names the
  constant without transcribing its value, so the next tuning change cannot
  invalidate the skill text again.

---

## Prioritized Fix Order

1. **PERF-D3-01** — a doc edit, but it is the gate on every VRAM claim in the
   project and it is currently self-contradictory by 9×. Do it first; it is
   also the input that decides whether PERF-D5-01's lazy-allocation option is
   worth building.
2. **PERF-D5-01** — one CPU line (`fog_reference[3] = 0.0` when combustion is
   inactive) removes a per-froxel 18-fetch stencil from every fog-bearing frame,
   with no shader change and a trivially testable predicate.
3. **PERF-D4-01** — (a) hoist the constant `offset` init to construction and
   (b) delete `indices.fill(0)`. Both are mechanical, both are provably safe
   against the shader's existing `count` bound. Do them together.
4. **PERF-D1-01 + PERF-D1-03** — one change: narrow the `waves_active` gate and
   move `surfaces` / `current_volumes` / `targets` / `writes` onto persistent
   scratch. Same function, same commit.
5. **PERF-D1-02** — type substitution plus a set swap plus a retain gate. Widen
   the #2923 guard assertion to cover the new sites so this cannot recur.
6. **PERF-D0-01 / PERF-D2-01 / PERF-D7-01** — comment and hoist cleanups.

---

## Guards verified intact (do NOT re-propose)

Dimension 1: #1371 `drain_dirty_into` — **zero** production `take_dirty` callers
(`crates/core/src/ecs/packed.rs:61`, remaining references are tests) · #1372
`entities_scratch` / `playback_scratch` clear+extend
(`byroredux/src/systems/animation.rs:452-453, 539-540`) · #1374 billboard
`last_cam` (`byroredux/src/systems/billboard.rs:28, 90, 93`) · #1376 debug-UI
snapshot gate on `ui.visible || game_menu_visible()`
(`byroredux/src/app_frame.rs:61-71`) · #1379 `next_slot` contraction
(`crates/core/src/ecs/resources/skin_slot_pool.rs:318-335`, guard test
`sweep_contracts_next_slot_when_tail_is_freed`) · #1794 `bone_world` no-clear
(`byroredux/src/render/mod.rs:678`) · #1803 dead `GlobalTransform` probe still
absent from `byroredux/src/render/particles.rs` (zero occurrences).

Dimension 2: #1377/#1805 GT-presence hoist binding `transform` in one lookup
(`byroredux/src/render/static_meshes.rs:163`) · #1804/#2165
`needs_two_sided_blend_split` = `is_blend && b.two_sided &&
b.order_dependent_glass`, **no** `z_write` limb
(`crates/renderer/src/vulkan/context/draw.rs:1204-1207`) · #2682 self-swap guard
`if raster_len != index` (`byroredux/src/render/mod.rs:557`) ·
`DRAW_SORT_PARALLEL_THRESHOLD = 3000` applied to the raster prefix only
(`render/mod.rs:561-566`) — still correctly placed: only the FO4 baseline
(`bench_draws_cmds` 3440) crosses it.

Dimension 3: dynamic BLAS budget `(heap_bytes / 3).max(MIN_BLAS_BUDGET_BYTES)`
(`crates/renderer/src/vulkan/acceleration/predicates.rs:659`) · #1792
`blas_over_budget` folding `pending_bytes` (`predicates.rs:470-476`) ·
`BATCH_EVICTION_CHECK_INTERVAL = 64`, `MIN_TLAS_INSTANCE_RESERVE` /
`WORKING_SET_FLOOR` = 8192 (`acceleration/constants.rs:47-54, 74`) ·
`MeshRegistry` soft/hard caps + `check_pool_growth`
(`crates/renderer/src/mesh.rs:29-34, 70`) · #1430 half-eviction on the
BGSM/BGEM/failed-path caches (`byroredux/src/asset_provider/material.rs:716,
749, 836`) · `BYRO_NIF_CACHE_MAX` 2048 LRU
(`byroredux/src/cell_loader/nif_import_registry.rs:235-236`) ·
deferred-destroy countdown tied to `MAX_FRAMES_IN_FLIGHT`
(`crates/renderer/src/deferred_destroy.rs:32-33`).

Dimension 4: `MAX_INSTANCES = 0x40000`, `MAX_INDIRECT_DRAWS = MAX_INSTANCES`,
`MAX_MATERIALS = 16384` (`crates/renderer/src/vulkan/scene_buffer/constants.rs:139,
162, 191`) · `gpu_instance_is_128_bytes_std430_compatible`
(`scene_buffer/gpu_instance_layout_tests.rs:99-111`) · PBR resolved once —
`resolve_pbr` called only from `byroredux/src/material_translate.rs:391, 472`
and the two `commands/scene.rs` live-edit sites; zero production per-draw
`classify_pbr_keyword` (the `material.rs:997/1009` hits are test shims inside
`mod tests`) · water UBO upload is O(live): `param_scratch` clear+extend with
`.take(MAX_WATER_DRAWS)` and a `Once`-gated truncation warning
(`crates/renderer/src/vulkan/water.rs:527-547`).

Dimension 5: #1799 `ENABLE_LEGACY_WRS = 0` in both
`crates/renderer/src/shader_constants_data.rs:901` and the generated
`crates/renderer/shaders/include/shader_constants.glsl:238` · `invViewProj` is
CPU-side UBO data (`cluster_cull.comp:60`, `ssao.comp:24`); the only shader-side
`inverse()` calls are `triangle.frag:496` (the flag-gated non-uniform-scale
normal path) and `volumetrics_inject.comp:1467` — the latter is a genuinely
per-invocation `mat3` from per-froxel finite-difference steps, gated on
`dynamics.w > 1e-5 || activity > 0.01`, i.e. **not** the hoistable-constant
regression pattern · `froxel_extent` resolution-derived
(`volumetrics.rs:512-524`) · froxel dispatches are O(froxels)
(`volumetrics.rs:2214-2217, 2262-2264`) and the caustic dispatch is O(pixels)
(`caustic.rs:944-945`) with a per-thread `meshIdRaw & 0x80000000` early-out
before any ray query (`caustic_splat.comp:268`) · `requires_dispatch` gates the
whole volumetrics pass (`volumetrics.rs:2449-2467`) · GPU-timer readback gates on
`active_bits` and does not `WAIT` (`gpu_timers.rs:220-222, 378-385`).

Dimension 6: #1195 `pose_dirty: FxHashSet<EntityId>` and all five `SkinSlotPool`
collections `Fx` (`crates/core/src/ecs/resources/skin_slot_pool.rs:78-150`) ·
#2923 guard `pose_dirty_crosses_the_crate_boundary_without_siphash` present and
asserting both `FrameInputs.pose_dirty` and
`record_skinned_blas_refit`'s parameter
(`crates/renderer/src/vulkan/context/mod.rs:4392-4423`) · `skin_offsets` is
`FxHashMap<EntityId, u32>` across the whole `byroredux/src/render/` boundary
(`render/skinned.rs:77`) · #1791/#1796 `skin_dispatch_ran` rollback check after
the dispatch (`byroredux/src/app_frame.rs:483-498`).

Dimension 7: #877 two-phase `pre_parse_cell` with `PRE_PARSE_RAYON_MIN = 8`
(`byroredux/src/streaming.rs:1308-1309`) · batched exterior teardown through
`cell_loader::unload_cells` + `World::despawn_batch` · the sub-frame apply budget
seeds a `FrameTimeBudget::until` from `STREAMING_APPLY_BUDGET`
(`app_step.rs:179-180`) — **value changed 4→16 ms, see PERF-D0-01**.

Dimension 8: #833 `read_pod_vec` as the sole bulk reader
(`crates/nif/src/stream.rs:438`) · #831 `#[must_use]` present at 22 sites in
`stream.rs` · #832 zero `or_insert(name.to_string())` occurrences in
`crates/nif/src`.

Dimension 9: #1492–96 one `snap_render_origin` + one `look_at_rh` per frame
(`byroredux/src/render/camera.rs:182-185`) · #1489
`origin_corrected_prev_view_proj` (`crates/renderer/src/vulkan/context/draw.rs:2018`,
defined `:3878`) · `ScratchTelemetry` refreshed per frame
(`byroredux/src/app_frame.rs:148`).

---

## Existing OPEN issues touched (deduplicated, not re-reported)

#3061 (`PERF-D6-01` — renderer-side SipHash siblings on the skinning path;
re-verified still present, still scoped away from the new water/vegetation
sites) · #3062 (`PERF-D8-01` — `read_pod_vec`'s `vec![T::default(); count]`
pre-fill, confirmed unchanged at `crates/nif/src/stream.rs:449`) · #3063
(`PERF-D0-01` — bench-of-record past its gate; now 369+ commits) · #2881
(`PHYS-D3-06` — per-frame `env::var_os` probes; `crates/physics/src/sync.rs:104,
171` are still un-cached, which is exactly what that issue describes) · #2871,
#2880, #2887, #2888, #2889 (WATAL/physics) · #2776, #2779, #2780, #2787, #2763
(water/caustic) · #2766, #2782, #2821, #779.

## Known-open, deliberately not re-reported

- Interior cell load still calls `load_references` with
  `FrameTimeBudget::unlimited()`; the per-frame NPC-spawn budget remains
  exterior-only. Stated as open-for-interiors in this skill's Dimension 7 and
  recorded by the 08-16 sweep; unchanged.
- #1793 (missing rigid BLAS has no recovery; synchronous multi-cell burst can
  false-evict) and #1797 (shared `blas_scratch_buffer` serializes N dirty
  skinned entities) — documented-not-fixed, unreachable on the 12 GB dev card.
- `TextureRegistry` slots are grow-only by design
  (`docs/engine/memory-budget.md`), not a leak.
- The two-sided blend split is structurally dormant for engine-classified glass
  (#2691) — no batch-count movement may be attributed to it.

## Candidates investigated and dropped (so a later sweep does not re-derive them)

- **`caustic.rs`'s decay+splat double dispatch on a parked camera** — two
  full-screen dispatches instead of one clear+splat, but the splat shader exits
  after a single `texelFetch` on any non-transparent pixel
  (`caustic_splat.comp:268`), so a scene with no transparent geometry pays
  essentially nothing. Deliberate design (#2239/#2401).
- **`volumetrics_inject.comp:1467`'s `inverse(basis)`** — per-invocation, but
  `basis` is built from per-froxel `froxel_to_world` deltas (`:2311-2319`), so it
  is genuinely invocation-varying and cannot be hoisted to the UBO. Also gated on
  active combustion. Not the Dimension 5 regression pattern.
- **`block_hole_mask`'s `resident_full_cells.contains(&(gx, gy))`**
  (`byroredux/src/cell_loader/terrain_lod.rs:210`) — a linear `Vec` scan inside a
  16-cell mask assembly, so O(16 × loaded_cells) per block per reconcile attempt.
  Tens of thousands of `i32` pair comparisons on a reconcile frame; below the
  reporting floor, and the reconcile path is already budget-yielded.
- **`ChurnTracker::observe`'s fresh `HashSet` per call**
  (`byroredux/src/cell_loader/lod_coverage.rs:137-138`) and `find_overlaps`' O(n²)
  (`:53-63`) — both correctly documented as bounded by the ring radius, and both
  only reachable while `lod_reconcile_pending` is true.
- **`build_render_data`'s two `WeatherDataRes` resource acquisitions**
  (`byroredux/src/render/water.rs:82, 88`) — two lock cycles where one would do;
  no allocation, below the floor.
- **`water_damage_system`'s `.collect()`** (`byroredux/src/systems/water.rs:38-49`)
  and `submersion_system`'s `disturbance_events` (`:244`) — both `Vec::new()` on
  the common frame (no damaging water / no camera disturbance), which does not
  allocate.
- **`weather_system`** (`byroredux/src/systems/weather.rs:423+`) — allocation-free;
  `build_tod_keys` returns a fixed `[(f32, usize); 7]`.
- **`fog_volume_upload` / `combustion_light_candidates`** — `Box`ed fixed arrays
  and a `.clear()`ed Vec respectively; correctly bounded (the *upload* size is
  PERF-D4-01's subject, the allocation shape is fine).

## Scope note

`crates/mod-runtime`, `crates/facegen`, `crates/hkx`, `crates/debug-server` and
`crates/debug-protocol` were not examined — none has a per-frame path and none is
in this skill's dimension list. `crates/physics` and `byroredux/src/systems/` were
treated as in-scope for Dimension 1 because the delta put new per-frame work
there. No `cargo` command was run (suite rule 4) and no engine instance was
launched (`feedback_no_parallel_engine_launch.md`); every figure above is derived
from checked-in source, constants, and struct layouts.

TALLY: CRITICAL=0 HIGH=1 MEDIUM=3 LOW=5
