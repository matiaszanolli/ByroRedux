# #4784: PERF-D5-2026-09-23-02: When any transport emitter is active, every froxel in the grid runs the RK2 backtrace, the six-neighbour gather and the curl forcing, including the empty majority

**Severity**: MEDIUM
**Labels**: medium, performance, renderer, shaders, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23.md (PERF-D5-2026-09-23-02)

- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp`:
  - `main` → `transportCombustion` (`:2594`);
  - the RK2 block `:2005-2078`;
  - the `combustionActivity < 0.08` neighbour gather `:2007-2018` → `incomingDynamicsFromNeighbors` `:1770-1828`;
  - the midpoint/source probes `:2026`, `:2039`;
  - the `dt > 0` curl/wind forcing `:2309-2337`, computed and then scaled by `activity` (0 on empty froxels).
- **Status**: NEW. This is the residual of #3131: that fix zeroes `dt` only when *nothing* in the scene transports.
- **Description**:
  - `simulationDt` is one scalar for the whole dispatch. With `dt > 0`, an empty froxel (the overwhelming majority) does all of the following:
    - samples its own history (3 fetches);
    - fails the `< 0.08` activity test the "wrong" way, so it runs the six-neighbour gather (6 × `samplePreviousTransport` = 18 fetches; `carriesCombustion` rejects each neighbour only *after* its three samples);
    - takes a midpoint and a source sample (6 fetches);
    - evaluates two `curlField`s (12 `sin`/`cos`) and 2–3 `atmosphericWindVelocity` `sin` calls, all multiplied by `activity = 0`.
  - Total: 27 trilinear RGBA16F 3D fetches and 9 reprojections (mat4·vec4 + `length` + log slice mapping) per quiet froxel, against 3 fetches at `dt = 0`.
  - A plume occupies a tiny fraction of the grid, and its transport support is bounded: `MAX_COMBUSTION_SPEED_MPS` 28 m/s × the `dt` clamp of 1/15 s is 1.87 m per step, far below one 16 m fog cluster.
- **Evidence**:
  - *est.*: +24 fetches × 921,600 froxels = +22 M trilinear fetches/frame at a 720p render extent (FSR Quality at 1080p output), and +50 M at 1080p render.
  - At the 4070 Ti's texture rate that is ≥ 0.16 ms ideal at 1080p. Latency-bound reality is plausibly 0.3–0.6 ms.
  - Bench corroboration (1280×720 output, FSR-Q, 410,880 froxels): Whiterun BanneredMare (hearth → Flame volumes; `fog.rs:595` names its `FlamesSmall03-Emitter`) is 0.83 ms (≈ 2.0 ns/froxel). Prospector is 0.155 ms (≈ 0.38 ns/froxel). The scenes differ in more than transport, so this is corroboration, not attribution. PERF-D8-01's split bracket would make it attributable.
- **Impact**:
  - A tier-invariant cost on every frame of every cell with a fire in view, which covers most Skyrim interiors and many exteriors (Markarth's braziers).
  - It scales linearly with render resolution: roughly 4× at native 4K versus 1080p.
  - The adaptive controller cannot shed it, because only `volumetric_light_cap` is tiered.
- **Related**: #3131, PERF-D5-01, REN-D8-2026-09-23-03 (the frozen residual keeps `transportedMediumActive` froxels paying `transportedCombustionTransmittance`: 8 steps × (1 + lights) fetches).
- **Suggested Fix**:
  1. Add a world-space coarse occupancy mask on the existing 16³ fog-cluster grid. Mark a cell when the inject pass writes `carriesCombustion` (atomicOr into a small SSBO read next frame, one frame behind like the moment buffer), and OR in the clusters that hold a transported source volume (the CPU already knows them).
  2. Dilate by one cell (16 m ≫ 1.87 m/step).
  3. In the shader, skip the whole `hadHistory && dt > 0` block for froxels whose cell and its 26 neighbours are empty, falling through to the `dt = 0` carry.
  4. Hoist the curl/wind forcing under `activity > 0` in any case.
  5. Measure with the PERF-D8-01 split bracket before and after, at a pinned `--rt-test-ray-quality-tier`.
- **Confidence**: High on the structure (read from GLSL). Medium on the magnitude (arithmetic plus cross-scene bench, not an A/B).

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-23.md` (`/audit-suite --preset volumetrics-deep`, HEAD `2237da9c3`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related passes (other compute passes / history buffers / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
