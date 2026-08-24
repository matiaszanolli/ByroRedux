# PHYSAL / Physics Audit — 2026-08-24

**Scope**: `crates/physics/` (world, sync, convert, components, config, ragdoll,
water) + `byroredux/src/ragdoll.rs` + `byroredux/src/systems/character.rs` +
`byroredux/src/systems/water.rs` + `byroredux/src/commands/water.rs` +
`byroredux/src/combat.rs` (the death-reconciliation contract) + the producer
sites in `byroredux/src/cell_loader/spawn.rs` and the parse side
`crates/nif/src/import/collision/`.

**Depth**: deep, solo execution (no sub-agent fan-out this pass — the run was
constrained to a single agent by the invoking task). All seven dimensions
covered directly by reading, grepping, and tracing code; no headless engine
instance was launched (`feedback_no_parallel_engine_launch`).

**Games traced**: the solver path is game-agnostic (doctrine re-verified
below); the water/current path was traced against the FO3/FNV `XWCU`+`XPRM`
producer and the Skyrim wave-authoring defaults; the sensor/collidable path
against the shared classic-bhk `havok_filter` layer-15 route (Oblivion / FO3 /
FNV / Skyrim LE+SE).

**Tests**: `cargo test -p byroredux-physics` — **148 passed, 0 failed**.
`cargo check -p byroredux` — clean. (Workspace-wide `cargo test` is blocked by
an unrelated `E0004` in `crates/scripting/examples/fragment_coverage.rs:59`,
per the task briefing; not physics-owned, not investigated here.)

**Prior pass**: `docs/audits/AUDIT_PHYSICS_2026-08-20.md` (0 CRITICAL / 3 HIGH
/ 3 MEDIUM / 3 LOW). **Delta audited**: `23068af0..HEAD` over the
physics-owned files is now +4297/-335 across 13 files (the 2026-08-20 report's
own delta baseline was a subset of this — 12 more commits landed 2026-08-21
through 2026-08-24, closing most of that report's findings). This pass
re-verified every one of the prior report's 9 findings against current code
and hunted for new defects in the substantially rewritten `water.rs` (+1280
lines) and `world.rs` (+609 lines).

---

## Executive Summary

| Dimension | CRITICAL | HIGH | MEDIUM | LOW |
|---|---|---|---|---|
| 1 — Shape Translation | 0 | 0 | 0 | 0 |
| 2 — Step Determinism & Budget | 0 | 0 | 0 | 0 |
| 3 — ECS Sync | 0 | 0 | 0 | 0 |
| 4 — Ragdoll Articulation | 0 | 0 | 0 | 0 |
| 5 — Character Controller | 0 | 0 | 0 | 0 |
| 6 — Water / Buoyancy | 0 | 0 | 1 | 0 |
| 7 — Queries & Diagnostics | 0 | 0 | 0 | 0 |
| **Total (new)** | **0** | **0** | **1** | **0** |

One new MEDIUM finding. The headline of this pass is not a new defect — it is
that **8 of the 2026-08-20 report's 9 findings are now fixed in code**, verified
line-by-line against the commits that landed 2026-08-21/23/24. The ninth
(a doc-rot comment) remains unfixed and is restated below without a new ID.

### The headline: the 2026-08-21/23/24 commit run closed almost the entire prior report

| Prior finding | Fix commit | Verified |
|---|---|---|
| PHYS-D6-2026-08-20-01 (unbounded current-volume force wind-up) | `04774f7e` | ✅ `reset_forces` hoisted to one call site per body per frame (`water.rs:804-806`) |
| PHYS-D5-2026-08-20-02 (sensors invisible to KCC/ground probes) | `04774f7e` | ✅ `solid_probe_filter()` + `.exclude_sensors()` now shared by `move_character`, `cast_ray_down`, `cast_capsule_down*`; `static_colliders_aabb` skips `c.is_sensor()` |
| PHYS-D4-2026-08-20-03 (water deaths skip AI/ragdoll reconciliation) | `05de6a30` | ✅ both water death sites now call `queue_dead_actor_reconciliation`, drained by a dedicated `Stage::Late` exclusive that calls the shared `reconcile_dead_actor` |
| PHYS-D6-2026-08-20-04 (quiesced fast path unreachable — wave amplitude gate) | `d628acfc` | ✅ replaced with `waves_require_contact_rescan` — gated on an *existing* `WaterContact` near the surface, not "any surface has non-zero amplitude" |
| PHYS-D3-2026-08-20-05 (`physics_sync_system` omits `TotalTime`/`WindField`/`WaterCurrentVolume`) | `5428e872` (today) | ✅ all three now declared (`boot.rs:1267-1268`, `:1286`) |
| PHYS-D6-2026-08-20-07 (`clear_stale_water_contacts` skipped when a current outlives its plane) | (same water rewrite) | ✅ now called whenever `surfaces.is_empty()`, independent of `current_volumes` (`water.rs:604-609`) |
| PHYS-D5-2026-08-20-06 (`swim_vertical_velocity` frame-rate-dependent damping) | — | ❌ **still open** (tracked as #3125) |
| PHYS-D5-2026-08-20-08 (`advance_breath` zero-dt refill) | — | ❌ **still open** (tracked as #3128) |
| PHYS-D3-2026-08-20-09 (`pull_dynamic` lock-ordering comment describes drops 75 lines away) | — | ❌ **still open** (tracked as #3130), doc-only |

Six of nine are fixed; three remain open exactly as filed, unchanged since
2026-08-20 (see Known-Open Register — not re-filed).

### PHYSAL doctrine verdict — HOLDS

`grep -rn "GameKind|bsver|NifVersion|game_kind"` over `crates/physics/src/` and
`byroredux/src/ragdoll.rs` returns comments only (Skyrim-sized `HUMAN` preset
rationale, Oblivion-authored degenerate-axis notes). No game or version
discriminator has leaked into the solver path. The constraint CInfo decode
remains the only per-game seam; `docs/engine/physal.md` §3 still matches the
code, including the FO4+ `BhkSystemBinary` opacity handling, which now routes
through a distinguished `MissingCollisionFallback::PackedAabbProxy` /
`ArchitectureTriMesh` / `None` classification (`cell_loader/spawn.rs:60-90`)
rather than a single "no collision" bucket.

---

## Solver Invariant Matrix

| Invariant | State | Evidence |
|---|---|---|
| Fixed step: accumulator clamped **before** the loop | ✅ VERIFIED | `world.rs:465-469` |
| Negative / NaN `frame_dt` guarded | ✅ VERIFIED | `frame_dt.max(0.0)`, `world.rs:465` |
| Anti-spiral budget starts before substep 1, checked after | ✅ VERIFIED | `world.rs:535`, `:571` |
| `pending_wake` consumed only when a substep ran | ✅ VERIFIED | `world.rs:588-590` |
| Query pipeline rebuilt at most once per frame, never per substep | ✅ VERIFIED | `None` passed to `pipeline.step`; rebuild at `world.rs:624-627` |
| No-step frames still flush a dirty collider set | ✅ VERIFIED | `world.rs:521-524` |
| Determinism: no wall-clock or map order feeds the solver | ✅ VERIFIED | budget `Instant` only truncates the catch-up loop |
| Phase order collect → push → buoyancy → step → pull | ✅ VERIFIED | `sync.rs:127-165` |
| Phase 1 read guards released before write guards | ✅ VERIFIED | `sync.rs:759-806` (collect_newcomers → drop → register_newcomers) |
| Newcomer registration idempotent | ✅ VERIFIED | gate lives in `collect_newcomers` (`sync.rs:759-767`, #2867) |
| Placement scale reaches the collider exactly once, every primitive clamped to a shared ceiling | ✅ VERIFIED | `convert.rs::clamp_shape_extent` (#3238) unifies Ball/Cuboid/Capsule/Cylinder |
| Degenerate shape input cannot reach Rapier | ✅ VERIFIED | `convert.rs` TriMesh/ConvexHull/Compound guards, all covered by tests |
| Contact skin applied at every collider producer | ✅ VERIFIED | `sync.rs:889/909`, `ragdoll.rs:259/271` |
| Sensors excluded from every cast that must see solids | ✅ **RESTORED** | `solid_probe_filter()`, `world.rs:108-111`, used at every probe site |
| Buoyancy force is reset exactly once per body per frame | ✅ **RESTORED** | `water.rs:804-806` (#3114 fix) |
| Buoyancy quiesced fast path reachable in shipping config | ✅ **RESTORED** | `waves_require_contact_rescan`, `water.rs:490-510` |
| Water death → ragdoll handoff is single-sinked | ✅ **RESTORED** | `PendingDeathReconciliations` queue + `reconcile_pending_dead_actors_system` (`combat.rs:71-90`, `:422-433`) |
| Vertical integration is dt-correct | ⚠️ PARTIAL | terrestrial (`integrate_vertical`) yes; swim (`swim_vertical_velocity`) still frame-rate-dependent (#3125, unchanged) |
| Current-volume force wakes the body it's applied to | ❌ **NEW GAP** | see PHYS-D6-2026-08-24-01 |
| PHYSAL: constraint CInfo decode is the only per-game seam | ✅ VERIFIED | no game/version symbol in the solver path |

---

## Findings

### MEDIUM

#### PHYS-D6-2026-08-24-01: A dynamic body resting inside a `WaterCurrentVolume` with no overlapping `WaterPlane` never wakes to receive the current force

- **Severity**: MEDIUM
- **Dimension**: Water / Buoyancy
- **Location**: `crates/physics/src/water.rs:920-937` (the current-flow
  branch) · `crates/physics/src/water.rs:825-831` (the *only* wake site,
  gated to the surface branch) · `crates/physics/src/water.rs:1847-1990`
  (`current_volume_without_a_water_plane_does_not_wind_up_user_force`, whose
  own in-code comment names this exact gap and works around it)
- **Status**: NEW
- **Trigger Conditions**: an authored `XWCU`/current marker (`WaterCurrentVolume`)
  whose box does not overlap any `WaterPlane` in XZ (or overlaps one only
  outside the body's column), containing a dynamic body that is asleep and
  receives no other disturbance — the common case for any clutter/debris that
  streams in already at rest inside the marker (spawn-asleep is the
  EXTERIOR-FREEZE default for every dynamic newcomer, `sync.rs:860-874`).
- **Description**: `apply_buoyancy_with_scratch`'s per-body loop has exactly
  one wake site, and it lives entirely inside the *surface* branch — a
  dry→wet transition wakes the body once
  (`!t.prior_wet || (sleeping && depth changed)`, `water.rs:825-831`). The
  current-volume branch that runs afterward is gated on `!b.is_sleeping()`
  (`water.rs:922`) and, by design, calls `add_force` with `wake_up = false`
  (a correct choice for a per-frame re-derived force — see the comment at
  `water.rs:934`, "no wake — see the surface branch"). But if the body never
  passes through the surface branch at all — because no `WaterPlane`
  overlaps its position — nothing ever wakes it in the first place, so the
  `!b.is_sleeping()` gate on the current branch is permanently false and the
  authored flow is never applied.
- **Evidence**: the crate's own regression test for the *now-fixed*
  unbounded-force sibling of this bug
  (`current_volume_without_a_water_plane_does_not_wind_up_user_force`,
  `water.rs:1847`) constructs precisely this fixture — a `WaterCurrentVolume`
  with no `WaterPlane` anywhere — and its loop body carries this comment
  verbatim:
  ```rust
  // Hold the body awake and armed. Both are load-bearing and neither
  // is incidental to the defect: the current branch is gated on
  // `!is_sleeping()`, and `apply_buoyancy`'s quiesced-scene fast path
  // returns before the per-body scan unless something is awake or
  // `pending_wake` is armed. Nothing in the current-volume path wakes
  // a body itself — only the SURFACE branch calls `wake_up` — so a
  // marker with no overlapping plane needs an independent
  // disturbance, which is exactly this issue's trigger condition.
  {
      let mut pw = world.resource_mut::<PhysicsWorld>();
      let handles = *world.query::<RapierHandles>().unwrap().get(body).unwrap();
      if let Some(b) = pw.bodies.get_mut(handles.body) {
          b.wake_up(true);
      }
      pw.wake();
  }
  physics_sync_system(&world, PHYSICS_DT);
  ```
  The test manually wakes the body every iteration to exercise the force
  math; without that workaround the fixture never enters the current branch
  at all and the (already-fixed) wind-up defect would never have been
  observable either. The gap this finding reports is what's left after that
  workaround is removed: **in the shipping engine nothing plays the role of
  this test's manual `wake_up`**.
- **Impact**: an XWCU/current marker authored where it does not spatially
  coincide with a `WaterPlane` — e.g. a current extending past a river's
  rendered water-plane footprint toward a bank, or a bounding-box mismatch
  between the two records — silently fails to move debris resting in it.
  There is no crash, no force leak, and no visible artifact beyond "the
  current doesn't do anything" for that placement; the common co-located
  case (current marker overlapping its own plane) is unaffected because the
  surface branch's wake fires first and the body then stays awake while
  buoyant. Narrower than the fixed PHYS-D6-2026-08-20-01 sibling, but the
  same root cause family: the current-volume path was added without its own
  wake discipline, only a force-safety one.
- **Related**: PHYS-D6-2026-08-20-01 (CLOSED — the opposite-signed defect in
  the same code path), `watal.md` §7 Phase 2 (current volumes).
- **Suggested Fix**: wake a body once on entering a current volume from rest,
  mirroring the surface branch's one-shot pattern — track a `prior_in_current`
  bit (either on `WaterContact`, which currently only models surface state,
  or a small parallel latch) and call `b.wake_up(true)` / set `woke_any` the
  first frame `current_flow.is_some()` is true for a sleeping body. Extend
  the existing test (or add a sibling) that removes the manual per-iteration
  `wake_up` and asserts the body still accelerates from rest under the
  authored current.

---

## Known-Open Register

The three don't-re-litigate items from the skill file, and what this pass
changed about them:

1. **`tes_grounding_zero_mass_dynamic_fix`** — mass=0 Dynamic Skyrim
   architecture reclassified Static (#1832). Not re-investigated; nothing in
   this delta touches the mass=0 angle. The sensor-exclusion fix
   (PHYS-D5-2026-08-20-02, now closed) removes the second confounder the
   2026-08-20 report flagged against the still-open door-threshold spawn gap.
2. **`interior_spawn_point_fix`** — interiors spawn at the first door's own
   placement; vanilla `coc` has no auto-spawn-point logic. Untouched.
3. **`fnv_furniture_sit_needs_transition`** — sit loops have no pelvis/root
   channel; M42 seat-snap stays behind `BYRO_SANDBOX_SIT`. Untouched.

**Prior-report status** (`AUDIT_PHYSICS_2026-08-20.md`, 9 findings, 0/3/3/3):

- **Fixed and verified at HEAD, this pass** (6): PHYS-D6-2026-08-20-01,
  PHYS-D5-2026-08-20-02, PHYS-D4-2026-08-20-03, PHYS-D6-2026-08-20-04,
  PHYS-D3-2026-08-20-05, PHYS-D6-2026-08-20-07. See the headline table above
  for the fixing commit and verification evidence for each.
- **Still open, unchanged, not re-filed** (3): PHYS-D5-2026-08-20-06 (#3125 —
  `swim_vertical_velocity`'s `prev_velocity * 0.72` per-frame decay is
  unchanged), PHYS-D5-2026-08-20-08 (#3128 — `advance_breath`'s `dt <= 0.0`
  branch still collapses into the full-refill case), PHYS-D3-2026-08-20-09
  (#3130 — the `pull_dynamic` lock-ordering comment at `sync.rs:1135` still
  describes guard drops that actually happen 75 lines earlier at
  `sync.rs:1060-1061`; doc-only, no runtime effect).
- **Tracker note**: `gh issue list` shows #3125, #3128, #3130 still OPEN,
  matching the code state above (no action needed — genuinely unfixed). It
  also shows #3122 (the PHYS-D6-2026-08-20-04 fast-path issue) and what
  appears to be #3238 (the Ball/Capsule/Cylinder extent-clamp gap this
  report's Dimension 1 re-verified as fixed via `clamp_shape_extent`, #3238
  landed 2026-08-24) still marked OPEN despite the fix being present and
  tested at HEAD — worth a housekeeping close pass since the code is already
  right, but not something this audit's remit covers directly.

---

## Cross-Audit Dedup

- **Lock ordering / access declarations** → `/audit-concurrency` Dim 5. Today's
  concurrency pass (2026-08-24, in progress alongside this one) filed
  CONC-D5-2026-08-24-01 through -04 against `physics_sync_system` and its
  neighbors. This pass independently re-derived and confirms two of those
  from the physics side without re-filing:
  - **CONC-D5-2026-08-24-01** (undeclared `Parent`/`ActorBoneCollider` reads
    on `physics_sync_system`) — confirmed live: `sync.rs:1100` reads `Parent`
    inside `pull_dynamic`, and `sync.rs:784` reads `ActorBoneCollider` inside
    `collect_newcomers`; neither appears in the `Access` declaration at
    `boot.rs:1257-1297`, which lists `CollisionShape`/`RigidBodyData`/
    `GlobalTransform`/`RapierHandles`/`Transform`/water components/
    `RenderLayer`/`FormIdComponent`/`PhysicsSourceForm` but not these two.
  - **CONC-D5-2026-08-24-03** (`FormIdPool` resource acquired across storage
    guards in two diagnostics) — one of the two sites is
    `dump_awake_fallers` (`sync.rs:321-341`): `pool =
    world.try_resource::<FormIdPool>()` is taken, then held across
    `layer_q`/`form_q`/`physics_source_q` storage lookups in the loop below.
    Filed there, not re-filed here.
  - This pass did not independently verify CONC-D5-2026-08-24-02
    (`player_water_state` re-locking `TotalTime`+`WindField` inside a loop)
    or -04 (`physics_sync_system` re-entrant from 3 non-scheduler sites) —
    take those from the concurrency report directly.
- **`unsafe`** → none in `crates/physics/`; nothing for `/audit-safety`.
- **Water rendering half** (the shader-side crest
  `authored_wave_height_with_weather` mirrors) → `/audit-renderer` Dim 15.
  Unchanged from the 2026-08-20 report: still asserted only by prose, no
  cross-check test.
- **`bhk*` wire parsing / `havok_filter` decode** → `/audit-nif` Dim 5. The
  producer half of the now-fixed sensor-exclusion finding
  (`havok_filter_is_collidable`) is unchanged and still correct.
- **`CollisionShape` resolution** → `/audit-nifal` Dim 6.
- **The `Dead` → AI/animation half** of the now-fixed water-death
  reconciliation gap overlaps `/audit-ecs` and `/audit-character`; the
  ragdoll-handoff half (verified fixed) is owned here.
- **`XCLW`/`WATR` canonical decode feeding `WaterPlane`** → `/audit-esm` Dim 5.

---

## Recommended Fix Order

1. **PHYS-D6-2026-08-24-01** (this pass's new finding) — narrow scope, one
   wake-site addition mirroring the existing surface-branch pattern.
2. **PHYS-D5-2026-08-20-06** / **PHYS-D5-2026-08-20-08** (#3125 / #3128,
   still open) — both isolated, single-function fixes in
   `byroredux/src/systems/character.rs`; neither depends on the other.
3. **PHYS-D3-2026-08-20-09** (#3130, still open) — doc-only, fold into
   whichever future edit next touches `pull_dynamic`.
4. Housekeeping: close #3122 and (once confirmed) #3238 in the issue tracker
   — both are fixed in code, still open in `gh issue list`.

---

*Report ready.*

```
/audit-publish docs/audits/AUDIT_PHYSICS_2026-08-24.md
```

*(domain label: `physics`; add `water` for the WATAL current-volume finding;
no `game:*` label applies — the trigger is content-shape-dependent, not
per-title.)*

TALLY: CRITICAL=0 HIGH=0 MEDIUM=1 LOW=0
