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
[#2370](https://github.com/matiaszanolli/ByroRedux/issues/2370) (**done**), EX-10/11
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
| [#2738](https://github.com/matiaszanolli/ByroRedux/issues/2738) | EX-16b — parse NAVM geometry + connectivity | — | **Geometry + connectivity landed** (2026-08-13): typed `NVVX`/`NVTR`/`NVEX` for FO3/FNV, packed `NVNM` for Skyrim-era, and tile→cell/grid association for every game including FO4. What is left on the issue is the `NAVI` `NVMI`/`NVPP` index, whose stated purpose — associating tiles with cells — the `NVNM` header now serves directly; its unique content (merged-mesh lists, preferred paths) belongs to the pathfinding work in #2372. Pure parsing. |

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

The 2026-08-16 upload pass removed the next identified per-hash cost without
splitting a hash across render frames. Every fresh submesh in one NIF/hash now
keeps its own vertex/index destination buffers, cache identity, and BLAS
ownership, but its copies share one aligned staging arena, command buffer, and
fence wait; cache hits are resolved first, entity spawn order is unchanged,
and the scalar uploader remains the compatibility fallback. Against a release
binary built immediately before the change, the deterministic FO4 radius-1
static load reached the streaming-context marker in about 13 s instead of
114 s. Its worst precombine CPU-spawn span fell from 1,627.79 to 342.77 ms
(78.9%), worst complete hash from 1,714.01 to 357.25 ms (79.2%), and BLAS max
from 66.62 to 54.90 ms, while preserving 59,232 entities, roughly 22,989
draws, 17,071 meshes, 1,979 textures, and a 22,715-instance TLAS. FO3 Megaton
fell from about 4 s to about 1 s with the same 3,451 entities, 1,091 draws,
and 1,087-instance TLAS. Both image/environment/RT gates passed and neither
run used the scalar fallback.

The post-change FO4 `grid-cross` gate also passed all three crossings with no
failed NIFs, supersession, unsettled work, device loss, or upload fallback.
Its worst apply slice improved from the preceding 291 ms result to 174.51 ms,
and unload remained bounded at 40.32 ms. It is not yet a traversal-throughput
win: this capture accumulated 104,210 entities / 25,154 draws and reported
58.71/59.45 s full-detail/LOD maxima plus a 1.50 s worst frame. The upload
transaction is therefore complete as a subtarget, while aggregate cooperative
apply pacing and global-geometry/LOD rebuild work remain the active EX-07
bottleneck. Artifacts are retained at `/tmp/byro-perf-after-fo4`,
`/tmp/byro-perf-after-fo3`, and `/tmp/byro-perf-after-fo4-boundary`.

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
   Precombined cells yield between hashes, and the 2026-08-16 packed staging
   path completes the single-hash upload subtarget: all fresh submeshes in a
   hash copy through one transfer submission while the existing one-batch BLAS
   contract and entity ordering remain intact. Global unload finalization, ECS
   row removal, and mesh/texture cache purges are also batched. The measured
   unload tail remains about 40 ms total / 21–23 ms GPU and worst apply fell to
   174.51 ms, but the new boundary gate still reaches 58.71/59.45 s
   full-detail/LOD settlement and a 1.50 s frame tail. Aggregate cooperative
   apply pacing plus global-geometry and LOD rebuilds are the remaining
   deadline targets.
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

#### EX-10/11 (#2371) — near-terrain correctness and distant LOD bands

**Correction (2026-08-22): this row and ROADMAP.md's M35 entry were stale.**
Substantial work landed 2026-08-12 through 2026-08-19 that neither doc
reflected — the 4/8/16/32 band ladder, far-plane sizing, Skyrim `.btr` normal
maps, and a live LOD-overlap/churn audit gate are all done. The plan below
starts from the corrected state.

1. [x] **4/8/16/32 LOD band selection** — done for Skyrim/FO4
   (`byroredux/src/cell_loader/lod_bands.rs`, commit `d96110eb`,
   2026-08-12). `LodBandLadder::for_game` reads each game's own
   `fBlockLevel0/1/2Distance` from the shipped `Ultra.ini`; both `.btr`
   (`terrain_lod_btr.rs`) and `.bto` (`object_lod.rs`) consume the same
   ladder. Oblivion/FO3/FNV correctly stay single-ring (no baked quadtree
   exists for those games) — that is the right per-game fallback, not a
   remaining gap.
2. [x] **Far-plane / depth-resolution measurement** — done (commit
   `9e96a9f9`, 2026-08-12). `DEFAULT_RENDER_DISTANCE` is derived and
   compile-time-asserted against the widest LOD ring's far-corner diagonal
   (`crates/core/src/ecs/components/camera.rs:65,213-228`);
   `Camera::depth_resolution_at` measures (not guesses) the ~37,250 BU/step
   precision collapse at the 250,000 BU ring on the current conventional
   (non-reversed) depth buffer.
3. [x] **Skyrim `.btr` normal maps** — done (commit `d96110eb`). FO4's
   `.btr` normals are `_msn` model-space and are deliberately left unbound
   pending a `Material` component on LOD entities — tracked as #2444, not
   re-scoped here.
4. [x] **Collider/render agreement** — already correct by construction:
   `spawn_terrain_mesh` (`byroredux/src/cell_loader/terrain.rs:652-668`)
   derives the trimesh collider from the exact same vertex/index buffers
   just uploaded to the GPU. Add the one missing piece: a regression test
   for the `log::warn!` zero-triangle-collider path (currently untested).
5. [x] **LAND real-data guards** — done, including the value-plausibility
   half. **Correction (2026-08-23)**: BTXT/ATXT FormID *existence* is not
   actually a parse-time gap — `cell_loader::terrain::spawn_terrain_mesh`
   already resolves every layer's LTEX against `landscape_textures` and
   `log::warn!`s + skips the layer when it's missing (`terrain.rs:158-162,
   528-532`). What landed the same day: the missing `land.rs` test file
   (`crates/plugin/src/esm/cell/tests/land.rs`, 9 tests) — VHGT
   delta-decode arithmetic (including the forward-row-accumulation case,
   the actual algorithmic risk in that function), VNML/VCLR raw-byte
   storage, and the ATXT-then-VTXT pairing contract (multi-layer,
   orphan-VTXT-dropped). No such coverage existed before.

   **Value-plausibility guards landed same day, later in the session** —
   the "no game data was available this session" premise turned out to
   be wrong too: real Oblivion/Skyrim/FalloutNV `.esm` files are present
   on this machine. A throwaway probe (`crates/plugin/examples/
   _tmp_land_stats.rs`, deleted after use) parsed all three real ESMs
   (~83M height samples, ~82M VNML samples total) rather than guessing
   thresholds:
   - **Height**: 0 non-finite samples across every real game. Still
     worth guarding at the consumer (`sanitize_land_height` in
     `terrain.rs`, clamps to 0.0) — `parse_land_record`'s VHGT decode
     starts from a raw `f32` read directly off the wire, so a corrupt or
     adversarial sub-record *could* seed NaN/Inf into an entire row's
     delta chain even though vanilla content never does; a poisoned
     world-space vertex would corrupt the mesh AABB, BLAS, and collision
     trimesh.
   - **VNML**: raw (pre-renormalize) magnitude ranged **0.7501–1.4254,
     identical to four decimal places across all three independently
     authored games** — strong evidence this is the byte-quantization
     grid's achievable range for "mostly upward" terrain normals, not
     an incidental per-game property. `vnml_raw_magnitude` +
     `VNML_DEGENERATE_RAW_MAGNITUDE = 0.5` (a wide margin below the
     measured floor) flags genuinely degenerate data — chiefly the
     exact-zero vector (`(128,128,128)` bytes) the existing `.max(0.001)`
     renormalize floor already survives without exploding into NaN/Inf,
     but previously with no diagnostic visibility into when it fires.
   - **VCLR**: confirmed a non-issue, not merely undone — a `u8` byte is
     inherently bounded to `0..=255`; there is no "out of range" a VCLR
     value could ever be. No guard needed, and none was invented.

   Both guards count anomalies across a cell's 1089 vertices and emit
   one summary `log::warn!` after the loop (not per-vertex — a fully
   corrupt file could otherwise flood the log). 4 new unit tests on the
   pure `sanitize_land_height`/`vnml_raw_magnitude` helpers.
6. [x] **Adjacent-cell crack detection** — pure checker half done; live
   wiring flagged, not rushed. Landed `cell_loader::terrain_seam::
   check_seam` — a pure function over two `LandscapeData` values (no
   `World`/`VulkanContext`) that reports every shared-edge vertex where
   two adjacent cells' heights disagree, plus whether their VNML raw
   bytes disagree at the edge, mirroring `lod_coverage`'s "pure
   functions over plain state" posture. Deliberately reports facts, not
   a pass/fail verdict — inventing a height-delta tolerance without real
   corpus data to calibrate it against would be exactly the guessed
   threshold this project avoids elsewhere (see item 5's note on the same
   constraint). 8 unit tests. **What's still open**: wiring this into a
   live `m-exteriors.sh` capture-mode check (item 7's stated
   prerequisite) needs `LandscapeData` to be resident somewhere queryable
   after `spawn_terrain_mesh` runs — today it's a transient parse-result,
   consumed and dropped at spawn time, confirmed by grep (zero retention
   sites). That's a real design decision, not a small addition: retain
   the full 33×33 grid per loaded cell (~4.4 KB/cell, simplest) vs. a
   lighter edge-only cache storing just the 4 border rows/columns
   (~130 B/cell). Flagged for a deliberate choice, not guessed at here.
7. [x] **Extend live coverage to catch near-field (full-detail LAND)
   geometric cracks** — done (2026-08-23), console-command half; the
   `m-exteriors.sh` capture-mode half is a natural but separate
   follow-up. The live `lod.coverage` gate (`lod_coverage.rs`,
   `find_overlaps`/`find_full_detail_overlaps`/`ChurnTracker`, commit
   `235c787c`) proves footprint-set correctness — no LOD-vs-full-detail
   overlap, zero churn — but not sub-cell geometric correctness within the
   full-detail ring itself. Item 6's seam checker is exactly that missing
   half, now wired live.

   **Correction**: item 6's own doc previously named a real design
   decision as this item's prerequisite — retain the full 33×33
   `LandscapeData` grid per loaded cell (~4.4 KB/cell) vs. a lighter
   edge-only cache (~130 B/cell) — because `spawn_terrain_mesh` was
   assumed to consume and drop it. That premise was wrong: `land: &
   LandscapeData` is borrowed from `CellData.landscape`, which lives
   inside `EsmIndex.cells.exterior_cells`, and `ExteriorWorldContext.
   record_index` (`Arc<EsmIndex>`) keeps the whole thing resident for the
   entire worldspace-streaming session — it's never dropped after spawn.
   No new cache needed at all; the design decision the doc flagged never
   actually had to be made.

   `streaming_helpers::update_terrain_seam_stats` walks `state.loaded`'s
   currently-resident grid-tile keys for east/north neighbor pairs that
   are BOTH resident (each boundary pair checked exactly once), looks up
   both sides' `LandscapeData` straight from `state.wctx.record_index`
   (same access pattern `apply_cell_region_ambient`/
   `apply_cell_climate_override` already use), and folds `check_seam`'s
   verdict into a new `TerrainSeamStats` resource — same shape as
   `LodCoverageStats` (`sampled`/`verdict()`/`machine_line()`), same
   refresh cadence (every `reconcile_lod_rings` call), same
   `PENDING`/`PASS`/`FAIL` console-command posture. New `terrain.seams`
   console command. Zero-tolerance verdict by design: authored terrain
   shares byte-identical LAND payloads at seams, so `pairs_dirty > 0`
   is always a real authoring/merge defect, never a magnitude judgement
   call. 3 new `TerrainSeamStats` tests (`crates/core`); the underlying
   `check_seam` logic already had 8. `update_terrain_seam_stats` itself
   has no direct unit test — same posture as `update_lod_coverage`,
   its untested sibling: too heavy to construct a `WorldStreamingState`
   fixture for, covered by live smoke instead.

   **Still open**: wiring `terrain.seams`' verdict into
   `m-exteriors.sh`'s capture-mode gate (the way `lod.coverage`/`r.health`
   already are) — a real follow-up, not attempted here; and live
   validation against real cross-plugin/DLC LAND override content (no
   game data available this session to confirm a real crack actually
   trips `pairs_dirty`, only synthetic fixtures).
8. [ ] **VWD active culling** — deliberately deferred, not a blocker.
   `exal.md` §5.2's ring-separation argument (full REFRs only inside
   `radius_unload`, LOD rings only outside it) already prevents a full
   model and its `.bto` proxy from coexisting by construction; the
   regression-detection half (`LodCoverageStats::vwd_full_model_overlaps`,
   gated at 0 in `m-exteriors.sh`) is done. Building an *active* cull needs
   decoupling the full-detail spawn radius from `radius_unload` (reintroducing
   the #1866 overlap risk) plus real-game visual validation — scope as its
   own follow-up issue rather than folding into EX-10/11 closure.
9. [ ] **Reversed-Z** — deliberately deferred (documented in
   `camera.rs:34-64`); touches SSAO/SVGF/TAA/composite/water/FSR3 and needs
   a GPU capture gate. The measurement work items 2 already did (far-plane
   sizing, depth-resolution-at-distance) is exactly the data that follow-up
   issue needs — file it separately given the blast radius, don't fold it
   into this one.

**First action**: refresh ROADMAP.md's M35 row and this section (done as
part of landing this plan) so the starting line is accurate before any of
items 5-7 begin.

#### EX-14/15 (#2369) — ground cover/trees, persistent refs, FO4 spatial data

Three independent sub-threads — recommend separate PRs/tracking sub-issues
per the project's own "re-scoped buildable slices" convention (§ above)
rather than one PR.

**A. Ground cover.** Phase 0 (canonical types, LTEX keyword map, palette) is
already done under this issue number (2026-08-12,
[`groundcover.rs`](../../crates/core/src/ecs/components/groundcover.rs) /
[`groundcover_translate.rs`](../../byroredux/src/groundcover_translate.rs)).
Phases 1-5 per [`exal-groundcover.md`](exal-groundcover.md) §9:

1. [ ] **§11.1 blocking measurement** — bench the terrain-attribute-sampling
   indirection cost (global-vertex-SSBO read per candidate point vs. a
   baked per-cell attribute texture) via `--bench-hold` against real
   terrain. Required before any Phase 1 code by the doc's own gate and this
   project's no-guessing policy — do not estimate this, measure it.
2. [ ] **Phase 1 — scatter**: `ExcludedFromTlas` generalisation, chunking
   (size TBD by the §11.2 sweep), the density field in GLSL,
   `groundcover_scatter.comp`, debug point rendering over real terrain.
3. [ ] **Phase 2 — blades + wind** (near tier only).
4. [ ] **Phase 3 — LOD chain**, tier-3 (always-on detail layer) lands first
   within this phase so later tiers are authored against a correct backdrop.
5. [ ] **Phase 4 — RT proxy shell.**
6. [ ] **Phase 5 — per-game palette**: `GRAS` → species resolution. This is
   where GRAS finally gets a real consumer — today it parses to a
   `MinimalEsmRecord` stub with every field discarded
   (`crates/plugin/src/esm/records/dispatch_misc_stub.rs:96-99`), so a real
   GRAS field decode is a prerequisite for this phase, not assumed done.
7. [ ] Add `OwnershipTracker` classes for ground-cover blade/chunk buffer
   byte counts alongside Phase 1 — none exist today, and the soak (EX-08)
   currently cannot distinguish a ground-cover leak from anything else.

**B. SpeedTree full rendering.** Confirmed billboard-only today
(`crates/spt/src/import/mod.rs` — always emits one placeholder quad,
fully wired and tested via `byroredux/src/systems/billboard.rs`). Genuinely
**has no existing design authority**: `exal-groundcover.md` §10 defers it to
`exal.md` §5, but §5 only covers terrain/object LOD, not SpeedTree geometry.
Recommend: write a short design doc (a §5.x addition to `exal.md`, or a
sibling `exal-trees.md`) covering geometry-tail decode, leaf-card billboards,
and `BezierSpline` wind-curve consumption — per `crates/spt/src/import/mod.rs`'s
own Phase 2 TODO (lines 31-45) — **before** any code, matching how ground
cover got its design doc before Phase 0. Scope as its own follow-up issue.

**C. Persistent refs across parent worlds + FO4 precombine previs/occlusion.**

1. [x] Fix `byroredux/src/cell_loader/transition.rs`'s stale module doc —
   done. It claimed exterior↔exterior transitions were "out of scope,
   errors"; `app_step.rs:755-871` actually fully implements them (drain,
   rebuild `ExteriorWorldContext`, restream) — flagged independently by
   both this investigation and #2370's.
2. [x] **Persistent-ref cross-worldspace continuity — WNAM walk done; live
   state carry-over flagged, not attempted.** Landed
   `cell_loader::exterior::resolve_persistent_cell`: walks the WNAM parent
   chain (bounded, cycle-guarded) when a worldspace authors no persistent
   CELL of its own, using its nearest ancestor's instead. Before this, a
   childless-persistent-CELL worldspace got NONE at all —
   `begin_worldspace_persistent_cell` exact-matched only the current
   worldspace's own key, so every globally-persistent quest actor/ref that
   lives only in a parent worldspace's persistent CELL silently failed to
   spawn the moment the player entered such a child worldspace, even
   though the child is meant to inherit that content by construction. 6
   unit tests (direct hit, one-hop inherit, two-hop inherit, no-persistent-
   cell-anywhere, WNAM cycle termination, unknown worldspace).
   **What's still open**: the "reconcile instead of re-spawning" /
   live-state-carry-over half. A worldspace crossing is still a full
   `drain_streaming_state` drain-and-reparse regardless of whether the
   source and destination resolve to the SAME persistent CELL (e.g.
   leaving a child worldspace back to its parent, or between two
   siblings sharing one ancestor's persistent CELL) — so a live
   modification to a persistent ref (moved object, script-driven state
   change) is lost on any such crossing, and the re-spawn is pure waste
   when the underlying CELL didn't actually change. Fixing this means
   comparing the resolved persistent-CELL identity across the crossing
   (via `resolve_persistent_cell`, now available) BEFORE
   `drain_streaming_state` runs, and skipping the persistent-CELL
   drain+rebuild specifically when it matches — while still fully
   draining the non-persistent grid tiles, which always change with the
   worldspace. That's a change to the crossing teardown sequence itself
   (`step_cell_transition`'s Exterior arm, `execute_pending_save_loads`'s
   exterior reload, `begin_exterior_streaming`), not an additive lookup
   like the WNAM walk — real regression risk to already-working
   transition code, flagged for a deliberate follow-up rather than rushed
   here.
3. [ ] **FO4 previs/occlusion** (`.uvd`, XPCI-equivalent) — zero parser,
   zero consumer (`byroredux/src/cell_loader/precombined.rs:25-31`
   documents this as a known deferred sub-item). Still true; still
   recommended as its own research-spike issue rather than folded in
   here — the visibility-set payload itself remains fully unknown.

   **Partial crack, 2026-08-23**: "no niftools spec is cited anywhere in
   the codebase" doesn't mean unstartable — real FO4 data was available
   this session (see item 4's correction below for the same discovery).
   Extracted and byte-compared 3 real `.uvd` files from `Fallout4 -
   MeshesExtra.ba2`'s `vis\fallout4.esm\<cell_formid>.uvd` (sizes 3.4 KB
   / 538 KB / 965 KB — genuine size variance, not a fixed-size format).
   The outer header cracks cleanly:
   - Bytes `0..4`: `u32 = 0xD6000012` LE, byte-identical across all 3
     samples — a format magic/version constant.
   - Bytes `4..8`: varies per file, not yet identified (candidate: a
     content hash/checksum, or a per-cell coordinate — not conclusively
     either).
   - Bytes `8..12`: `u32` LE, **confirmed exact match to the file's own
     total byte length** in all 3 samples (3472 / 538176 / 964768) — a
     self-reported size field, directly verifiable against any sample
     without guessing.
   - Bytes `12..16`: `f32` LE `= 512.0`, byte-identical across all 3
     samples.
   - Bytes `0xB0..0x100`: a null-padded **embedded ASCII debug string**,
     byte-identical across all 3 samples: `T 512.0 SO 128.0 SH 16.000
     BF 100 F 0 CS 0.0 - 3.3.17 F 1 0 OG 0`. The `T 512.0` term matches
     the `f32` at bytes `12..16` exactly — confirms that field's
     semantic (tile size) and confirms the string is a generation-tool
     parameter/version fingerprint baked in at build time, not per-cell
     content (identical across cells of wildly different size).
   - Bytes `0x14` onward (before the string) vary per file and look
     float-shaped (bounding-box/coordinate candidates) — **not
     decoded**; this is exactly where genuine per-cell visibility-set
     structure would begin, and guessing a layout here without more
     samples/cross-referencing against known cell bounds would be the
     kind of invented-threshold work this project's no-guessing policy
     exists to prevent.

   Net: the outer framing (magic, self-validating size, tool
   fingerprint) is real and reproducible, not a guess — but the actual
   visibility-set payload (what previs data is *for*) is still
   completely uncracked. Not enough to build a parser worth shipping
   (a magic-number + debug-string reader has no real consumer), but a
   real head start for whoever picks up the research spike next —
   recorded here rather than only in scratch output so it isn't
   re-discovered from zero.
4. [ ] **Precombine collision** — **correction (2026-08-23): NOT smaller or
   better-scoped than previs; same blocker class.** Investigated against
   real `Fallout4 - MeshesExtra.ba2` data (159,866 files) rather than
   assumed. Two premises in the original framing were both wrong:
   - **Naming**: the sibling file is `<cell_formid:08x>_physics.nif`
     (4,484 of them in `MeshesExtra.ba2` alone), NOT `_precomb.nif` — that
     name appears nowhere in the archive.
   - **Block types**: sampled 16 real `_physics.nif` files (consistent
     across all 16, not a one-off) — every one is exactly `NiNode` +
     `NiExtraData` + `bhkNPCollisionObject` + `bhkPhysicsSystem`.
     `bhkPhysicsSystem` decodes to `BhkSystemBinary`
     (`crates/nif/src/blocks/collision/collision_object.rs:121-151`): a
     **raw undecoded Havok-serialised (HKX-like) byte blob**, explicitly
     documented as "store the raw bytes... hand off to a Havok parser
     later" — there is no convex-hull/rigid-body data to extract with
     `crates/nif`'s *existing* parsers the way the original framing
     assumed. This is the SAME `BhkSystemBinary` blob that already blocks
     general FO4+ physics/ragdoll work (PHYSAL). Needs a Havok
     NP-physics binary decoder — greenfield format work, not a small
     addition — before any precombine-collision extraction is possible.
     Re-scope as its own research spike, same posture as item C3 above,
     not "land independently and sooner."
5. [x] **Double-geometry guard** — already correct and shared between
   interior/exterior loaders via `absorbed_refs_or_empty`
   (`byroredux/src/cell_loader/precombined.rs:52-75`, #2063). No work
   needed here; VWD/LOD overlap is covered separately under EX-11 above.

### Tranche D — make the exterior a world, not a render demo

Persist streaming/change-form state, integrate REGN/NAVM/audio/AI, and validate
DLC/mod override behavior. Add Starfield and then FO76 when their runtime
profiles are stable.

Exit: EX-09, EX-16, and EX-17 are closed; exterior traversal survives gameplay,
transitions, saves, and load-order changes.

#### EX-09/17 (#2370) — exterior transitions, save/load, load-order conformance

1. [x] **Trivial first**: fix `transition.rs`'s stale module doc — done
   (see EX-14/15 item C1 above — same finding, fixed once for both).
2. [x] **Transition mechanics** (worldspace/grid/pose/time/weather) — all
   fully wired today via `App::step_cell_transition`
   (`byroredux/src/app_step.rs:692-873`): grid/pose restore correctly
   (citing two prior bug fixes, #1874 and #2869), the game clock is never
   touched by transition code (`GameTimeRes` survives by omission, per
   `world_setup.rs`'s own comment), and `collapse_weather_transition` runs
   before every re-apply so no stuck crossfade survives a crossing. What's
   missing is test coverage: only coordinate-conversion and radius-arg unit
   tests exist in `transition.rs` today — add one that exercises the full
   multi-field restore end-to-end (grid + pose + time + weather together).
3. [x] **Duplicate-persistent-ref guard** — done. Landed
   `PersistentRefIndex` (`byroredux/src/components.rs`) + its build/query
   logic (`cell_loader::persistent_ref_index::{resolve_persistent_ref,
   invalidate}`): an `O(1)`-after-rebuild `FormId(u32, global load-order
   space) → Entity` map scoped to the resident persistent CELL, keyed for
   invalidation on the persistent-cell root's `EntityId` rather than
   `NameIndex`/`SubtreeCache`'s component-count heuristic (a worldspace
   crossing always allocates a fresh root, so identity is a precise
   signal here). Reuses `resolve_entity_by_global_form_id`'s key space
   and `FormIdPool` resolution rather than duplicating it — this only
   adds the missing O(1) cache over an already-correct O(n) resolver.
   Landed ahead of its consumer (`#[allow(dead_code)]`, same posture as
   `groundcover_translate.rs`'s Phase 0 constants), since today's call
   pattern (`begin_worldspace_persistent_cell` always spawns a fresh root
   after a full teardown) genuinely has nothing to guard against yet —
   confirmed by investigation, not assumed. **This is the foundational
   piece both EX-14/15 (parent-world persistent refs) and EX-16
   (actor/package migration) were blocked on** — both can now build on
   it. 5 unit tests cover resolve/miss/cross-root-exclusion/rebuild/
   invalidate.
4. [x] **Exterior save/load** — done. Landed `CurrentExteriorContext`
   (`byroredux/src/cell_loader/transition.rs`), the `CurrentCellContext`
   counterpart for exterior sessions: worldspace key + esm/masters +
   grid + load/unload radius, save-registered like `CurrentCellContext`.
   Kept in sync at every point a fresh exterior session starts
   (`scene::begin_exterior_streaming`, now the single call site all four
   producers — boot's `--grid` mode, `App::step_cell_transition`'s
   Exterior arm, the `dbgload` console command, and `save_io`'s reload
   path — funnel through, consolidating what had been four separately
   maintained copies of the same setup sequence), moves (the
   `grid_changed` block in `App::step_streaming`), or tears down
   (`clear_current_exterior_identity`, hung off
   `streaming_helpers::drain_streaming_state` — the same choke point
   every exterior teardown already funnels through, mirroring how
   `clear_current_interior_identity` hangs off `unload_current_interior`).
   `LoadCommand::execute` now accepts either context and only rejects a
   snapshot carrying neither (loose-NIF saves); `execute_pending_save_loads`
   dispatches to a new `reload_exterior_session`, split from the interior
   path into `reload_interior_session`/`reload_exterior_session` sharing
   one restore/apply/validate/pose-restore tail.
   **In-flight-streaming-worker decision**: cancel/drain, not wait or
   snapshot-and-resume — the same posture `drain_streaming_state` already
   uses at every other exterior teardown boundary (cell transitions, and
   this function's own interior branch). A discarded in-flight cell
   payload just means that cell isn't in `World` yet; the fresh
   `WorldStreamingState` rebuilt around the saved grid re-requests it from
   scratch, so nothing is lost, only re-fetched — picked because it's the
   consistent answer, not the convenient one. Preflights the same way
   SAVE-D6-02 requires for interior (`build_exterior_world_context` before
   any destructive teardown, so a bad save can't strand the session in an
   empty world), and reuses the already-built context on success rather
   than re-parsing, avoiding the double-parse the interior path's separate
   `validate_cell_loadable`/`load_cell_with_masters` pair accepts.
   2 new regression tests (exterior save→load command round-trip; loose-save
   rejection with the updated error message). Full workspace suite: 1408
   passed (was 1406), 0 failed, no new warnings.
5. [x] **CELL/REFR/LAND/environment multi-master merge** — fully wired and
   tested via `EsmCellIndex::merge_from`
   (`crates/plugin/src/esm/cell/mod.rs:1271-1316`): per-REFR-FormID
   last-write-wins reference merging (fixing the historical #1546
   whole-vec-replace bug), absent-field-inherits-from-base for
   lighting/landscape/water/climate, all covered by real tests in
   `crates/plugin/src/esm/cell/tests/merge.rs`. **Correction**: the
   `crates/plugin/src/datastore.rs`/`resolver.rs` (`DataStore`,
   `DependencyResolver`) machinery this issue's "conflict-resolution
   mechanism" language might suggest looking at is dead code — never
   constructed anywhere in `byroredux/src`. Don't build on it; either wire
   it in for real or formally deprecate it in a follow-up (flagging for a
   decision, not deciding here).
6. [x] **WRLD merge is weaker than CELL's** — flagged, deliberately not
   fixed (no real fixture to validate a partial-inherit rewrite against —
   see the "no-guessing" posture applied elsewhere in this codebase).
   Recorded in code, not just here: `EsmCellIndex::merge_from`'s own doc
   comment (`crates/plugin/src/esm/cell/mod.rs`) now states explicitly that
   `self.worldspaces.extend(...)` is whole-record last-write-wins, unlike
   CELL's `merge_cell_override` partial-field inherit, and names the
   condition for revisiting it (a real sparse-WRLD-override mod). Pinned by
   a real load-order fixture, `wrld_override_replaces_whole_record_not_partial_fields`
   (item 8) — if a future change gives WRLD partial-inherit, that test is
   *meant* to start failing; update it to match rather than relaxing it.
7. [x] **Cross-plugin deleted-ref bug** — fixed. Added `CellData::deleted_refs:
   Vec<u32>` (`crates/plugin/src/esm/cell/mod.rs`), populated by
   `parse_refr_group` (`walkers.rs`) when it hits a Deleted-flagged
   REFR/ACHR/ACRE instead of the previous bare `continue` that recorded no
   removal signal at all. `merge_placed_references` now removes any base
   REFR whose FormID appears in the override's `deleted_refs` before
   folding the override's own additions/changes in — a later plugin
   re-placing a REFR at the same FormID (a legitimate un-delete) still
   wins via the existing last-write-wins path, unaffected by the earlier
   deletion. Deliberately transient (not inherited forward via `extend`
   like `absorbed_refs`): the signal only means something for the ONE
   override round that produced it. Both `parse_refr_group` call sites
   (interior `walkers.rs`, exterior/worldspace `wrld.rs`) updated
   identically (SIBLING check). 4 new unit tests in
   `crates/plugin/src/esm/cell/tests/merge.rs` (single delete, delete +
   override combined, delete-then-later-readd, exterior sibling) plus a
   strengthened `deleted_refr_tombstone_is_skipped` assertion.
8. [x] **Load-order conformance profiles** — added, in
   `byroredux/src/cell_loader/load_order.rs`'s existing `#[cfg(test)] mod
   tests` (the module that already owns `parse_record_indexes_in_load_order`
   end-to-end fixtures, not `crates/plugin`'s `EsmCellIndex::merge_from`
   unit tests — those exercise the merge *algorithm* directly; these
   exercise the real multi-plugin pipeline: on-disk files, MAST-based
   FormID remap, per-plugin parse feeding the running merge).
   `three_plugin_chain_composes_refr_merge_and_cross_plugin_delete` is a
   genuine base→DLC→mod 3-plugin chain (deeper than any existing 2-plugin
   fixture) exercising item 5 (already-working CELL/REFR merge) and item 7
   (this session's delete fix) together, including the un-delete case.
   `wrld_override_replaces_whole_record_not_partial_fields` pins item 6's
   documented gap with a real fixture. Both go through
   `parse_record_indexes_in_load_order`, not a hand-built `EsmCellIndex`.

#### EX-16 (#2372) — REGN, NAVM, ambient audio, AI integration

Buildable parse-side slices are done: EX-16a (REGN RDAT, #2737) and EX-16b
(NAVM geometry + connectivity, #2738) are both closed. Everything below was
genuinely new when this section was first written — **zero runtime
consumers existed for either parsed dataset**, confirmed by exhaustive
grep. REGN's `Sound.music` field now has a real consumer (items 1 + 5,
2026-08-23); NAVM and REGN's other fields still do not.

1. [ ] **REGN runtime consumption** — **correction (2026-08-23): NOT the
   isolated slice this originally claimed.** `RegionDataEntry`/
   `RegionDataKind` (`crates/plugin/src/esm/records/misc/world.rs:452-780`)
   fully expose Objects/Weather/Map/Landscape/Grass/Sound/Imposter payloads
   with authored priority ordering (`entries_by_priority`), and `CellData.
   regions: Vec<u32>` (XCLR) already gives every resident cell its REGN
   FormID list directly — no polygon-containment math needed. But
   `RegionDataKind::Sound`'s `sound_form: u32` points at a `SOUN` record,
   and `SOUN` is dispatched through `parse_minimal_esm_record`
   (`dispatch_misc_stub.rs:75-79`) — EDID + optional FULL only, same
   "stub, no real field decode" posture GRAS has for EX-14/15 phase 5. The
   sound *filename* sub-record is never parsed, so there is no path today
   from a REGN sound FormID to an actual archive audio file. The existing
   `crates/audio`/`asset_provider` sound-loading examples
   (`try_load_default_footstep`/`try_load_default_water_splash`,
   `asset_provider/texture.rs:92-172`) only prove the archive-extract →
   decode → `AudioEmitter` pipeline against **hardcoded canonical paths**,
   not FormID-driven resolution — that resolver doesn't exist either. A
   real SOUN field decode (mirroring the GRAS gap) plus a FormID→path
   resolver are both prerequisites this item didn't originally name.
   Re-sequenced after item 2 as a result — do item 2 first.

   **Both named prerequisites done (2026-08-23).** `parse_soun`
   (`crates/plugin/src/esm/records/soun.rs`) decodes `FNAM` — SOUN's
   file path, relative to `Data\Sound\` — graduating `EsmIndex.sounds`
   from `HashMap<u32, MinimalEsmRecord>` to `HashMap<u32, SounRecord>`;
   `SNDD`/`SNDX` attenuation-curve bytes are deliberately left undecoded
   (no verified byte layout available this session, and not needed to
   resolve a FormID to a playable path — the actual EX-16 item 1
   blocker). `asset_provider::audio::{resolve_sound_path,
   sound_archive_path}` add the FormID→archive-key resolver, mirroring
   `script::pex_archive_path`'s shape (lowercase, `sound\` folder
   prefix, no double-prefixing). 9 unit tests (3 `parse_soun`, 6
   resolver).

   **Selection + live resolution also done (2026-08-23), same day.**
   `select_active_region_sound` (`crates/plugin/src/esm/records/misc/
   world.rs`) generalises `RegnRecord::entries_by_priority` across every
   region tagging one cell: given a cell's `regions: Vec<u32>` (XCLR) and
   the parsed `REGN` map, it collects every tagging region's `Sound`
   entries and picks the authored-priority winner (stable-sort tie-break,
   same rule as the single-region method). `RegionAmbientRes`
   (`byroredux/src/components.rs`) is the CPU-only resource carrying the
   winner's `music`/`incidental` FormIDs — deliberately NOT the RDAT
   `sounds: Vec<RegionSound>` ambient-loop list, whose `chance_raw`
   selection probability has an unresolved fixed-point scale (ties back
   to item 5's #2372 note); picking one without a verified scale would be
   a guessed threshold. Wired at cell-apply time on both loaders: interior
   via a new `CellLoadResult::region_ambient` field (computed alongside
   `resolved_lighting`, before the `index.cells` move, same pattern the
   existing `cell_name` capture already uses) and inserted at all three
   production call sites (`scene.rs`, `debug_load.rs`,
   `transition.rs`) right next to `apply_interior_cell_lighting`; exterior
   via a new `scene::apply_cell_region_ambient`, called from
   `App::step_streaming` right next to `apply_cell_climate_override` (same
   "outside the grid-changed guard" placement, for the same bootstrap
   reason), resolving against `wctx.record_index.cells.exterior_cells` +
   `wctx.record_index.regions` — no new state needed, `ExteriorWorldContext`
   already retained the full `EsmIndex`. 7 new selection tests + 3
   resource tests; `RegionAmbientRes` classified in
   `NOT_SAVED_BY_DESIGN` (rederived identically every load, same posture
   as `CellLightingRes`/`NavmeshTile`).

   **`RegionAmbientRes` now has a real consumer (2026-08-23, same day)**:
   item 5's `music`-track dispatch — see item 5 below for the full
   picture. `incidental` still has none. Also still open:
   `Weather`/`Map`/`Landscape`/`Objects`/`Grass`/`Imposter` RDAT kinds
   have no selection logic at all (only `Sound` was built) — REGN-driven
   weather-table selection in particular would overlap with the
   worldspace-level climate/weather system and needs its own design
   pass, not an assumption that `select_active_region_sound`'s shape
   generalises cleanly to every kind.
2. [x] **NAVM streaming lifecycle** — done. Landed `NavmeshTile`
   (`byroredux/src/components.rs`), a plain CPU-only component wrapping
   one resident `NavmRecord`, spawned by the new
   `components::spawn_navmesh_tiles` helper from both cell loaders
   (`cell_loader::load::load_cell_with_masters` for interiors,
   `ExteriorCellApplyJob::begin` for exterior tiles) inside the same
   `first_entity..last_entity` window every other cell-owned entity is
   spawned in. No bespoke reclaim path needed the way `LodBlock` needs
   one — NAVM carries no GPU handle, so the existing generic
   `stamp_cell_root_range` → `CellRootIndex` → `unload_cell` teardown
   chain reclaims it automatically. Landed ahead of its consumer
   (`#[allow(dead_code)]`, same posture as `PersistentRefIndex`) since
   item 3 (pathfinding) is deliberately scoped as its own follow-up issue
   and isn't built here. **Known small gap, not fixed**: the worldspace
   *persistent* CELL path (`PersistentCellApplyJob` in `exterior.rs`)
   isn't wired — persistent CELLs are REFR/actor-focused and NAVM data on
   one has no confirmed real-content occurrence; revisit if real data
   shows it matters, same "flag not guess-fix" posture as item 6 above.
   2 new unit tests (`components::navmesh_tile_tests`). Full workspace
   suite: 1412 passed (was 1410), 0 failed, no new warnings.
3. [ ] **NAVM pathfinding** — genuinely greenfield. `locomotion.rs:9` and
   `wander.rs:6-7,23-26` explicitly document straight-line-only movement as
   a known gap, not an oversight. This is the single largest item in the
   whole epic — a pathfinding algorithm plus integration with all five
   existing AI package systems (sandbox/wander/travel/follow/guard/escort).
   **Recommend scoping as its own follow-up issue** rather than folding
   into EX-16, given its size relative to everything else here.
4. [ ] **Actor/package suspend-migrate-resume across stream boundaries** —
   **correction (2026-08-23): "unblocked by `PersistentRefIndex`" doesn't
   hold up; the real shape is bigger.** `unload_cell_inner`
   (`cell_loader/unload.rs:90-247`) does an unconditional `despawn_batch`
   with zero AI-package-state awareness; reload respawns fresh via
   `spawn_npc_entity` with package state re-initialized from scratch —
   confirmed correct, that half of the premise stands.

   What doesn't stand: `PersistentRefIndex` is scoped to entities whose
   `CellRoot` equals the worldspace's `persistent_root`
   (`cell_loader::persistent_ref_index::rebuild`'s `owned_by_persistent_cell`
   check) — and a persistent-flagged actor's `CellRoot` IS
   `persistent_root`, precisely because `WorldStreamingState.persistent_root`
   is documented as "not keyed by a grid coordinate and never participates
   in radius eviction; reclaimed only when the worldspace drains." A truly
   persistent actor is therefore *never despawned by ordinary streaming in
   the first place* — there is nothing for `PersistentRefIndex` to help
   resolve on respawn, because no respawn happens. The actors that DO get
   despawned/respawned by radius streaming are exactly the *temporary*
   (non-persistent) ones placed in ordinary grid-tile `CellData`, and
   those never pass through `persistent_root` at all — `PersistentRefIndex`
   cannot resolve anything about them by construction. Even setting that
   aside, `PersistentRefIndex` is a live-entity FormId→Entity *resolver*,
   not a state *snapshot store*; it has no mechanism to answer "what was
   this actor's package/animation state before it was despawned" even for
   entities it can see.

   The real blocker, confirmed by reading the runtime state types
   (`AmbientPackageRuntime`, `TravelState`, `WanderState`,
   `crates/core/src/ecs/components/{travel,wander}.rs`): a respawned
   actor doesn't just lose package *selection* — `spawn_npc_entity`
   places it back at its **authored REFR position**, discarding any
   movement since spawn. `TravelState.destination` is a frozen, lazily-
   resolved point (not a progress/waypoint tracker), and there is a
   `Traveled` terminal marker with no despawn-time snapshot — so an actor
   that already finished a Travel package and stopped would, after any
   cell-boundary despawn/respawn cycle affecting its *spawn* tile
   (irrespective of the actor's current position — ownership is
   entity-range/`CellRoot`-based, assigned at spawn, not tracked by
   current location), restart its entire walk from the original spawn
   point. `WanderBehavior`'s own doc already establishes the opposite
   philosophy for Wander specifically — its `form_id` feeds a
   deterministic desync hash *by design*, so a re-roll is intentional,
   not a gap — which is presumably why the original framing already
   named Wander as safe to reset.

   A real fix needs a genuine snapshot/restore mechanism spanning
   Transform (or lazily re-deriving it correctly instead of resetting)
   plus `AmbientPackageRuntime`/`TravelState`/`Traveled`/`Seated`
   together — not a smaller index lookup — and it is the *same*
   underlying architecture gap already flagged, and deliberately not
   fixed, under EX-14/15 item C2's "reconcile instead of re-spawning"
   half: `drain_streaming_state`/`unload_cell_inner` always fully tear
   down and rebuild, with no live-state-carryover path at all today. Real
   regression risk to already-working streaming/despawn code, same
   "flag for a deliberate follow-up, don't rush it" posture applied there
   and to FO4 previs/precombine-collision above. Recommend scoping as its
   own design pass (or folding into whatever follow-up eventually attacks
   EX-14/15 item C2's reconcile half, since they'd likely share a
   mechanism) rather than attempting a partial fix here.
5. [x] **Ambient audio emitter REGN-binding — music done, `incidental` not
   attempted.** The generic crossfade machinery already existed
   (`AudioWorld::play_music`/`stop_music`, `crates/audio/src/lib.rs`);
   what landed (2026-08-23) is the dispatch wiring, not a new spatial
   emitter type — `play_music` is non-spatial by design (background
   track, not a positioned SFX), which is the right fit for `music` and
   made the originally-scoped "REGN-keyed `AudioEmitter`" the wrong
   shape for this field. `asset_provider::audio::SoundArchiveProvider`
   (mirrors `ScriptProvider`: repeatable `--sounds-bsa`, first-hit-wins,
   registered once at boot in `boot.rs`) gives arbitrary-FormID-driven
   archive lookups a persistent handle, unlike
   `try_load_default_footstep`/`try_load_default_water_splash`'s ad hoc
   single-hardcoded-path reopens. `dispatch_region_ambient_music`
   resolves `RegionAmbientRes::music_form` → `resolve_sound_path` →
   `sound_archive_path` → `SoundArchiveProvider::extract` →
   `load_streaming_sound_from_bytes` → `AudioWorld::play_music`
   (3-second crossfade, nominal volume — no per-region volume field
   exists to scale by), or `stop_music` on any failure at any step
   (unresolvable FormID, no archive, file not found, decode error) —
   deliberately fails to silence rather than leaving the *previous*
   cell's track playing into a cell that doesn't call for it.

   Called from both cell loaders, change-guarded against the resource's
   *prior* value so walking between two cells/tiles sharing one tagging
   region doesn't restart the track with an audible crossfade every
   crossing: interior inside `load_cell_with_masters` itself (reads the
   live `RegionAmbientRes` — still the *departing* cell's value at that
   point, since the three external call sites haven't overwritten it
   yet — compares against the freshly resolved `music_form`, dispatches
   before returning); exterior inside `apply_cell_region_ambient`
   (same comparison, using `wctx.record_index.sounds`, before the
   resource write).

   `SoundArchiveProvider` classified in `NOT_SAVED_BY_DESIGN`
   (engine-wide archive handle, same posture as
   `FootstepConfig`/`WaterAudioConfig`). 8 new tests (2
   `SoundArchiveProvider` construction, 4 `dispatch_region_ambient_music`
   failure-path no-panic/stops-playback cases — all headless-safe, no
   audio device or real archive required).

   **What's still open, deliberately**: `incidental` (`RDSI`, FNV-only)
   and the `sounds: Vec<RegionSound>` chance-based ambient-loop list have
   no dispatch at all — `incidental` because it genuinely wants a
   spatial/looping `AudioEmitter` rather than the non-spatial `music`
   track (a real design decision on emitter placement/attenuation, not
   attempted here), and `sounds` because its `chance_raw` selection
   probability still has an unresolved fixed-point scale (unchanged from
   item 1's note). REGN-driven weather/objects/map/landscape/grass/
   imposter selection also remains entirely unbuilt — only `Sound` has a
   selector.
6. [ ] **OwnershipTracker telemetry** — partially done.
   `navm_tiles_resident` landed (2026-08-23), `Exact` policy, following
   the existing `OwnerClass`/`ReclaimPolicy` pattern
   (`crates/core/src/ecs/resources/ownership.rs`) exactly —
   `NavmeshTile` carries no GPU handle and relies entirely on the
   generic `stamp_cell_root_range` → `CellRootIndex` → `unload_cell`
   chain, so the class exists to prove that generic path actually
   reclaims it (same reasoning `precombine_mesh_rows` already
   established for splitting a residency question out of the
   `cell_root_rows` aggregate), not because a bespoke leak vector is
   suspected. `regn_active_entries` does NOT fit this pattern and
   should not be added: `RegionAmbientRes` is a single fixed-size Copy
   struct, always exactly one instance, nothing to leak or grow — the
   `OwnershipTracker` model exists for collections/handles that can
   accumulate, not for a plain resource value. `ai_package_rows`
   remains blocked on item 4, which is unresolved (see its correction
   above).

**Recommended sequencing**: items 1, 2, and 5 are done end-to-end for
`music` — a player crossing into a region-tagged cell now audibly hears
its REGN ambient track, with no gap left between "parsed" and "playing."
Item 1 turned out NOT to be dependency-free (a real `SOUN` field decode
was an unstated prerequisite); item 5 turned out to need a persistent
`SoundArchiveProvider` `--sounds-bsa` handle that didn't exist (the
existing footstep/splash loads are one-off hardcoded-path reopens, not a
FormID-driven lookup) — both gaps are closed. Item 6's `navm_tiles_resident`
sub-piece is done (item 2 gave it something real to count); its
`ai_package_rows` sub-piece and item 4 itself both turned out to need
more than the plan assumed — item 4's "unblocked by `PersistentRefIndex`"
premise doesn't hold (see its correction: that index cannot see the
temporary, non-persistent actors that are actually affected, and
wouldn't provide state snapshot/restore even if it could) — recommended
as its own design pass, likely sharing a mechanism with EX-14/15 item
C2's already-flagged "reconcile instead of re-spawning" half rather than
being built independently. Remaining: `incidental` playback (needs a
real spatial-emitter design decision, not just dispatch plumbing) and
item 3 (pathfinding — recommended as its own follow-up issue given its
size).

## Verification policy

- Pure selection/delta/ownership rules stay in `cargo test`.
- GPU/game-data checks stay in `docs/smoke-tests` and self-skip missing data.
- Every visual fix records the exact worldspace/grid, camera pose, hour,
  archives, renderer/upscaler mode, entity/draw counts, and output image.
- Performance changes report distributions and worst atomic units, not only
  average FPS.
- Documentation is refreshed only from a green smoke artifact; wiring claims
  without a runnable profile are marked parse/load-only.
