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
- The repeatable exterior smoke matrix and its deterministic three-boundary
  `grid-cross` mode live at `docs/smoke-tests/m-exteriors.sh`. The traversal
  records queue/worker/apply/unload/LOD latency plus whole-frame p50/p95/max,
  and hard-fails superseded or unsettled boundaries. Earlier exterior coverage
  was split among audits, one SpeedTree smoke, and prose claims.

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
[#2374](https://github.com/matiaszanolli/ByroRedux/issues/2374) (**done**), EX-09/17
[#2370](https://github.com/matiaszanolli/ByroRedux/issues/2370), EX-10/11
[#2371](https://github.com/matiaszanolli/ByroRedux/issues/2371), EX-12/13
[#2373](https://github.com/matiaszanolli/ByroRedux/issues/2373), EX-14/15
[#2369](https://github.com/matiaszanolli/ByroRedux/issues/2369), and EX-16
[#2372](https://github.com/matiaszanolli/ByroRedux/issues/2372).

### Re-scoped buildable slices (2026-08-12)

Auditing EX-12/13, EX-14/15 and EX-16 against current code showed that each
bundled buildable work with work blocked behind unparsed data or an open
prerequisite. The buildable halves were split out so they can move
independently:

| Issue | Slice | Blocked on | Why it is buildable now |
|---|---|---|---|
| [#2735](https://github.com/matiaszanolli/ByroRedux/issues/2735) | EX-12a — honour the non-climate `PNAM` inheritance bits | — | Only `0x10` (climate) is implemented. Measured: 6 Skyrim + 1 FO3 + 1 FO4 worldspaces author no `NAM2` and set the inherit bit, so they render no water at all. Pure CPU. |
| [#2736](https://github.com/matiaszanolli/ByroRedux/issues/2736) | EX-05 — pre-tonemap non-finite pixel counter | — | Self-contained; also closes Tranche A item 6. The PNG mean/stddev gate cannot observe an HDR NaN. |
| [#2737](https://github.com/matiaszanolli/ByroRedux/issues/2737) | EX-16a — parse REGN `RDAT` region data | — | `RegionRecord` captures EDID/weather/colour only; `RDAT` carries the ambient sound, ground cover, objects and priority EX-16 needs. Pure parsing. |
| [#2738](https://github.com/matiaszanolli/ByroRedux/issues/2738) | EX-16b — parse NAVM geometry + connectivity | — | `NavmRecord` is EDID + version; no vertices, triangles or external connections exist to stream. Pure parsing. |

What stays blocked, and behind what:

- **EX-12/13 continuity across doors and cells** — EX-09 (#2370).
- **EX-14 ground cover Phases 1–5** — §11's open questions require measurement
  before Phase 1 (terrain-attribute sampling path, chunk size, tier distances),
  and the density field lives in GLSL where unit tests cannot reach it.
- **EX-14 full SpeedTree tree rendering** — out of ground-cover scope by §10;
  a separate authority in [`exal.md`](exal.md) §5.
- **EX-15 persistent refs across parent worlds** — EX-09 (#2370).
- **EX-16 runtime integration** (actor/package migration, emitter crossfade) —
  #2737 + #2738 plus M42/M44.

| ID | Pri | Work item | Acceptance gate | Depends on |
|---|---:|---|---|---|
| EX-01 | P0 | Exterior smoke matrix and artifact bundle | One command runs each installed profile, captures a PNG plus bench/debug telemetry, self-skips absent data, and hard-fails empty/blank/non-exterior scenes | — |
| EX-02 | P0 | Foreground readiness and safe spawn contract | Missing/empty/terrainless center is reported with nearest viable cells; Character mode begins on a verified ground probe or explicitly falls back to FlyCam/error | EX-01 |
| EX-03 | P0 | Deterministic worldspace selection | Repeated ambiguous-grid loads choose the same preferred worldspace and log candidates | **Done: #2340** |
| EX-04 | P0 | Per-game terrain collision gate | Character is grounded at frame 0 and after a cell crossing on LAND for Oblivion/FO3/FNV/Skyrim/FO4; collider count and probe result are captured | EX-01, EX-02 |
| EX-05 | P0 | Non-finite/image-health regression gate | HDR/presentation output reports zero non-finite pixels; deterministic captures reject near-solid white/black and fallback-dominated frames | EX-01 |
| EX-06 | P0 | Boundary-crossing benchmark | Deterministic path crosses 2+ cells and reports per-cell queue, worker, apply, unload, LOD, and frame p50/p95/max timings | EX-01 |
| EX-07 | P0 | Finish deadline-bounded streaming | NIF finalization, terrain/water/precombine setup, ordinary static placement, texture/mesh upload, BLAS build, and every LOD provider yield by bytes/mesh batches under one measured budget | EX-06 |
| EX-08 | P0 | Cancellation and ownership soak | Repeated out-and-back traversal leaves no orphan CellRoot entries, physics bodies, textures, meshes, BLAS, audio emitters, scripts, or unbounded cache growth | **Done: #2374** (FNV + FO3 green) |
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
6. [x] Add the pre-tonemap non-finite pixel counter; PNG statistics cannot
   observe an HDR NaN directly. **Done (2026-08-12, #2736)** — counted in
   `presentation.frag`, which is the last place the scene exists in linear HDR
   before ACES clamps it to `[0,1]`. Counting there rather than in a dedicated
   compute pass means no new pipeline, dispatch or barrier: that shader already
   runs exactly once per output pixel, so the check is one branch and an
   atomic. Surfaced as `r.health` and gated in every smoke mode.

   Proven with a negative control rather than assumed: injecting a
   runtime-derived NaN into the left quarter of the frame reported exactly
   230,400 pixels per frame against a 1280×720 output — precisely 25% — which
   confirms the atomics, the host-visible readback and the per-frame zeroing
   all behave. Reverted after the check; the counter reads clean on FNV
   WastelandNV and is Vulkan-validation clean.

7. [x] Add the environment-value asserts — the other half of EX-05.
   **Done (2026-08-12, #2368)** — `env.health` gates `CellLightingRes` +
   `SkyParamsRes` on finite, usable values and hard-fails the matrix on any
   violation. The pixel counter and this are not redundant: a NaN sun colour
   behind a zero-intensity sun renders a clean frame over a broken resource,
   and only one of the two checks can see it.

   The rules are deliberately confined to properties the producers already
   guarantee — finite, non-negative radiance, unit-length directions, and
   agreement between `lighting.is_interior` and `sky.is_exterior` (two
   independently-populated flags describing one fact, which is the "confirmed
   exterior lighting" clause). Fog distances are *reported, not gated*:
   `fit_legacy_fog_extinction` already treats `far <= near` as "no fog", so an
   inverted ramp is a shipped authoring pattern rather than a defect. The live
   matrix bore that out — FNV WastelandNV ships `fog_near = -10`, which a
   plausible-looking "near must be positive" rule would have failed.

   Proven with a negative control, as #2736 was: an injected finding turned
   the FNV run into `HARD FAIL - 1 unusable environment value(s)` with
   `env=bad=1` in the summary, confirming the report, the grep anchor and the
   TSV column all behave. Reverted after the check. 14 unit tests cover the
   rules themselves without a device or game data.

Current state: all five profiles pass. The 2026-08-12 re-run reads FNV
4,524/1,221, FO3 3,383/1,086, Oblivion 6,043/2,355, Skyrim 5,915/928, and FO4
57,596/22,983 (entities/draws); every PNG passed image health, every profile
reported zero non-finite pre-tonemap pixels, and every profile passed the new
environment-value gate. FNV/FO3/Oblivion/Skyrim had zero missing textures,
while FO4 reported one. EX-01 and EX-05 are both implemented; the
diagnostic/safe-fallback half of EX-02 is implemented.

The first live boundary matrix on 2026-08-04 established the EX-06 baseline:

- FNV settled all three crossings (full-detail max 1.17 s, LOD max 1.21 s,
  apply max 26.5 ms, frame max 76.9 ms).
- FO3 settled all three (47.2 ms / 6.1 ms full-detail/LOD maxima). Its final
  view is intentionally sparse; boundary mode therefore gates renderability,
  not the starting cell's static population floor.
- Oblivion settled all three (908 ms / 1.50 s, apply max 24.0 ms, frame max
  72.3 ms).
- Skyrim settled all three, but exposed an 8.10 s full-detail / 8.14 s LOD
  tail and 340 ms worst frame.
- FO4 initially exhausted device-local memory and lost the Vulkan device while
  repeatedly rebuilding a 600+ MiB global geometry SSBO. Capacity-aware
  rebuild batching plus an explicit large-buffer idle fallback removed the
  device loss and reduced roughly one hundred rebuilds to one per settled
  transaction. Making precombined spawning resumable per hash then reduced the
  worst apply slice from 2.55 s to roughly 300 ms. `grid-cross` now pauses its
  logical movement clock while a boundary is active, so renderer FPS cannot
  shorten the wall-clock handoff window and supersede the benchmark's own
  work. The resulting FO4 correctness gate passes all three independent
  crossings with no device loss, supersession, or unsettled work. It also
  exposes the remaining performance debt: full-detail/LOD max 18.71/18.73 s,
  unload 848 ms, apply 315 ms, and frame max 860 ms. A mesh-level precombine
  experiment reduced apply max to 145 ms but raised first settlement to 32.73
  s through many tiny BLAS submissions; it was removed rather than trading a
  smaller frame spike for 4.5x worse handoff throughput. A follow-up now
  batches the usual three-cell boundary eviction so global ECS compaction and
  BLAS scratch shrinking run once after all cell-local teardown. Its 900-frame
  validation again settled 3/3 crossings and reduced unload/dispatch/frame
  maxima from 848/850/860 ms to 826/827/836 ms. Apply max was 329 ms and
  full-detail/LOD max varied to 19.24/19.26 s, confirming that the remaining
  P0 is cell-local teardown plus single-hash apply/BLAS throughput rather than
  repeated global finalization. Phase telemetry then attributed 758 ms (91%)
  of the 837 ms unload to repeated `World::despawn` calls. A storage-oriented
  `despawn_batch` now sorts the victim set once, acquires each storage once,
  uses sparse-set O(1) removal, and linearly compacts packed storage once. The
  next live gate kept population/draw counts and 3/3 settlement intact while
  reducing unload to 101 ms, dispatch to 102 ms, and ECS despawn to 13 ms
  (58x lower); frame max fell to 548 ms. The remaining measured unload split
  is GPU release 83 ms, owned state/physics 5 ms, handle collection below 1
  ms, and finalization below 1 ms. Single-hash apply still reaches 316 ms and
  full-detail/LOD settlement 19.06/19.08 s. Per-hash phase timing rules out
  CSG/archive preparation (21 ms max) and the batched BLAS submission (20–24
  ms in boundary cells): CPU entity creation plus texture/mesh upload is the
  dominant 273–385 ms atomic span. A follow-up cursor accumulated all BLAS
  specs and still submitted exactly once per hash, but yielding CPU work per
  mesh serialized thousands of units behind render frames: the first two
  crossings regressed to 50.27 s and 105.66 s, then the run hit the 300 s hard
  timeout before the third settled. That cursor was removed. The next design
  must preserve CPU wall throughput as well as GPU batch throughput—most
  likely worker-side preparation or calibrated upload batches—not merely
  produce smaller individual frames. Cell-local GPU release had the same
  repeated-cache-scan shape as ECS teardown: every freed mesh retained over
  the full mesh cache, and every freed texture retained over the full path
  cache. Holder-counted batch APIs now perform those purges once while keeping
  per-handle descriptor fallback and deferred destruction. The next live gate
  reduced GPU release from 81 ms to 23 ms, total unload from 99 ms to 40 ms,
  and dispatch from 100 ms to 41 ms with unchanged population, draws, 3/3
  settlement, and device stability. Apply max was 291 ms and frame max 551 ms;
  the latter is now outside unload and remains tied to global geometry work.

### Tranche B — make entry and traversal safe

1. [x] Define a foreground-ready result carrying center source, terrain/reference
   availability, spawn candidate, and ground-probe status instead of returning
   bare `Vec3::ZERO` on ambiguity. **Done (2026-08-12, #2375)** — the typed
   `ExteriorForegroundReadiness` half landed with the Tranche A diagnostics;
   #2375 added the ground-probe half.

   The defect was one of *ordering*, not of missing information. The mode was
   chosen from cell content alone, the spawn probe ran afterwards, and a probe
   miss placed the capsule at `aabb.max.y + 200` — 200 units above the world
   with nothing beneath it. The probe already knew the ground was missing; it
   simply came after the decision it should have informed. `probe_spawn_ground`
   now runs before `select_initial_player_mode`, which gained a
   `ground_walkable` term, and a typed `GroundProbe` distinguishes "no
   colliders at all" from "colliders exist, none under the spawn column".

   Verified on the issue's own reproduction (FO3 `MegatonWorld` 0,0): the
   diagnostics name exactly the cells the issue predicts — (-1,-5), (0,-6),
   (-1,-6) — the probe reports `no-colliders`, and the rig falls back to
   FlyCam. The curated (-1,-7) profile still starts in Character with 789
   colliders, so the new gate does not demote healthy cells. All five profiles
   are grounded at frame 0: FNV 555, FO3 789, Oblivion 1244, Skyrim 314, FO4
   15,818 colliders.
2. [x] Add a deterministic three-boundary camera path and bounded streaming /
   whole-frame telemetry (EX-06).
3. [ ] Bring remaining atomic apply, unload, global-geometry, and LOD work under the shared wall-clock
   deadline.
   Precombined cells now yield between hashes. The next slice must move/defer
   single-hash CPU preparation/upload without serializing every mesh behind a
   rendered frame; BLAS is already one measured 20–24 ms batch per hash.
   Global unload finalization, ECS row removal, and mesh/texture cache purges
   are batched. The measured unload tail is now 40 ms total / 23 ms GPU;
   global-geometry rebuild and single-hash upload are the next frame-tail
   targets.
4. [x] Run cancellation/ownership soak loops and repair leaked owners (EX-08).

The soak is `m-exteriors.sh <profile> soak`. It drives the new `grid-soak`
bench camera — a triangle wave over the same boundaries `grid-cross` crosses
one way — because the *reversal* is what reaches pending-worker cancellation,
partial-apply cancellation, unload hysteresis, and stale-payload rejection. A
one-way traversal never exercises any of them. Ownership is sampled engine-side
once per completed round trip, deferred to the first frame with no boundary in
flight, and folded into an `OwnershipTracker` whose verdict is read out at
bench-hold via `world.owners report`.

Twenty-one owner classes are tracked, each carrying a reclaim policy so the
"modulo documented bounded caches" clause is executable rather than editorial:

- `Exact` — must return to baseline. Residency and ownership: ECS rows, cell
  roots, the `CellRootIndex`, live mesh/texture slots, BLAS, TLAS instances,
  terrain tiles, physics bodies, audio tracks, script state, particles, water.
- `Bounded` — may retain, must not grow without bound. Documented caches, plus
  the two reachability counts (below).
- `Monotonic` — allocator watermarks that rise by construction and are never
  failed on: `entities_spawned` and the mesh/texture registry *lengths*, which
  never shrink because retired slots stay as placeholders so a dangling
  `mesh_id` / `texture_index` cannot resolve to a different resource (#372).

First live results (2026-08-12, FNV WastelandNV and FO3 MegatonWorld, five
recorded cycles each after the first traversal establishes the baseline): both
profiles PASS. Every `Exact` class returned to baseline exactly — FNV held
`transform_rows` 3733, `cell_root_rows` 4556, `cell_root_index_entries` 13,
`meshes_live_slots` 718, `texture_live_slots` 190, `blas_entries` 494,
`tlas_instances` 1252, `terrain_tiles` 12, `physics_bodies` 874 constant across
all five cycles. No leaked owner was found, so no repair was needed; the
soak's present value is as a standing regression gate.

Two classes were reclassified `Exact` → `Bounded` on the evidence. `meshes_in_use`
and `textures_in_use` count *distinct handle values* reachable from entities,
which is a different question from residency: under #372 handle retirement,
re-entering a cell issues fresh handles for content whose slots were freed while
reusing handles for content that stayed resident, so identical scenes
legitimately map to different handle sets. The measured series was
620/715/591/620/715 — oscillating inside a fixed band, always at or below the
flat 718 live-slot count, never monotonic. They keep the growth check, which is
the half that would still catch a real leak; the exact-return duty sits with the
live-slot classes that genuinely fall on unload.

Clean shutdown under `BYRO_VALIDATION=1` is verified: zero panics and zero
validation errors during or after teardown. The eight
`VUID-VkShaderModuleCreateInfo-pCode-08740` reports are startup shader-module
creation and reproduce identically in `static` mode, so they are pre-existing
and outside EX-08.

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
