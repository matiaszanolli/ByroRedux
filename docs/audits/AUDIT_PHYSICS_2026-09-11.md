# PHYSAL / Physics Audit — 2026-09-11 (full 7-dimension pass)

**Run**: `/audit-physics --depth deep`, orchestrator + 7 dimension agents (max 3
concurrent), per `audit-physics/SKILL.md`. **HEAD** at dispatch: `b3db49fa`.

**Scope**: `crates/physics/src/` (world, sync, convert, components, config,
ragdoll, water, lib) + `byroredux/src/ragdoll.rs` +
`byroredux/src/systems/character.rs` + `byroredux/src/commands/{physics,water,scene}.rs`
+ `byroredux/src/boot/schedule/` (Access declarations, split out of `boot.rs`
under `8c5e02aa`) + `byroredux/src/cell_loader/{spawn,unload,references}.rs` +
`byroredux/src/scene.rs` + `byroredux/src/scene/nif_loader.rs` + the parse side
`crates/nif/src/import/collision/` and `crates/nif/src/blocks/collision/constraints.rs`.

**Tests**: `cargo test -p byroredux-physics` — **166 passed, 0 failed,
0 ignored** (0.04s), run once by the orchestrator before dispatch. Dimension
agents additionally ran targeted `cargo test` subsets while verifying specific
fixes (`convert::`, `sync::phase_sync_tests::`, `systems::character::`,
`synthesize_trimesh_tests`) — all green, cited inline in each dimension's
verification.

**Engine**: not launched (`feedback_no_parallel_engine_launch`). No source
file, game file, or GitHub issue was modified by this audit.

**Delta audited**: `2026-09-06..b3db49fa` is **8 commits** touching physics
scope: `7c8347f0` (scale²/prismatic-seed/registration-freshness triple fix),
`4d0226b8` (#3969 wake-discipline doc), `082df3c7` (#3970 PHYSAL seam-inventory
doc), `104bc949` (#3971/#3972 ground-probe screening + gate test), `9823ba2f`
(#4064 parallel-system declaration-completeness guard), `ae17629e` (#3929/#3963/
#3975/#4014/#4015 doc-rot sweep, includes the `awake_counts` rename), `06fa77e3`
(#3973 XZ containment extension), `4c9a5b36` (W1 real-character water
traversal). This is therefore almost entirely a **fix-verification sweep**
against the 2026-09-06 report's 18 findings, not a delta review from a quiet
baseline — every prior HIGH and most MEDIUMs had a same-week fix commit
naming them directly, and this pass's job was to confirm each fix closes what
it claims rather than trust the commit message.

**Orchestrator verification**: every dimension agent was instructed to
independently re-derive fix status from current source (line numbers, actual
diffs, actual test runs) rather than accept the commit message, and to check
GitHub issue state for any newly-flagged item before filing it as NEW. The
orchestrator additionally re-ran `gh issue list` and confirmed all six
carried-forward findings this pass leaves open already have OPEN GitHub
issues (#3968, #3964, #3974, #3965, #3966, #3967) — i.e. they are known,
tracked, un-regressed gaps, not rediscoveries.

---

## Executive Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | **0** |
| MEDIUM | **5** |
| LOW | **3** |
| **Total (new + still-open)** | **8** |

### The result that matters

**The 2026-08-30/09-06 theme — "partial closes, not new bugs" — broke this
pass, in the good direction.** Of the two prior HIGHs and five prior MEDIUMs
this pass re-checked, **six of seven were confirmed genuinely, completely
fixed** on independent re-derivation (not just a passing test, but the actual
mechanism traced end-to-end): the `ref_scale²` packed-Havok proxy cuboid, the
stale-`GlobalTransform` registration bootstrap, the "who applies scale"
doc-rot (all three closed together by `7c8347f0`), the wake-discipline
docstring (`4d0226b8`), the PHYSAL constraint-seam doc rot (`082df3c7`), the
unscreened ground-probe + its untested gate (`104bc949`), the XZ containment
extension (`06fa77e3`), and the `awake_counts` kinematic mislabel (`ae17629e`,
landed the same day this audit ran). Three dimensions (3, 4, 5) closed with
**zero new findings** after their fix-verification — a first for this
subsystem's audit history.

**What remains open is exactly what the fix commits didn't claim to touch.**
Three MEDIUMs from the 2026-09-06 report (`phys.census`'s self-hit, its
disabled 3-way authoring split, and `SpawnCensusEntry`'s AABB-to-Y narrowing —
all Dimension 7, the diagnostic channel) and two more (the undeclared
`Ragdoll` Access read on `physics_sync_system`, and `player_water_state`'s
missing `WaterCurrentVolume` arm — both Dimension 6) were never named in any
of the eight commits' messages, and direct re-reading of the current code
confirms none of the six has moved since 2026-09-06. All six already have
OPEN GitHub issues (filed by the prior audit's publish pass), so nothing here
is a rediscovery — this report's contribution is re-confirming each is still
live at HEAD with fresh line numbers and evidence, and the `#4064` guard's own
blind spot that explains why it didn't catch the `Ragdoll` declaration gap
(D6-1 below).

**Two new findings, both LOW/MEDIUM and both doc/hardening-shaped, not live
bugs.** Dimension 1 found a **dead** `CollisionShape::scaled()` method
(unreachable since `b8c4e6af`, 2026-08-17) whose doc comment still states the
*inverse* of the now-corrected scale contract — an attractive-nuisance
landmine for a future producer, not a live defect (MEDIUM, because it's the
exact documentation shape that has already produced four scale² bugs).
Dimension 1 also flagged the character-controller/ground-probe's ephemeral
capsule shapes, which use a bare `.max(1e-3)` floor with no `clamp_shape_extent`
ceiling — currently fed only by engine constants, so unreachable today, but
worth routing through the shared clamp before any data-driven (CHARAL
per-race height) sizing lands (LOW).

### PHYSAL doctrine verdict — **HOLDS, sixth consecutive pass**

```
$ grep -nE "GameKind|game_kind|bsver|NifVersion|is_skyrim|is_fo4|is_oblivion|is_fo3|game ==|BS_F76|SF_FORM_ID" \
        crates/physics/src/*.rs byroredux/src/ragdoll.rs byroredux/src/systems/character.rs
(no matches)
```

Zero game/version branches on the solver side. The per-game seam remains
confined to the parse-side CInfo decode in
`crates/nif/src/blocks/collision/constraints.rs`, and `docs/engine/physal.md`
§3's inventory is now itself current (fixed by `082df3c7`): four wire types
(`bhkRagdollConstraint`, `bhkLimitedHingeConstraint`, `bhkHingeConstraint`,
`bhkPrismaticConstraint`) → three CInfo structs → three importer functions,
across three wrappers. `bhkBallAndSocketConstraint` / `bhkStiffSpringConstraint`
remain the only undecoded kinds (deliberately deferred per #3792's own
checklist; not re-filed — no fresh occupancy census was run this pass).

### Games traced

Consistent with prior passes: the solver path is game-agnostic by
construction (above), so no per-game trace of the solver was needed. Shape
translation was re-verified against the shared classic-`bhk` producer
(Oblivion/FO3/FNV/Skyrim) and the FO4+/Starfield packed-Havok proxy route
(the `7c8347f0` fix's own regression test uses a synthetic placement, not
live game data — no corpus census was run this pass, consistent with the RAM
constraint noted in every physics audit since 2026-08-30).

### Per-dimension counts

| Dimension | CRIT | HIGH | MED | LOW | New findings this pass |
|---|---:|---:|---:|---:|---|
| 1 — Shape Translation | 0 | 0 | 1 | 1 | 2 (both NEW) |
| 2 — Step Determinism & Budget | 0 | 0 | 0 | 1 | 1 (re-filed, Existing #3968) |
| 3 — ECS Sync | 0 | 0 | 0 | 0 | 0 — both priors CONFIRMED FIXED |
| 4 — Ragdoll Articulation | 0 | 0 | 0 | 0 | 0 — both priors CONFIRMED FIXED |
| 5 — Character Controller | 0 | 0 | 0 | 0 | 0 — both priors CONFIRMED FIXED |
| 6 — Water / Buoyancy | 0 | 0 | 1 | 1 | 2 (re-filed, Existing #3964, #3974) |
| 7 — Queries & Diagnostics | 0 | 0 | 3 | 0 | 3 (re-filed, Existing #3965, #3966, #3967); 1 prior CONFIRMED FIXED (#3975/D7-04) |

---

## Solver Invariant Matrix

| Invariant | Verdict | Note |
|---|---|---|
| Fixed step — accumulator clamps before the loop; `frame_dt.max(0.0)` guards NaN | **HOLDS** | unchanged, re-verified (Dim 2) |
| Anti-spiral budget (#1698) | **HOLDS** | unchanged, re-verified (Dim 2) |
| Static-scene fast path gate, kinematic deliberately excluded | **HOLDS** | unchanged, re-verified (Dim 2) |
| Wake discipline — every motion-starting path wakes, and the doc now says so correctly | **HOLDS** | `4d0226b8` fixed the doc half; `register_newcomers`'s spawn-exempt behavior itself was always correct (Dim 2) |
| Collider-set mutation announces itself via `mark_colliders_dirty` | **DRIFTED (unchanged)** | `register_newcomers` does; `build_ragdoll` still does not — Existing #3968 (Dim 2) |
| Query pipeline rebuilt once per frame, outside the substep loop | **HOLDS** | unchanged (Dim 2) |
| Lock ordering — Phase 1 releases every storage guard before `PhysicsWorld`/`RapierHandles` writes | **HOLDS** | re-traced in full, no new edge (Dim 3) |
| Phase order — collect/register → push kinematic → buoyancy → step → pull dynamic | **HOLDS** | re-traced, `BYRO_PROFILE` labels still correct (Dim 3) |
| Phase 1 precondition — `GlobalTransform` is fresh when a newcomer is registered | **FIXED** | `7c8347f0` made this structurally self-enforced (propagation runs inside `register_newcomers_and_refresh_queries` itself) rather than caller-trusted; all three live call sites traced (Dim 3) |
| Scale applied exactly once, at `collision_shape_to_parts` | **FIXED** | `7c8347f0` removed the third pre-baking producer (`synthesize_packed_havok_proxy`); doc contract corrected in `convert.rs` and `physics.md`; no fourth producer found (Dim 1, Dim 3) |
| Registration idempotency | **HOLDS** | unchanged (Dim 3) |
| Teardown completeness — activate→remove leaks no body/collider/joint | **HOLDS** | unchanged, re-tested (Dim 4) |
| Ragdoll bone→body mapping; writeback targets the same bone | **HOLDS** | unchanged (Dim 4) |
| Z-up→Y-up applied once, upstream at the NIF boundary | **HOLDS** | unchanged (Dim 4) |
| Joint seeding carries the animated pose into reduced coordinates (#2337) | **FIXED** | `7c8347f0` made `seed_joint_from_body_poses` dispatch on joint kind, not `ndofs()`; Prismatic now seeds/clamps the slide distance, not a rotation angle (Dim 4) |
| Scheduler `Access` declares every storage the system reads | **DRIFTED (unchanged)** | `Ragdoll` read (added by #3492) still undeclared on `physics_sync_system`; the new #4064 completeness guard is structurally blind to this cross-file (`sync.rs`→`water.rs`) hop — Existing #3964 (Dim 6) |
| Buoyancy never pins the fast path; `n_new > 0` escape hatch present | **HOLDS** | unchanged (Dim 6) |
| Water containment uses one reference point | **FIXED** | `06fa77e3` extended the AABB-centre fix from Y to X/Z; one shared `reference_point` now feeds the prefilter and both containment predicates (Dim 6) |
| Death reconciliation happens in exactly one place (#3119) | **HOLDS** | both water-death producers funnel through the one `Stage::Late` drain (Dim 6) |
| Ground-probe screens its grounded half on the walkable normal | **FIXED** | `104bc949` split `probe_found_support` (screened) from the raw correction distance (unscreened, deliberately); gate extracted to a tested pure `support_probe_enabled` (Dim 5) |
| Diagnostics do not lie | **PARTIALLY DRIFTED** | the `awake_counts` mislabel is fixed (`ae17629e`); the census self-hit, disabled 3-way split, and Y-only AABB narrowing are unchanged — Existing #3965/#3966/#3967 (Dim 7) |

---

## Findings — MEDIUM

### PHYS-D1-2026-09-11-01: Dead `CollisionShape::scaled()` carries a doc comment that contradicts the corrected scale contract and would reintroduce a fixed bug class if followed

- **Severity**: MEDIUM
- **Dimension**: Shape Translation
- **Location**: `crates/core/src/ecs/components/collision.rs:51-113`
- **Status**: NEW
- **Trigger Conditions**: a future engineer, debugging a "collider looks the wrong size for a scaled instance" report, reads `CollisionShape::scaled`'s doc comment ("any site that turns an authored shape into a collider for a scaled instance must pass it through here first, or the collider silently keeps bind-scale proportions") and calls `.scaled(global_scale)` on a shape before handing it to a producer that (like every current producer) still passes through `collision_shape_to_parts`/`ragdoll::build_ragdoll`, which apply `GlobalTransform::scale` again.
- **Description**: `CollisionShape::scaled()` was added in `264f44fd` (2026-08-14, fixing #2868) as the ragdoll producer's scale-application mechanism. `b8c4e6af` (2026-08-17) removed that call site — `activate_ragdoll` now defers scaling entirely to the shared PHYSAL converter, with an in-code comment explaining why ("pre-scaling here produced scale² limb geometry while the joint pivots below remained scale¹"). That removal made `.scaled()` **dead code**: the only remaining non-test call sites are its own recursive call for `Compound` children (reachable only from itself) and an unrelated `RadiantIntensity::scaled` in `render/lights.rs`. Its doc comment was never updated to reflect either that its own producer stopped calling it, or that the engine-wide contract flipped to "no producer pre-bakes scale; the shared converter applies it exactly once" — the exact contract `7c8347f0` just had to state explicitly in `convert.rs` and `physics.md` because the *old, wrong* version of that contract (which this method's doc still embodies) is what let the sibling `#3959` bug survive four review passes.
- **Evidence**: `crates/core/src/ecs/components/collision.rs:58-60` — *"any site that turns an authored shape into a collider for a scaled instance must pass it through here first, or the collider silently keeps bind-scale proportions (#2868)"* — directly contradicts `crates/physics/src/convert.rs:139-145`'s current, corrected contract ("producers keep the shape in local units; this function applies `GlobalTransform::scale` exactly once. Baking it on both sides yields scale² geometry ... and #3959, which was exactly that"). `git log -S"pub fn scaled"` shows it added `264f44fd`; `git log -S"retain canonical authored geometry" -- byroredux/src/ragdoll.rs` shows the call site removed `b8c4e6af` — both predate this audit by 3-4 weeks and survived five prior physics audit passes (08-24, 08-29, 09-05, 09-06, 09-10). `grep -rn "\.scaled(" --include="*.rs" .` confirms no production caller anywhere in `byroredux/src`, `crates/nif/src`, or `crates/physics/src`.
- **Impact**: currently zero — unreachable code, no live bug. The risk is entirely prospective: this is the exact documentation shape that already produced #2868 and independently #3064/#3065/#3959 (three separate producers hit the identical scale² mistake before `7c8347f0` closed the third). A doc comment instructing the *opposite* of the now-canonical contract, on a public method in `core` any producer can call, is a standing invitation to regress a bug class that has already cost four fix passes.
- **Related**: same bug family as CONFIRMED-FIXED PHYS-D1-2026-09-06-01 (#3959), and its siblings #2868, #3064, #3065.
- **Suggested Fix**: either delete `CollisionShape::scaled()` (and its tests) now that it has no caller — `convert.rs`'s own comment already argues against "a list of producers maintained on the consumer" style duplication, and this method is exactly that kind of duplicate scale-application surface — or, if a future direct-Rapier producer is expected to bypass the shared converter, rewrite the doc to state plainly that it must never be combined with `collision_shape_to_parts`/`ragdoll::build_ragdoll`.

### PHYS-D6-2026-09-11-01: `physics_sync_system`'s `Ragdoll` read is undeclared, and the new #4064 completeness guard is structurally blind to it

- **Severity**: MEDIUM
- **Dimension**: Water / Buoyancy (ECS access declaration)
- **Location**: `byroredux/src/boot/schedule/physics.rs:7-49` (declaration); `crates/physics/src/water.rs:487,747` (the actual reads); `byroredux/src/boot/schedule/mod.rs:400,419-440` (the guard's blind spot)
- **Status**: Existing: #3964 (OPEN) — re-verified still open at HEAD, NOT fixed by `9823ba2f` (#4064)
- **Trigger Conditions**: any code path that treats `physics_sync_system`'s declared `Access` as its true read/write set (currently only `install_runtime_registries`'s same-stage-parallel-batch deadlock proof), combined with a live game world that has `Ragdoll` bodies.
- **Description**: `physics_sync_system` calls `crate::water::apply_buoyancy(world, n_new > 0)` (`sync.rs:150`), which reaches `world.query::<Ragdoll>()` twice — once in the per-body buoyancy scan (`water.rs:747`) and once in `clear_stale_water_contacts` (`water.rs:487`). Neither is declared in `byroredux/src/boot/schedule/physics.rs`'s `Access` list. Commit `9823ba2f` (2026-09-08, "Fix #4064: check declaration completeness on the parallel systems") built a completeness guard specifically to catch this shape of gap, but the guard's own per-system source table (`PARALLEL_SYSTEMS` in `boot/schedule/mod.rs:436-439`) lists only `crates/physics/src/sync.rs` as `physics_sync_system`'s source; `apply_buoyancy` is defined in `water.rs`, a different file, so the guard's call-follower looks for `apply_buoyancy`'s body inside `sync.rs`'s text, finds nothing, and silently drops the hop — it never scans `water.rs` and therefore never sees the `Ragdoll` acquisition. The test passes green, but it is not exercising the real acquisition surface for this system.
- **Evidence**: `sync.rs:150` — `crate::water::apply_buoyancy(world, n_new > 0);`; `water.rs:747` and `:487` — `world.query::<Ragdoll>()`; `boot/schedule/physics.rs:7-49` — no `Ragdoll` mention anywhere in the declared `Access`; `boot/schedule/mod.rs:436-439` — `("physics_sync_system", &[(PHYSICS_SYNC_SRC, "physics_sync_system")])`, the only source is `sync.rs`.
- **Impact**: not a live conflict today — no other same-stage parallel system currently touches `Ragdoll` besides the harmless `world.register::<Ragdoll>()` setup call. The exposure is exactly what #4064's own commit message warned about for the nine systems it *does* cover: "an under-declared parallel system yields `known_conflict_count() == 0` while a real write/write overlap sits in the batch" — that protection simply doesn't run for this acquisition, so a future parallel system added to `Stage::Physics` reading/writing `Ragdoll` would pass the guard's own self-check and still not be flagged as a conflict.
- **Related**: #3492 (added the `Ragdoll` arm to the buoyancy scan), #4064/`9823ba2f` (added the completeness guard but didn't extend its per-system source table to cover this hop), #3951/#3473 (the guard's original precursor), PHYS-D3-2026-08-20-05 (a prior instance of this exact declaration-completeness class on the same system, fixed once already).
- **Suggested Fix**: two independent fixes, either sufficient alone: (1) add `.reads::<byroredux_physics::Ragdoll>()` to the `physics_sync_system` registration in `boot/schedule/physics.rs`; (2) extend `PARALLEL_SYSTEMS`'s `physics_sync_system` entry to also scan `water.rs` (and `ragdoll.rs`) so the guard's own scan would have caught this and will catch the next occurrence.

### PHYS-D7-2026-09-11-01: Census re-sweep still self-hits the player's own capsule

- **Severity**: MEDIUM
- **Dimension**: Queries & Diagnostics
- **Location**: `crates/physics/src/sync.rs:646-652` (call), `:479-497` (`SpawnCensusProbe` — no exclusion field)
- **Status**: Existing: #3965 (OPEN) — re-verified still open at HEAD, not fixed
- **Trigger Conditions**: `phys.census` (or the boot-time door-teleport `dump_spawn_collider_census` path) run while `PlayerMode::Character` is active (a real `KinematicPositionBased` player capsule exists in Rapier) and the census column's XZ/Y probe origin lands inside or just above that capsule.
- **Description**: `spawn_collider_census_report` re-runs its walkability verdict sweep via `pw.cast_capsule_down_surface_and_normal(..., None)` with a literal `None` for `excluded_body`. `SpawnCensusProbe` carries no field through which either live caller (`commands/physics.rs`, `scene.rs`) could supply an exclusion even if it wanted to — and `commands/physics.rs::PhysCensusCommand` deliberately resolves the player as the reference point ("the operator runs this command *because* the thing that fell through the floor is the player"), which is exactly the scenario the mandatory-`excluded_body` doc on the underlying cast warns about (a coaxial self-hit returns `toi≈0`, beating every real floor).
- **Evidence**: `sync.rs:646-652` shows the literal `None` fifth argument; `world.rs:799-807`'s own doc states `excluded_body` "must be passed whenever the origin can lie inside a body ... a silent self-hit is invisible (#2859)"; every sibling probe (`scene.rs::probe_walkable_floor_near`, the character controller's own per-frame cast) threads a real handle.
- **Impact**: `phys.census` run against the player — the documented common case — can report a phantom `Walkable` floor at roughly the player's own capsule centre instead of the real floor beneath, misdirecting exactly the debugging session the command exists for.
- **Related**: #2874 (built the unfiltered re-sweep), #2869 (where the sibling probe learned to exclude for this reason).
- **Suggested Fix**: add `exclude: Option<RigidBodyHandle>` to `SpawnCensusProbe`, thread it to the cast, and have `PhysCensusCommand` populate it from the resolved player's `RapierHandles`.

### PHYS-D7-2026-09-11-02: `phys.census`'s 3-way authoring split is still disabled on a false premise

- **Severity**: MEDIUM
- **Dimension**: Queries & Diagnostics
- **Location**: `byroredux/src/commands/physics.rs:119-133`
- **Status**: Existing: #3966 (OPEN) — re-verified still open at HEAD, not fixed
- **Trigger Conditions**: any `phys.census` invocation whose column comes up with zero colliders — the exact case the 3-way split exists to resolve.
- **Description**: the code still hard-codes `authoring: None` with the comment "The live path has no NIF import cache to sum." That premise is false: `NifImportRegistry` is a live, process-lifetime `World` resource (inserted at boot, `boot/world.rs:211`), read directly by a sibling console command in the same module tree (`commands/assets.rs:523`) and by the boot-time census caller (`scene.rs:667-669`) with the identical one-line lookup.
- **Evidence**: `commands/physics.rs:119-133` (current, unchanged); `commands/assets.rs:509-525`; `scene.rs:659-683`; `boot/world.rs:211`; `cell_loader/nif_import_registry.rs:445` (`collision_authoring_totals`, `pub(crate)`, directly callable).
- **Impact**: every `phys.census` run against a cell with zero colliders in the column loses the "nothing authored" vs "authored but dropped in translation" distinction on the one path an operator actually runs interactively — the boot-time path gets the real diagnosis, the live/interactive path does not.
- **Related**: #2874 (built the 3-way split this disables).
- **Suggested Fix**: `let authoring = world.try_resource::<crate::cell_loader::NifImportRegistry>().map(|r| r.collision_authoring_totals());` before building `probe`, then pass `authoring` instead of `None` — identical to `scene.rs:667-669`.

### PHYS-D7-2026-09-11-03: `SpawnCensusEntry` still discards X/Z extents and has no "wrong size" verdict

- **Severity**: MEDIUM
- **Dimension**: Queries & Diagnostics
- **Location**: `crates/physics/src/sync.rs:539-552` (struct), `:668-696` (construction, Y-only), `:779-793` (render, Y-only), `:506-516` (`SpawnProbeVerdict` — no size arm)
- **Status**: Existing: #3967 (OPEN) — re-verified still open at HEAD, not fixed
- **Trigger Conditions**: any investigation of a collider whose XZ footprint (not just Y extent) is wrong — e.g. a scale² regression of the class just fixed at PHYS-D1-2026-09-06-01/#3959, or any future placement-scale drift.
- **Description**: `NearbyCollider` carries a full 3-D `aabb_min`/`aabb_max`; `SpawnCensusEntry` keeps only `center_y`/`min_y`/`max_y`, and the render line prints only the Y range. No expected/authored scale is surfaced for comparison, and `SpawnProbeVerdict`'s three arms (`NoHit`/`RejectedNonWalkable`/`Walkable`) have no arm for "hit something, but its footprint doesn't match."
- **Evidence**: re-read `sync.rs:539-801` in full; unchanged since 2026-09-06 — confirmed via `git show ae17629e -- crates/physics/src/sync.rs`, whose only diff there is the unrelated `active_island_counts` rename.
- **Impact**: the exact scale² bug class this audit confirmed fixed at PHYS-D1-2026-09-06-01 remains, and would remain, completely invisible through this diagnostic channel — `phys.census` shows a collider present and looking Y-normal with no signal that its X/Z footprint is wrong.
- **Related**: PHYS-D1-2026-09-06-01/#3959 (the bug class this gap would hide), #2202/#2874/#2875.
- **Suggested Fix**: carry the full `aabb_min`/`aabb_max` through `SpawnCensusEntry`, print X/Z half-extents alongside the Y range, and (longer-term) surface an authored/expected extent for direct comparison.

---

## Findings — LOW

### PHYS-D1-2026-09-11-02: Character-controller/ground-probe capsule shapes use a floor-only `.max(1e-3)`, not `clamp_shape_extent`

- **Severity**: LOW
- **Dimension**: Shape Translation
- **Location**: `crates/physics/src/world.rs:986`, `:1184-1185`
- **Status**: NEW
- **Trigger Conditions**: `CharacterMoveParams::capsule_half_height`/`capsule_radius`, or a ground-probe caller's equivalent, becomes `f32::INFINITY` (NaN is already handled correctly by IEEE `maxNum` semantics; `INFINITY.max(1e-3) == INFINITY` is not).
- **Description**: these two sites build an ephemeral `SharedShape::capsule_y` for a character-controller move / ground-probe cast — not a persistent `CollisionShape`-derived collider, so they never go through `collision_shape_to_parts` (which is fully clean per Dimension 1's checklist item 7). They are the only other `capsule_y(...).max(1e-3)` pattern in the crate with no ceiling clamp analogous to `clamp_shape_extent`. Traced the values back to `CharacterController`'s engine-authored HUMAN-preset constants — not untrusted NIF/ESM content — so practical exploitability today is low, but there is no ceiling the way `clamp_shape_extent` provides everywhere else, and project memory records a planned CHARAL-driven per-race height scale that would make these values data-derived.
- **Evidence**: `SharedShape::capsule_y(capsule_half_height.max(1e-3), capsule_radius.max(1e-3))` at both sites vs. the guarded `clamp_shape_extent` used throughout `convert.rs`.
- **Impact**: none currently observable; a future data-driven height value without validation could hand the broad-phase an unbounded capsule for character-controller sweeps specifically.
- **Related**: none — distinct from the `CollisionShape` producer family.
- **Suggested Fix**: if/when character capsule dimensions become data-derived, route them through `clamp_shape_extent` or an equivalent; not urgent while the values are engine constants.

### PHYS-D2-2026-09-11-01: `build_ragdoll`'s collider insert still never calls `mark_colliders_dirty()`; the query-BVH visibility gap for a fresh ragdoll still depends on an unrelated teardown loop's side effect

- **Severity**: LOW
- **Dimension**: Step Determinism & Budget
- **Location**: `crates/physics/src/ragdoll.rs:322` (insert) and `:417` (`pw.wake()`); gate at `crates/physics/src/world.rs:579-586`/`:683-686`; indirect coverage at `byroredux/src/ragdoll.rs:433-458` (#1772 teardown), `crates/physics/src/world.rs:257-278` (`remove_body`)
- **Status**: Existing: #3968 (OPEN) — re-verified still open at HEAD; none of the eight commits touched between 2026-09-06 and 2026-09-11 touch `ragdoll.rs`'s `build_ragdoll` function
- **Trigger Conditions**: a ragdoll activated via `activate_ragdoll` on an actor whose bones have no live `RapierHandles` at activation time (so the #1772 teardown's `bone_handles` collection is empty and never runs `remove_body`), on a frame where `accumulator < PHYSICS_DT` (the common case above 60 fps).
- **Description**: `mark_colliders_dirty()` is the sole documented mechanism (since #2864) for getting a collider-set mutation into the query BVH on a frame that runs zero substeps. `build_ragdoll`'s per-part collider-insert loop calls only `pw.wake()`, which clears the static-scene fast-path skip but does not force the substep loop to execute — so on a sub-tick frame the post-loop rebuild gate (`if steps > 0 || self.colliders_dirty`) evaluates false, and the query pipeline is not rebuilt. In the common production path, `activate_ragdoll`'s #1772 teardown (removing the actor's pre-existing keyframed bone followers) incidentally marks the pipeline dirty as an unrelated side effect, covering the gap — but only when that teardown's `bone_handles` set is non-empty.
- **Evidence**: `grep -c "mark_colliders_dirty" crates/physics/src/ragdoll.rs` → 0. The 2026-08-20 audit's disproof text ("Inert: `build_ragdoll` calls `pw.wake()`, so the next `step` always takes the substep path") is confirmed false by direct trace and remains uncorrected in `docs/audits/AUDIT_PHYSICS_2026-08-20.md:568-573`.
- **Impact**: bounded — the common activation path is covered by the #1772 side effect; the residual exposure is a ≤ one-tick window where a fresh corpse's colliders are absent from ray/shape casts, plus a live false statement in the audit trail that could mislead a future maintainer who reorders or removes the #1772 teardown.
- **Related**: #2864, #2856, #1772, #2863, #3969 (the sibling wake-discipline doc finding, fixed by `4d0226b8`).
- **Suggested Fix**: add `pw.mark_colliders_dirty();` beside `pw.wake();` in `build_ragdoll`, making the function self-sufficient; separately correct the 2026-08-20 audit's disproof text.

### PHYS-D6-2026-09-11-01: `player_water_state` still has no `WaterCurrentVolume` arm

- **Severity**: LOW
- **Dimension**: Water / Buoyancy
- **Location**: `byroredux/src/systems/character.rs:1043-1106`; contrast `crates/physics/src/water.rs:843-858`
- **Status**: Existing: #3974 (OPEN) — re-verified still open at HEAD, untouched by the W1 traversal work (`4c9a5b36`)
- **Trigger Conditions**: player capsule swimming inside an authored `WaterCurrentVolume` (XWCU rapids marker) that doesn't overlap a `WaterPlane`, or does but whose flow differs from the marker's.
- **Description**: `player_water_state` queries only `WaterPlane`/`WaterVolume`/`WaterFlow` — never `WaterCurrentVolume`, the separate placed-marker current type the dynamic-body buoyancy scan already handles as a distinct source (`water.rs:843-858`). `4c9a5b36` ("close W1 — real-character water traversal") added the swim-down spring fix, surface ascent clamp, grounded-misread fix, and terrestrial-carry-velocity reset — none touch flow-source selection.
- **Evidence**: `character.rs:24` imports no `WaterCurrentVolume`; `grep -n "WaterCurrentVolume" byroredux/src/systems/character.rs` → no hits.
- **Impact**: a swimming player in an authored rapids marker drifts differently from every dynamic body (crates, ragdolls) in the same volume, which do pick up the marker's flow. Gameplay-visible parity gap, not a crash or corruption risk.
- **Related**: `docs/engine/watal.md`, `water.rs:843-858` (the dynamic-body counterpart this lacks).
- **Suggested Fix**: add a `WaterCurrentVolume` query to `player_water_state` mirroring the dynamic-body containment test (reference point = capsule centre), folding its flow into `PlayerWaterState.flow` with the same surface-wins-over-current precedence the buoyancy scan uses.

---

## Confirmed-Fixed Register (informational — not counted in totals)

These were explicitly re-derived from current code/tests by this pass, not
accepted from a commit message, and should not be re-filed by a future audit
absent a regression:

| Finding | Fixed by | Re-derivation summary |
|---|---|---|
| PHYS-D1-2026-09-06-01 (`ref_scale²` packed-Havok proxy) | `7c8347f0` | pre-bake removed from `spawn.rs`; ghost parented at scale 1.0 under a scaled `placement_root`; doc contract corrected; end-to-end regression test (`packed_proxy_placement_scale_is_applied_once_end_to_end`) spawns → propagates → syncs → reads the real Rapier AABB; no fourth scale-baking producer found (Dim 1, Dim 3) |
| PHYS-D3-2026-09-06-01 (stale-`GlobalTransform` registration bootstrap) | `7c8347f0` | `register_newcomers_and_refresh_queries` now runs propagation internally before collecting; producer seeds the composed `rest_pose` instead of `IDENTITY`; all three live call sites traced; regression test passes (Dim 3) |
| PHYS-D3-2026-09-06-02 (scale contract stated backwards) | `7c8347f0` | `convert.rs` rustdoc and `docs/engine/physics.md` both corrected; stale per-producer census deleted rather than patched (Dim 1, Dim 3) |
| PHYS-D4-2026-09-06-01 (`seed_joint_from_body_poses` dispatched on `ndofs()`) | `7c8347f0` | now dispatches on `&RagdollJointSpec` exhaustively; `Prismatic` seeds/clamps the along-rail translation, not a rotation angle; two new tests including a falsifiable twisted-but-unslid case (Dim 4) |
| PHYS-D4-2026-09-06-02 (PHYSAL/ecs.md doc rot) | `082df3c7` | `physal.md` §3 now states 4 wire types / 3 CInfo structs / 3 importers with Hinge+Prismatic rows; `ecs.md` splits the non-overlap claim to correctly scope `ragdoll_writeback_system` (Dim 4) |
| PHYS-D2-2026-09-06-02 (wake docstring falsely required spawn) | `4d0226b8` | `wake()`/`pending_wake()` docstrings now state the spawn exemption explicitly, cross-reference `had_newcomers`, pinned by 3 source-text tests (Dim 2) |
| PHYS-D5-2026-09-06-01 (unscreened ground-probe grounded half) | `104bc949` | `probe_found_support` now screened on `min_walkable_normal_y`; raw correction distance deliberately stays unscreened (anti-drift); `cast_capsule_down`'s unfiltered form has zero production callers left (Dim 5) |
| PHYS-D5-2026-09-06-02 (untested 3-clause suppression gate) | `104bc949` | gate extracted to pure `support_probe_enabled(swimming, was_grounded, jump_fired)`; truth-table test + source-inspection guard against re-inlining (Dim 5) |
| PHYS-D6-2026-09-06-01/#3973 (XZ containment split) | `06fa77e3` | one shared `reference_point = collider.compute_aabb().center()` now feeds the union prefilter and both containment predicates on all three axes (Dim 6) |
| PHYS-D6-2026-09-06-02/#3963 (audit ground-truth table wrong) | `ae17629e` | `AUDIT_PHYSICS_2026-09-04.md`'s #3490 row corrected to "FIXED (Y axis only)" with a pointer to #3973 (Dim 6) |
| PHYS-D7-2026-09-06-04 (`awake_counts` kinematic mislabel) | `ae17629e`/#3975 | accessor renamed `active_island_counts`, doc corrected, both render sites relabeled, source-inspection regression test added — landed the same day this audit ran (Dim 7) |

---

## Known-Open Register

### The three don't-re-litigate items

| Item | What this pass did |
|---|---|
| **`tes_grounding_zero_mass_dynamic_fix`** (mass=0 angle CLOSED; door-threshold spawn gap open) | Not re-filed. Dimension 5 re-confirmed the mechanism end-to-end (collider present, cast hits, controller grounded all check out); the remaining gap is a content/collision-import question, unchanged. |
| **`interior_spawn_point_fix`** | Untouched; no finding assumes automatic spawn-point logic exists. |
| **`fnv_furniture_sit_needs_transition`** | Outside every dimension's file set this run. |

### Deliberately deferred, verified still deferred — not gaps

- **`bhkBallAndSocketConstraint` / `bhkStiffSpringConstraint`** remain the only undecoded constraint kinds (both bare and breakable-wrapped fall into `Other`). Deferred by #3792's own checklist; no fresh occupancy census was run this pass, so not re-filed.
- **FO4+/FO76/Starfield ragdolls** blocked on the opaque `BhkSystemBinary`/`hknpCompressedMeshShapeData` payload (#3809). Verified still reported as blocked (not "no collision") via `needs_packed_havok_fallback()`/`summarize_collision_authoring()` at four live call sites (Dim 4).
- **WATAL open items**, re-read from `docs/engine/watal.md` rather than inherited: water-walking and freezing, the exact Skyrim DNAM tail decode, the cross-game visual smoke matrix. None filed. (Character swimming and the W1 real-traversal defects are now shipped and verified regression-clean, not open — see Dim 6.)

### Open issues verified still true, cited not re-filed

- **#3477** — `collect_newcomers` rescans every `CollisionShape` row per tick. Owned by `/audit-performance`; not independently re-verified this pass (out of the assigned dimension scope), cited only.
- **#3809** — FO4 precombine collision / `BhkSystemBinary` decoder research spike.

---

## Cross-Audit Deduplication

| Topic | Owner | Note |
|---|---|---|
| Lock ordering / `Access` declaration completeness as a class | `/audit-concurrency` | PHYS-D6-2026-09-11-01 filed here because `physics_sync_system`/`water.rs` is this audit's own scope; the physics-crate lock discipline itself traced clean (Dim 3) |
| `unsafe` blocks | `/audit-safety` | this file set contributes none |
| Water rendering, wave shader, foam/shoreline | `/audit-renderer` Dim 15 | — |
| `XCLW` tri-state / WATR DNAM decode | `/audit-esm` Dim 5 | — |
| `bhk*` block parsing | `/audit-nif` Dim 5 | the constraint decode itself is in-scope here as the PHYSAL source seam |
| `CollisionShape` translation as a NIFAL boundary | `/audit-nifal` Dim 6 | the dead `CollisionShape::scaled()` finding (PHYS-D1-2026-09-11-01) is filed here because it's a solver-scale-contract landmine, not a NIFAL translation defect |
| `collect_newcomers` per-tick rescan (#3477) | `/audit-performance` | cited, not re-verified |
| Debug-server command surface as a whole | un-owned (see `_audit-common.md`) | — |

---

## Verification

- Every dimension agent was instructed to re-derive fix status from current
  source at exact line numbers and, where feasible, re-run the relevant
  `cargo test` subset directly rather than accept a commit message — the
  full-crate suite (`cargo test -p byroredux-physics`, 166/166) was run once
  by the orchestrator before dispatch, and targeted subsets (`convert::`,
  `sync::phase_sync_tests::`, `systems::character::` 41/41,
  `synthesize_trimesh_tests` 15/15) were re-run inside individual dimensions
  while verifying specific fixes.
- All six carried-forward "still open" findings were cross-checked against
  `gh issue list` and confirmed to already have an OPEN tracking issue
  (#3968, #3964, #3974, #3965, #3966, #3967) — none is a rediscovery.
- The two NEW findings (dead `CollisionShape::scaled()`, unclamped controller
  capsule shapes) were checked against the same issue list by keyword search
  and found unfiled.
- No corpus census was run and no engine was launched, consistent with the
  RAM constraint documented since 2026-08-30.

## Summary

Eight open findings (0 CRITICAL, 0 HIGH, 5 MEDIUM, 3 LOW), against eleven
independently-confirmed fixes from the 2026-09-06 report's eighteen. This is
the first pass in this subsystem's audit history where three of seven
dimensions (ECS Sync, Ragdoll Articulation, Character Controller) closed with
*zero* new findings after fix-verification — the "partial closes, not new
bugs" pattern the 2026-08-30/09-06 passes identified did not repeat for those
three fixes. It did repeat, unchanged, for the three Dimension-7 diagnostic
findings and the two Dimension-6 findings: none of the eight fix commits
between 2026-09-06 and 2026-09-11 named them, and none has moved. The two new
findings are both prospective/hardening (a dead API with a landmine doc
comment; an unclamped ephemeral shape fed only by constants today) rather
than live defects — consistent with a subsystem whose fix velocity has, for
the first time across six consecutive passes, outpaced its partial-close
rate on the dimensions that were actively worked.
