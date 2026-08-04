# Exterior readiness action plan

This is the execution plan for turning the existing exterior feature set into
a dependable, cross-game open-world path. It is deliberately organized around
runtime outcomes rather than milestones: selecting the right worldspace,
entering on valid ground, streaming across cell boundaries without hitches or
leaks, and preserving visual/world-system continuity at distance.

## Baseline and evidence

The foundation is already substantial and should be extended, not replaced:

- WRLD/CELL/LAND parsing, terrain splatting, persistent references, water,
  weather/sky/sun, async cell pre-parse, cooperative reference application,
  unload/reload, terrain/object/placement LOD, and interior/exterior door
  transitions are wired.
- `WorldStreamingState` maintains a load radius plus unload hysteresis and owns
  generation-based stale-payload rejection.
- Ambiguous grid selection is deterministic: preferred containing worldspaces
  win, then cell count and EDID provide stable fallbacks (`#2340`, covered by
  three `worldspace_selection_tests`).

Live validation on 2026-08-04 exposed the difference between "wired" and
"ready":

- FO3 `MegatonWorld`, grid `(-1,-7)`, radius 1 loaded 3,201 entities, 1,093
  draw commands, 412 meshes, and 218 textures. Lighting/sky values were finite,
  `tex.missing` reported no missing textures, and the previously recorded
  exterior white-out did not reproduce.
- The same worldspace at its nominal `(0,0)` CELL is a valid but empty dummy:
  8 entities and no terrain/static colliders. Before the first tranche, the
  loader treated this as a successful foreground handoff with a zero center
  and spawned a character that fell indefinitely. It now reports the authored
  content signals plus five deterministic alternatives and defaults to FlyCam;
  explicit `--player` remains an operator override.
- The populated run still had 19 failed NIF paths, primarily ambient FX and
  furniture markers. These are not core terrain failures, but they need a
  classified allowlist or real consumers so regressions do not hide inside an
  unbounded warning bucket.
- The first repeatable exterior smoke matrix now lives at
  `docs/smoke-tests/m-exteriors.sh`; a boundary-crossing latency benchmark is
  still missing. Earlier exterior coverage was split among audits, one
  SpeedTree smoke, and prose claims.

## Definition of done

An exterior profile is ready only when all of these hold:

1. The requested worldspace is selected deterministically and reported.
2. Foreground readiness means a renderable, terrain/reference-backed cell is
   coherent; an empty or missing center is diagnosed and cannot silently spawn
   a walking character into freefall.
3. The player can cross at least two cell boundaries while load/unload
   hysteresis, cancellation, terrain holes, persistent refs, water, weather,
   and temporal history remain coherent.
4. Frame-time budgets are measured deadlines. No remaining atomic NIF,
   terrain/precombine setup, static placement, or LOD operation can create an
   unbounded main-thread hitch.
5. Stream-out returns ECS, physics, texture, mesh, BLAS/TLAS, audio, and script
   ownership to the pre-entry baseline, modulo bounded caches.
6. Near terrain, distant terrain, objects, trees/ground cover, water, sky, and
   fog join without visible holes, double draws, or LOD thrash.
7. A deterministic capture passes image-health checks (not blank, white-out,
   NaN-saturated, or fallback-dominated) and records actionable asset/import
   failures.
8. Door transitions and save/load preserve the active worldspace, player grid,
   streamed change forms, time/weather state, and safe player pose.

The primary matrix is Oblivion Tamriel, FO3 Megaton/Capital Wasteland, FNV
WastelandNV, Skyrim Tamriel, and FO4 Commonwealth. Starfield joins after a
stable exterior profile and material source are pinned; FO76 remains parse-only
until its runtime profile exists.

## Issue slate

The identifiers below are stable plan IDs. GitHub issue numbers are added as
the work is filed; a single implementation may close more than one plan ID
when the acceptance gates are inseparable.

GitHub tracking is grouped by dependency-sized deliverable under
[#2377](https://github.com/matiaszanolli/ByroRedux/issues/2377): EX-01/05
[#2368](https://github.com/matiaszanolli/ByroRedux/issues/2368), EX-02/04
[#2375](https://github.com/matiaszanolli/ByroRedux/issues/2375), EX-06/07
[#2376](https://github.com/matiaszanolli/ByroRedux/issues/2376), EX-08
[#2374](https://github.com/matiaszanolli/ByroRedux/issues/2374), EX-09/17
[#2370](https://github.com/matiaszanolli/ByroRedux/issues/2370), EX-10/11
[#2371](https://github.com/matiaszanolli/ByroRedux/issues/2371), EX-12/13
[#2373](https://github.com/matiaszanolli/ByroRedux/issues/2373), EX-14/15
[#2369](https://github.com/matiaszanolli/ByroRedux/issues/2369), and EX-16
[#2372](https://github.com/matiaszanolli/ByroRedux/issues/2372).

| ID | Pri | Work item | Acceptance gate | Depends on |
|---|---:|---|---|---|
| EX-01 | P0 | Exterior smoke matrix and artifact bundle | One command runs each installed profile, captures a PNG plus bench/debug telemetry, self-skips absent data, and hard-fails empty/blank/non-exterior scenes | — |
| EX-02 | P0 | Foreground readiness and safe spawn contract | Missing/empty/terrainless center is reported with nearest viable cells; Character mode begins on a verified ground probe or explicitly falls back to FlyCam/error | EX-01 |
| EX-03 | P0 | Deterministic worldspace selection | Repeated ambiguous-grid loads choose the same preferred worldspace and log candidates | **Done: #2340** |
| EX-04 | P0 | Per-game terrain collision gate | Character is grounded at frame 0 and after a cell crossing on LAND for Oblivion/FO3/FNV/Skyrim/FO4; collider count and probe result are captured | EX-01, EX-02 |
| EX-05 | P0 | Non-finite/image-health regression gate | HDR/presentation output reports zero non-finite pixels; deterministic captures reject near-solid white/black and fallback-dominated frames | EX-01 |
| EX-06 | P0 | Boundary-crossing benchmark | Deterministic path crosses 2+ cells and reports per-cell queue, worker, apply, unload, LOD, and frame p50/p95/max timings | EX-01 |
| EX-07 | P0 | Finish deadline-bounded streaming | NIF finalization, terrain/water/precombine setup, ordinary static placement, texture/mesh upload, BLAS build, and every LOD provider yield by bytes/mesh batches under one measured budget | EX-06 |
| EX-08 | P0 | Cancellation and ownership soak | Repeated out-and-back traversal leaves no orphan CellRoot entries, physics bodies, textures, meshes, BLAS, audio emitters, scripts, or unbounded cache growth | EX-06, EX-07 |
| EX-09 | P1 | Exterior transition and save/load state | Interior↔exterior and exterior↔exterior transitions plus save/load restore worldspace/grid/player/weather/change forms without duplicate persistent refs | EX-02, EX-08 |
| EX-10 | P1 | Near-terrain completeness | LAND height/normal/color/splat paths have per-game real-data guards; adjacent cells have no cracks, texture discontinuities, or collider/render disagreement | EX-01, EX-04 |
| EX-11 | P1 | Complete distant LOD selection | 4/8/16/32 terrain/object bands, VWD full-model culling, `.btr` normal maps, far-plane/reversed-Z policy, and no near/LOD overlap or holes | EX-06, EX-10 |
| EX-12 | P1 | Sky, climate, weather, and parent-world inheritance | Child worldspaces inherit/override the correct climate and environment; cloud layers, sun, fog, and transition state remain finite and continuous across cells/doors | EX-05, EX-09 |
| EX-13 | P1 | Water coverage and seams | Worldspace default water, CELL overrides, rivers/planes, underwater state, and LOD boundaries have cross-game smoke profiles and no missing ocean/seam cases | EX-01, EX-10 |
| EX-14 | P1 | Ground cover and trees | GRAS/REGN placement and full SpeedTree/tree rendering replace billboard-only coverage where data exists; density is streamed and budgeted | EX-06, EX-10 |
| EX-15 | P1 | Persistent refs, parent worlds, and FO4 spatial data | Persistent/temporary ref ownership is correct across parent worldspaces; precombine/previs/occlusion data has explicit render and collision coverage | EX-08, EX-09 |
| EX-16 | P2 | Region/navmesh/audio/AI world integration | REGN drives ambient/fog/ground cover, NAVM paths streamed actors, and cell unload cleanly suspends/rebinds packages and audio | EX-08, EX-14, M42/M44 |
| EX-17 | P2 | Load-order/mod exterior conformance | Master/DLC/mod overrides merge WRLD/CELL/LAND/REFR/environment records deterministically, including deleted refs and partial worldspace overrides | EX-09–EX-15, M50 |

## Dependency-ordered execution

### Tranche A — make failures reproducible

1. [x] Add `docs/smoke-tests/m-exteriors.sh` for the five primary profiles.
2. [x] Capture bench, screenshot, lighting, texture, mesh-cache, camera, and
   renderer-scratch telemetry in one artifact directory.
3. [x] Add typed center-cell viability diagnostics, deterministic suggestions,
   safe FlyCam fallback, and missing/empty/content-backed regression tests.
4. [x] Replace the stale FO3 white-out prose with the automated image-health
   gate; keep the old capture as historical evidence, not a live claim.
5. [x] Calibrate all five live profiles and tighten their entity/draw floors
   against the 2026-08-04 radius-1 baseline.
6. [ ] Add the pre-tonemap non-finite pixel counter; PNG statistics cannot
   observe an HDR NaN directly.

Current state: all five profiles pass. FNV 4,367/1,229, FO3 3,201/1,093,
Oblivion 5,709/2,355, Skyrim 6,160/947, and FO4 57,102/22,706
(entities/draws); every PNG passed image health. FNV/FO3/Oblivion/Skyrim had
zero missing textures, while FO4 reported one. EX-01 is implemented; EX-05
remains open for the renderer-side non-finite counter, and the diagnostic/safe-
fallback half of EX-02 is implemented.

### Tranche B — make entry and traversal safe

1. Define a foreground-ready result carrying center source, terrain/reference
   availability, spawn candidate, and ground-probe status instead of returning
   bare `Vec3::ZERO` on ambiguity.
2. Add a deterministic two-boundary camera/player path and streaming telemetry.
3. Bring remaining atomic apply and LOD work under the shared wall-clock
   deadline.
4. Run cancellation/ownership soak loops and repair leaked owners.

Exit: EX-02, EX-04, EX-06, EX-07, and EX-08 are closed.

### Tranche C — close visual continuity

Complete near terrain and collision first, then multi-band LOD/VWD, environment,
water, and ground cover. Each slice adds a real-data profile and screenshot
comparison before expanding to the next game.

Exit: EX-10 through EX-15 are closed and all five primary profiles are visually
continuous from near field to horizon.

### Tranche D — make the exterior a world, not a render demo

Persist streaming/change-form state, integrate REGN/NAVM/audio/AI, and validate
DLC/mod override behavior. Add Starfield and then FO76 when their runtime
profiles are stable.

Exit: EX-09, EX-16, and EX-17 are closed; exterior traversal survives gameplay,
transitions, saves, and load-order changes.

## Verification policy

- Pure selection/delta/ownership rules stay in `cargo test`.
- GPU/game-data checks stay in `docs/smoke-tests` and self-skip missing data.
- Every visual fix records the exact worldspace/grid, camera pose, hour,
  archives, renderer/upscaler mode, entity/draw counts, and output image.
- Performance changes report distributions and worst atomic units, not only
  average FPS.
- Documentation is refreshed only from a green smoke artifact; wiring claims
  without a runnable profile are marked parse/load-only.
