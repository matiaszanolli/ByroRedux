**HEAD**: f97775ca8 · **Baseline**: `docs/audits/AUDIT_PHYSICS_2026-09-11.md` (HEAD `b3db49fa`) · **Audited**: Dim 1 Shape Translation, Dim 2 Step & Sync, Dim 3 Ragdoll & Constraint Seam, Dim 4 Character & NPC Controller, Dim 5 Water / Buoyancy, Dim 6 Queries & Diagnostics · **Unchanged since baseline (skimmed)**: none at dimension level (every dimension had commits); byte-unchanged files inside audited dimensions, spot-checked: `crates/physics/src/convert.rs`, `crates/physics/src/config.rs`, `crates/physics/src/water.rs`

# PHYSAL / Physics Audit — 2026-09-21

**Run**: `/audit-physics` (default scope, deep), one leg of `/audit-suite --preset comprehensive`. All six
dimensions were analysed sequentially by one auditor (no sub-agents, per the suite constraint). Per-dimension
scratch notes: `/tmp/audit/physics/dim_{1..6}.md` + `meta.md` (kept for the orchestrator; the skill's
Phase 3 `rm -rf` was deliberately skipped).

**Delta audited**: `b3db49fa..f97775ca8` has **21 commits** in physics scope. The load-bearing ones are
`613ad3124` (new per-substep explosion recovery in `PhysicsWorld::step` + six FO3 data-gated ragdoll tests),
`913fd39d8` (M42.10: NPC locomotion drives XZ through Rapier's KCC), `00a8a1eef` (ragdoll-local ×12 solver
iterations, unconditional follower-recipe strip, `ragdoll.status`), `53bdad9c7` (#3964–#3967), `bb0a5c1ff`
(#3968), `588ed7de2` (#3974), `c03d6d6fe` (`capsule_overlaps_solid`), `1cc47a165` (phantom water bounds),
`5172f5e26` (#4212 constraint CInfo decode), `726a2e930`/`07f2a5f78` (#4407/#4163 plane counter).

**Tests** (all through `cargo test`, `-j 4`, `TMPDIR=/mnt/data/tmp`):

| Lane | Result |
|---|---|
| `cargo test -p byroredux-physics` | **175 passed**, 0 failed, 0 ignored (baseline 166) |
| `cargo test -p byroredux --bin byroredux -- ragdoll` | 26 passed, **6 ignored** (`ragdoll_installed_tests.rs`, FO3 data; not run — each parses `Fallout3.esm` up to three times) |
| `… -- character` / `-- locomotion` / `-- water` | 54 / 4 / 132 passed (water: 2 ignored) |
| `… -- scheduler_access` / `-- rapier_release` / `-- npc_spawn` | 20 / 9 / 87 passed (npc_spawn: 9 ignored) |
| `cargo test -p byroredux-nif --lib -- collision` | 152 passed |

Two standalone release micro-benchmarks (rapier3d 0.22 `simd-stable`, code copied from `world.rs`) were built
outside the repo (`/tmp/audit/physics/bench{,2}`, target dir `/mnt/data/tmp`) to measure D2-01 and D6-02.
Machine load was ~17 during measurement; numbers are for ratio, not absolute budget. **Engine not launched;
no repo file other than this report was written.**

---

## Executive Summary

| Severity | NEW | Regression | Existing (matched, open) |
|---|---:|---:|---:|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | **4** | 0 | 0 |
| LOW | **7** | 0 | 1 (#4134, extended) |

### The result that matters

**The new explosion recovery (`613ad3124`) works, but it hides what it recovers from and costs more than it
should.** The per-substep snapshot/restore is correct on its main path. But:

- it is **invisible to every stability gate** (D3-01). The FO3 data tests, `ragdoll.status` and the p2 smoke all
  read post-recovery state. The project's own `playable-vertical-slice.md` records that the FO3 P2 PASS came
  with one recovery per restore, i.e. a corpse whose multibody was detached. The first-solve root cause is not
  tracked anywhere.
- it **walks the whole body arena on every substep** (D2-01): 0.45 ms at 30 k bodies and 1.8 ms at Cydonia's 95 k.
  On a 30 k-body world with one awake body that is about 10× the solver's own step, and it is paid on essentially
  every frame in a populated cell (interiors: ~5 µs).
- it leaves **three edge cases** uncovered (D2-02).

**The dominant per-frame physics cost is self-inflicted** (D6-02). The code rejected passing the query pipeline to
`pipeline.step` because that "rebuilt the whole tree up to 5× per frame". Rapier 0.22 actually maintains it
**incrementally** there. Measured: 2.78 → 0.09 ms/frame at 30 k colliders, and 9.59 → 0.10 ms at 95 k. The engine
pays a full QBVH `clear_and_rebuild` every stepped frame instead.

**The #3966 fix revived the census's three-way split on the wrong scope** (D6-01). It now feeds
process-lifetime registry totals (≤2048 NIFs from every visited cell), presented as "the cell's NIFs". So every
empty column reads "DROPPED IN TRANSLATION".

The remaining LOWs are M42.10 / #3974 contract gaps (D4-01, D4-02, D5-01), a release-only guard (D1-01), one doc
pointer (D2-03) and skill drift (META-01).

### PHYSAL doctrine verdict — **HOLDS, seventh consecutive pass**

```
$ grep -nE "GameKind|game_kind|bsver|NifVersion|is_skyrim|is_fo4|is_oblivion|is_fo3|is_fnv|game ==|BS_F76|SF_FORM_ID|havok_scale|GameVariant" \
    crates/physics/src/*.rs byroredux/src/ragdoll.rs byroredux/src/systems/{character,locomotion,water}.rs byroredux/src/commands/physics.rs
(no matches)
```

The three named seams stay at the parse→canonical boundary. The constraint CInfo decode now covers
7 wire types → 6 CInfo structs → 3 importers (`docs/engine/physal.md` §3 is current). `havok_scale_for` and
collision-object-kind dispatch are unchanged. Ball-and-socket, stiff-spring and chain constraints are decoded
(#4212) but still declined at import by design.

### Games traced

No game collision data was traced end-to-end this pass. There was no engine launch, and the FO3 data tests were
not run because of the RAM budget. The real-data evidence comes from project records:

- `docs/engine/playable-vertical-slice.md:1383-1399`: FO3/FNV/Skyrim SE P2 runs, 2026-09-17;
- `AUDIT_RUNTIME_2026-09-11.md`: Starfield Cydonia, `rapier_bodies=95,223`.

The solver side is game-agnostic by construction (see the doctrine grep above).

### Per-dimension counts

| Dimension | CRIT | HIGH | MED | LOW | Notes |
|---|---:|---:|---:|---:|---|
| 1 — Shape Translation | 0 | 0 | 0 | 1 | + #4134 extended (3rd site) |
| 2 — Step & Sync | 0 | 0 | 1 | 2 | #3964, #3968 confirmed fixed |
| 3 — Ragdoll & Constraint Seam | 0 | 0 | 1 | 0 | doctrine holds; #4572/#4574 cited |
| 4 — Character & NPC Controller | 0 | 0 | 0 | 2 | PERF-D1-2026-09-21-06 cited |
| 5 — Water / Buoyancy | 0 | 0 | 0 | 1 | #3974, #4183 confirmed fixed (stated case) |
| 6 — Queries & Diagnostics | 0 | 0 | 2 | 0 | #3965, #3967 confirmed fixed; #3966 see D6-01 |
| Cross-dimension (skill) | 0 | 0 | 0 | 1 | |

---

## Solver Invariant Matrix

| Invariant | Verdict | Note |
|---|---|---|
| Fixed step — accumulator clamped before the loop; `frame_dt.max(0.0)` | **HOLDS** | `world.rs:597-601` |
| Anti-spiral wall-clock budget (#1698) | **HOLDS** | checked after each substep, `:739-742` |
| Static-scene fast path (`active_dynamic_bodies` empty && `!pending_wake`) | **HOLDS** | `:658-665`; kinematic set deliberately not gated |
| Wake survives a zeroed accumulator | **HOLDS** | recovery branch counts its substep before `break` (`:730`), so `pending_wake` clears only after real work |
| Collider-set mutation marks the query pipeline dirty | **FIXED** | #3968 (`bb0a5c1ff`): `build_ragdoll` calls `mark_colliders_dirty()` (`ragdoll.rs:433`) |
| Query BVH refreshed once per frame, outside the substep loop | **HOLDS as designed, design avoidable** | full rebuild every stepped frame; rapier's incremental path is ~30-90× cheaper (D6-02) |
| Explosion recovery (new) | **PARTIAL** | snapshot precedes every substep ✓, delta bound ✓, wake-safe ✓. Joints detached not restored (by design), first articulation only, pre-non-finite bodies exempt, collider sync deferred (D2-02); O(all bodies)/substep (D2-01); invisible to gates (D3-01) |
| Lock ordering (Phase 1 read → release → write) | **HOLDS** | cited `AUDIT_CONCURRENCY_2026-09-21` Dim 3/5 (0 NEW) |
| Phase order collect → push kinematic → buoyancy → step → pull | **HOLDS** | `sync.rs:130-160` |
| `physics_sync_system` Access declares every acquisition | **FIXED** | #3964 (`53bdad9c7`); 23/23 acquired types declared with matching mode |
| Scale applied exactly once, at the sink | **HOLDS** | new fallback capsule producers (`npc_spawn.rs:359-420`) author local units |
| Teardown completeness | **HOLDS** | `rapier_release` 9 green; `remove_ragdoll` removes by body (cascades joints) |
| Bone→body mapping; Z-up→Y-up applied once upstream | **HOLDS** | unchanged |
| Death reconciliation in one place (#3119) | **HOLDS** | both water-death producers queue `reconcile_pending_dead_actors_system` |
| The two water samplers agree | **DRIFTED** | #3974 fixed the no-plane-flow case; flowing plane + marker diverges (D5-01) |
| Diagnostics do not lie | **DRIFTED** | #3965/#3967 fixed; #3966's totals are process-lifetime (D6-01); recovery invisible (D3-01) |
| PHYSAL doctrine (no solver-side game branch) | **HOLDS** | seventh consecutive pass |

---

## Findings — MEDIUM

### PHYS-D2-2026-09-21-01: The explosion-recovery snapshot walks the whole body arena (fixed bodies included) on every substep
- **Severity**: MEDIUM
- **Dimension**: Step & Sync
- **Location**: `crates/physics/src/world.rs:677-690` (snapshot), `:717-721` (restore scan), `:603-635` (cost-attribution comment)
- **Status**: NEW
- **Trigger Conditions**: any cell large enough to hold tens of thousands of Rapier bodies, on any frame where a substep runs. Anything kinematic moving runs one: `push_kinematic` wakes the world (`sync.rs:1189`), and so does the player's `set_kinematic_translation` (`sync.rs:106`). Animated NPC bones are pushed every frame.
- **Description**: `613ad3124` snapshots "all dynamics" so a freshly activated ragdoll's first solve is recoverable. It does so by iterating `self.bodies.iter()`: every arena slot, fixed and kinematic bodies included. That happens once per substep, before `pipeline.step`. Fixed bodies can never need recovery, so on a big world almost all of the walk is wasted.
- **Evidence**: this is the snapshot code at `:680-690`:
  ```rust
  self.bodies.iter().filter_map(|(handle, body)| {
      (body.body_type() == RigidBodyType::Dynamic && body_state_is_finite(body)).then_some(..)
  }).collect()
  ```
  I timed it in a standalone release proxy (`/tmp/audit/physics/bench`, code copied verbatim, 40 iterations):

  | World | Snapshot per substep | `pipeline.step` |
  |---|---|---|
  | 30 k fixed + 1 awake | **0.43–0.45 ms** | 0.04 ms |
  | 30 k fixed + 300 asleep + 1 awake | 0.46–0.58 ms | 0.26–0.28 ms |
  | 95 k fixed (Cydonia `rapier_bodies=95,223`) | **1.77 ms** | 0.62 ms |
  | 1.5 k (interior) | 0.005 ms | — |

  Walking only the active set instead costs 0.2–0.7 µs at every size.
- **Impact**: on a radius-12-class exterior with one awake body, the recovery costs about 10× the solver it
  protects. With `SUBSTEP_TIME_BUDGET = PHYSICS_DT`, five catch-up substeps fit on Cydonia, which is about 9 ms of
  snapshot alone. The `step()` comment's claim that the rebuild "accounts for essentially all of it" is no longer
  true. There is no correctness impact.
- **Related**: PHYS-D6-2026-09-21-02 (the other avoidable per-frame cost); `docs/engine/playable-vertical-slice.md:1383-1391` (why the recovery exists); PERF audit today did not cover it.
- **Suggested Fix**: keep an index of dynamic-body handles, maintained on insert, remove and `set_motion_type`,
  and snapshot from that. This keeps bodies woken by contact mid-step covered, which is why the arena walk exists.
  Re-measure and keep both recovery tests.

### PHYS-D3-2026-09-21-01: Solver-explosion recovery is invisible to every ragdoll stability gate — and the FO3 P2 pass already depends on it
- **Severity**: MEDIUM
- **Dimension**: Ragdoll & Constraint Seam
- **Location**: `byroredux/src/ragdoll_installed_tests.rs:215-236`; `crates/physics/src/world.rs:717-732` (recovery, `log::error!` only), `:258-263` (multibody detach); `byroredux/src/commands/ragdoll_status.rs:41-81`; `docs/smoke-tests/p2-melee-core.sh:326-329`, `:414-436`
- **Status**: NEW
- **Trigger Conditions**: any ragdoll whose first or any later solve goes non-finite, or jumps more than 2,048 BU in one substep. Per the project doc this already happens on every FO3 restored-corpse load.
- **Description**:
  - **Data tests.** The six FO3 data tests (added with the recovery in `613ad3124`) assert after each
    `physics.step()` that every body is finite and within 512 BU of placement. `step()` now reverts any body that
    went non-finite or jumped, sleeps it and detaches its multibody before returning. So a solver explosion
    satisfies both assertions by construction.
  - **Runtime command.** `ragdoll.status` reports only
    `bodies/live/complete/finite/max_distance/max_speed`, all read after recovery. It has no joint, articulation
    or recovery field.
  - **Smoke gate.** `p2-melee-core.sh` greps `complete=true finite=true` and bounds `max_distance`. Its own comment
    says it exists because "restored bones flew to millions of units".
  - **Only signal.** The recovery reports itself through one `log::error!`. No test installs a logger and no smoke
    script scans for it. No counter exposes it (no `restored|recover` in `commands/physics.rs`, `sync.rs` or
    `ragdoll_status.rs`).
- **Evidence**: `playable-vertical-slice.md:1383-1399` records three things:
  - the FO3 copied-save restore's first solve jumps the root to about 1e12 BU;
  - the recovery "logs one recovery";
  - "the full FO3 P2 gate passed".

  A recovery calls `remove_multibody_articulations` (`world.rs:262`), so that PASS certifies a corpse with no
  joints. No issue tracks the first-solve root cause: `issues.json` has no open corpse or ragdoll
  explosion/divergence title. The retained artifacts (`/tmp/byro-p2-melee-core.kHZjTh`,
  `/tmp/byro-corpse-placement.WzTUOw`) no longer exist.
- **Impact**: a regression in the real stability fixes now ships green through the unit tests, the data tests and
  the smoke gate. Those fixes are the ×12 ragdoll iterations, seed composition and joint seeding. In game the
  regression shows as a corpse whose limbs silently come apart, which is already the FO3 restore outcome. Slow
  divergence (under 2,048 BU per substep) is still caught by the distance bounds. The fast NaN/huge-jump class,
  the Rapier multibody signature (#2337, #1534), is not.
- **Related**: #2337, #1534, #3968; PHYS-D2-2026-09-21-02.
- **Suggested Fix**:
  1. Count recoveries on `PhysicsWorld` (total and last frame) and surface the count in `phys.stats` and
     `ragdoll.status`, together with the ragdoll's live multibody/joint count.
  2. Assert zero recoveries in the installed tests and in the p2 gate. FO3 will then fail, correctly.
  3. File the FO3 restore first-solve jump as its own root-cause issue.

### PHYS-D6-2026-09-21-01: The census's "DROPPED IN TRANSLATION" verdict runs on process-lifetime registry totals, not the probed cell or column
- **Severity**: MEDIUM
- **Dimension**: Queries & Diagnostics
- **Location**:
  - `crates/physics/src/sync.rs:456-479` (`SpawnCensusAuthoring` doc: "Summed by the caller over the cell's `CachedNifImport` entries") and `:789-815` (verdict)
  - `byroredux/src/cell_loader/nif_import_registry.rs:487-509` (`collision_authoring_totals` sums every entry) and `:395-449` (process-lifetime, LRU cap 2048, "normally never cleared mid-process by design")
  - callers `byroredux/src/commands/physics.rs:139-141` and `byroredux/src/scene/character_spawn.rs:581-583`
- **Status**: NEW (the live path became reachable through the #3966 fix, `53bdad9c7`)
- **Trigger Conditions**: `phys.census`, or the boot-time all-rungs-missed census, over a column with no colliders, in any session that has loaded at least one NIF with classic, packed or phantom collision.
- **Description**: the verdict is
  `entries.is_empty() && a.classic + a.new_physics + a.phantom > 0 ⇒ "the cell's NIFs DID author collision … DROPPED IN TRANSLATION"`.
  The two sides of that test measure different things:
  - `entries` is the probed ±radius column.
  - `a` is the sum over every cached NIF from every cell visited in the process, up to 2048.

  The only other empty-column arm ("authored NO collision at all ⇒ … REFR-level gap") needs the registry-wide
  total to be zero, which no real session reaches. The inference is also wrong at boot, because it jumps from cell
  level to column level: the #2202 design text (`:636-641`) wants "the owning REFR resolves" before blaming
  translation, and the code never checks placements.
- **Evidence**: both callers pass `registry.collision_authoring_totals()`. The #3966 tests only pin whether the
  disclaimer is present or absent.
- **Impact**: the live `phys.census` is the command an operator runs after falling through a floor. It reports
  every empty column as a translation drop, including genuinely empty ones: a terrain gap, an unstreamed cell,
  the void outside an interior shell. It is the same misdirection class as #3965–#3967 (all MEDIUM), in the
  opposite direction.
- **Related**: #2874, #3966, #2202.
- **Suggested Fix**: scope the totals to the column by summing `CollisionAuthoringSummary` over placement roots
  whose bounds intersect it, or at least scope them to the current cell's placements. Relabel whatever stays
  registry-wide. Add a test with a populated registry and an empty column that must not say "DROPPED".

### PHYS-D6-2026-09-21-02: The per-frame full QBVH rebuild is avoidable — rapier's incremental path costs ~1-3 % of it on large worlds, and the rationale that rejected it misreads rapier 0.22
- **Severity**: MEDIUM
- **Dimension**: Queries & Diagnostics
- **Location**: `crates/physics/src/world.rs:702-712` (`None` passed to `pipeline.step`, with the rationale), `:792-795` (post-loop `query_pipeline.update(&self.colliders)` on every stepped frame), `:603-635` (cost attribution)
- **Status**: NEW
- **Trigger Conditions**: every frame that runs a substep, which is essentially every frame in a populated cell (see D2-01).
- **Description**: the rationale says `QueryPipeline::update` is a full `clear_and_rebuild`, "so passing it here
  rebuilt the whole tree up to 5× per frame". But rapier 0.22's `PhysicsPipeline::step` never calls `update()` on
  a pipeline it is handed. It calls `update_incremental`, which re-inserts only modified or removed leaves and
  refits/rebalances once per step. The engine therefore pays an O(all colliders) rebuild per frame to avoid a cost
  that does not exist.
- **Evidence**: in the rapier3d-0.22.0 source, `pipeline/physics_pipeline.rs:494-497` calls
  `queries.update_incremental(colliders, &modified_colliders, &removed_colliders, false)`, and `:632-640` calls
  `update_incremental(.., remaining_substeps == 0)`. `pipeline/query_pipeline/mod.rs:315-339` shows it touches
  only dirty leaves. I measured it in `/tmp/audit/physics/bench2` (1 substep/frame, 360 kinematic movers pushed
  every frame, 1 falling dynamic, 60 frames):

  | World | Engine design (`step(None)` + full `update`) | Incremental (`step(Some(qp))`) |
  |---|---|---|
  | 30 k fixed | 2.78 ms/frame | **0.089 ms/frame** |
  | 95 k fixed | 9.59 ms/frame | **0.103 ms/frame** |
  | 5 k fixed | 0.33 ms/frame | 0.087 ms/frame |

  A post-run ray finds the moving body in both designs.
- **Impact**: the skill and the code both name this the dominant per-frame physics cost. On a Cydonia-sized world
  it is about 9.6 ms of the 16.7 ms frame on the reference Ryzen 7950X. Together with D2-01 that is about 11 ms per
  frame of avoidable physics CPU on the largest cells.
- **Related**: #2864, #2890 (the history the rationale cites), PHYS-D2-2026-09-21-01.
- **Suggested Fix**: pass `Some(&mut self.query_pipeline)` to `pipeline.step` and drop the post-loop full update
  on stepped frames. Keep a full update (or a tracked-handle incremental one) only for `colliders_dirty` frames that
  run no substep, which preserves the #2864/#3968 contract. Re-measure on real content, then update the
  `:603-635` comment and its pin test `step_cost_rationale_is_scoped_to_history_and_names_the_real_cost_centre`.

---

## Findings — LOW

### PHYS-D1-2026-09-21-01: The #2543 non-finite extent clamp is pinned only by a release-only test that no CI lane runs
- **Severity**: LOW
- **Dimension**: Shape Translation
- **Location**: `crates/physics/src/convert.rs:605-616` (`#[cfg(not(debug_assertions))]` test), `:26-32` (`clamp_shape_extent`); `.github/workflows/ci.yml:169-170`, `:197-198`
- **Status**: NEW
- **Trigger Conditions**: an edit to `clamp_shape_extent`'s non-finite branch.
- **Description**: `non_finite_cuboid_extent_clamps_instead_of_reaching_rapier` is compiled out whenever debug
  assertions are on. No debug-lane test feeds NaN/±Inf through Ball, Capsule, Cylinder or Cuboid. Those arms have
  no `debug_assert`, so such a test would work there. CI runs only `cargo test --workspace` in the default profile.
  The only `--release` test jobs are the cornell oracle and the byroredux-nif data gates.
- **Evidence**: `test_physics.log` has 175 tests and this one is absent. Its Compound twin
  `non_finite_compound_child_transform_trips_canonical_boundary_assertion` ran. `grep f32::NAN|INFINITY convert.rs`
  finds only `:609` (the release test), `:641`/`:662` (Compound), `:1201` and `:1327`.
- **Impact**: suppose someone "simplifies" the function to `value.clamp(1e-3, MAX)`. That is NaN-transparent, the
  #3194/#3529 class, and CI would stay green. The audit skill also lists this test as a "default lane" guard.
- **Related**: #2543, #3194, #3529.
- **Suggested Fix**: add a debug-lane test that sends NaN/±Inf through the Ball, Capsule and Cylinder arms, or
  unit-test `clamp_shape_extent` directly. Keep the release test.

### PHYS-D2-2026-09-21-02: Three edges of the new explosion recovery are uncovered
- **Severity**: LOW
- **Dimension**: Step & Sync
- **Location**:
  - `crates/physics/src/world.rs:223-265` (finite/recovery helpers), `:680-690` (snapshot filter), `:768-787` (kill plane), `:792-795` (post-loop rebuild)
  - `crates/physics/src/ragdoll.rs:278-293` (unvalidated seed)
  - `byroredux/src/ragdoll.rs:325-330` (seed composed from the live `GlobalTransform`)
  - `crates/physics/src/sync.rs` (no `is_finite` anywhere)
- **Status**: NEW
- **Trigger Conditions**: (a) needs an upstream NaN pose. (b) happens on any recovery. (c) happens when more than one articulation, or a ragdoll plus clutter that sits earlier in the arena, is invalidated in the same substep.
- **Description**:
  - **(a) Bodies already broken before the step are never recovered.** A dynamic body that is already non-finite
    when a substep starts is filtered out of the snapshot. Its NaN `y` also fails `y < KILL_PLANE_Y`, so it is
    never slept either. It stays in the active set forever and keeps the fast path off. Seeds are never checked:
    `activate_ragdoll` composes them from bone `GlobalTransform`s, and `build_ragdoll` and `register_newcomers`
    insert them as-is.
  - **(b) Query geometry stays at the exploded pose for at least a frame.** In rapier 0.22, `set_position` defers
    collider sync to the next pipeline step (`rigid_body.rs:854-868`), and the repo never calls
    `RigidBodySet::propagate_modified_body_positions_to_colliders`. `QueryPipeline::update` builds its leaves from
    each collider's cached `co.pos` (`query_pipeline/mod.rs:339`, `:348`). So the recovery frame's post-loop
    rebuild indexes restored bodies at their exploded or NaN pose until the next substep.
  - **(c) Only one articulation is detached per substep.** `invalid_articulation_member.get_or_insert(first)`
    detaches only the multibody of the first invalid body in arena order
    (`multibody_joint_set.rs:258-278`), and does nothing if that body is plain clutter. A dynamic-root
    multibody's link poses are pure forward kinematics of its own joint coordinates (`multibody.rs:999-1019`; the
    root pose is read from the body only on a root-type change, `:874-881`). For any other articulation in the
    same substep, `set_position` therefore does not stick: its links that were not invalid stay awake and keep it
    stepping, and the next substep re-emits the corrupt pose.
- **Evidence**: code and rapier source as cited.
- **Impact**:
  - (a): a permanently awake body, with the fast path off for good.
  - (b): stale query geometry for one frame or more after a recovery.
  - (c): a second error log and one more forfeited backlog per extra articulation.

  No bad pose reaches ECS, and saved state is not corrupted.
- **Related**: PHYS-D3-2026-09-21-01, #2337.
- **Suggested Fix**: sleep, zero and log dynamic bodies that are already non-finite before the step (or reject
  non-finite seeds); call `propagate_modified_body_positions_to_colliders` after a restore; collect every
  distinct multibody instead of the first.

### PHYS-D2-2026-09-21-03: `sync.rs` module doc points the #2880 "PhysicsWorld is inserted unconditionally" claim at the wrong file
- **Severity**: LOW
- **Dimension**: Step & Sync
- **Location**: `crates/physics/src/sync.rs:26-29`
- **Status**: NEW
- **Trigger Conditions**: none (documentation).
- **Description**: `db70595e3` mechanically repointed the old `boot.rs` reference to `byroredux/src/boot/schedule/`. That directory only registers the system. The one production insertion is `byroredux/src/boot/world.rs:165`.
- **Evidence**: every other `insert_resource(PhysicsWorld::new())` hit sits in a `#[cfg(test)]` fixture.
- **Impact**: doc-rot on the claim that closed #2880: a reader following the pointer lands on the registration, not the insertion.
- **Related**: #2880, #4412 batch (`db70595e3`).
- **Suggested Fix**: point at `byroredux/src/boot/world.rs`.

### PHYS-D4-2026-09-21-01: The NPC KCC's pinned copies of ContactConfig / fallback-capsule values have no guard
- **Severity**: LOW
- **Dimension**: Character & NPC Controller
- **Location**: `byroredux/src/systems/locomotion.rs:53-77`. The sources the constants copy: `crates/physics/src/config.rs:97-98`, `byroredux/src/npc_spawn.rs:359-377`, `CharacterController::HUMAN`.
- **Status**: NEW
- **Trigger Conditions**: any retune of `ContactConfig::DEFAULT`, the fallback capsule, or the player's step/slope values.
- **Description**: each constant's doc says "matching" or "pinned as a constant … the invariant is inherited".
  Nothing checks that. The constants involved are `LOCOMOTION_NPC_CAPSULE_HALF_HEIGHT`/`RADIUS` (32/20),
  `LOCOMOTION_NPC_STEP_HEIGHT`/`STEP_MIN_WIDTH`/`MAX_SLOPE_DEG`, and `LOCOMOTION_NPC_KCC_OFFSET_BU` (4.0). The
  player reads the live `ContactConfig` resource (`character.rs:367`); NPCs read the copies.
- **Evidence**: the only uses are `locomotion.rs:164-187`. `kcc_offset_clears_the_combined_contact_skin` pins only
  the resource default. There is no runtime `ContactConfig` writer, so the values are equal today.
- **Impact**: after such a retune (#2885 already moved these values once), walking NPCs keep the old numbers with
  CI green. Their offset could drop back inside the combined contact skin: the #2193 "blocked but permanently
  ungrounded" shape.
- **Related**: #2885, #2193, M42.10 (`913fd39d8`).
- **Suggested Fix**: derive the NPC offset from `ContactConfig::DEFAULT`, share one const for the 32/20 capsule, or add an equality pin test.

### PHYS-D4-2026-09-21-02: The documented M42.10 sweep contract ("own bones masked", "shove clutter") is not what the query does
- **Severity**: LOW
- **Dimension**: Character & NPC Controller
- **Location**:
  - `crates/physics/src/world.rs:101-111` (`actor_move_interaction_groups` doc), `:862-871` (`CharacterMoveParams::filter_groups` doc), `:1306-1366` (`move_character`)
  - `byroredux/src/systems/locomotion.rs:100-118`
- **Status**: NEW
- **Trigger Conditions**: two walking NPCs whose paths cross; an FO4 shape-less actor in a walker's path; a dynamic floor item in a walker's path.
- **Description**:
  - **All actors' bones are masked, not just the walker's own.** Bone colliders get membership
    `ACTOR_BONE_GROUP` (`sync.rs:1048-1055`) and the walker's filter is `Group::ALL & !ACTOR_BONE_GROUP`
    (`world.rs:98`). A group mask cannot tell "own" bones from anyone else's, so every live actor's ~18 bones and
    every FO4 fallback capsule are invisible to every walker.
  - **Dynamic clutter acts as an immovable wall.** `move_character` passes a no-op collision callback and never
    calls `solve_character_collision_impulses`, so nothing is pushed. Autostep uses
    `include_dynamic_bodies: false`, which makes rapier's `handle_stairs` refuse a dynamic "stair"
    (`control/character_controller.rs:658-670`). The main sweep keeps dynamic bodies (`:264`), so a walker is
    blocked by clutter but can neither push it nor step over it.
- **Evidence**: code and rapier source as cited.
- **Impact**: walking NPCs pass through each other and through FO4 shape-less actors. A dynamic floor item tall
  enough to exceed the 50° climb limit on the r=20 capsule (about 7–9 BU, offset included) blocks a walker outright.
  Wander and Patrol recover through the 2.5 s re-pick; Travel, Follow, Escort and Guard have none
  (`locomotion.rs:92-98`). That behaviour is routed to `/audit-gameplay`. This is not a pre-M42.10 regression for
  NPC-vs-NPC contact, since NPCs ghosted through everything before.
- **Related**: #2873, M42.10 (`913fd39d8`).
- **Suggested Fix**: fix the three doc sites. If NPC-vs-NPC blocking is wanted, exclude only the walker's own bones
  with a `QueryFilter` predicate keyed on `ActorColliderOwner`. Decide whether walkers should apply
  character-collision impulses to dynamic bodies.

### PHYS-D5-2026-09-21-01: #3974 made the player's current "plane wins"; the dynamic path it mirrors adds both drags
- **Severity**: LOW
- **Dimension**: Water / Buoyancy
- **Location**:
  - player side: `byroredux/src/systems/character.rs:1093-1120` (`plane_flow.or_else(marker_flow)`), `:286-299` (one drift term)
  - dynamic side: `crates/physics/src/water.rs:843-858` (marker lookup), `:971-980` (plane drag × submerged fraction), `:1040-1081` (marker drag × 1.0, applied after the plane), `:1008` (`WaterContact.flow` = plane flow only)
- **Status**: NEW (partial close of #3974, not a regression)
- **Trigger Conditions**: a flowing `WaterPlane` overlapped by an XWCU `WaterCurrentVolume`, as in Skyrim rapids. Secondary case: a marker that overlaps no plane.
- **Description**: the dynamic path applies the plane's drag and then the marker's drag, and the forces add. Its
  comment says why: "so a co-located water plane's force does not discard the marker's current". The player
  sampler consults the marker only when the plane has no `WaterFlow`. The #3974 commit cites "the dynamic path's
  `current_flow.or(plane flow)` resolution", which does not exist.
- **Evidence**: the production code of `water.rs` has no `.or(` flow resolution. Co-location occurs in real content:
  river WATR planes get a `WaterFlow` (`env_translate.rs:868-884`, `cell_loader/water.rs:699-701`), and XWCU
  markers are separate synthesized volumes (`cell_loader/references/synth_child.rs:36-49`).
- **Impact**: in rapids, a barrel feels plane drag plus marker drag while the swimmer beside it feels only the
  plane. `water.contacts` shows the same flow for both, which hides the difference. This is a gameplay parity issue
  only.
- **Related**: #3974, #3114, #3268.
- **Suggested Fix**: choose one composition for both samplers. Extend the #3974 test with a flowing-plane + marker case that asserts the two samplers agree.

### PHYS-META-2026-09-21-01: `audit-physics/SKILL.md` drift found while running it
- **Severity**: LOW
- **Dimension**: Queries & Diagnostics (audit infrastructure; cross-dimension)
- **Location**: `.claude/commands/audit-physics/SKILL.md` — the known-open register, Dim 1 guards, the Dim 2 recovery bullet, Dim 3 last bullet, Dim 4 NPC and player bullets, the Dim 5 samplers bullet, and the Dim 6 cost bullet
- **Status**: NEW
- **Trigger Conditions**: the next `/audit-physics` run.
- **Description**: eight statements in the skill are stale:
  1. #4407 is listed as open, but `726a2e930` closed it.
  2. `non_finite_cuboid_extent_clamps_instead_of_reaching_rapier` is listed as a default-lane guard. It is release-only (D1-01).
  3. "Multibody joints are restored with their bodies": by design the recovery detaches them (`world.rs:258-263`, `playable-vertical-slice.md:1386-1390`).
  4. "Writeback drives `Transform`": it writes `GlobalTransform` (`byroredux/src/ragdoll.rs:590-604`).
  5. `snap_character_body_to_camera` "takes `&mut World`": it takes `&World` by documented design (`character.rs:758-763`).
  6. "NPCs shove clutter": nothing pushes clutter (D4-02).
  7. "Plane flow wins over the marker in both": the two samplers disagree (D5-01).
  8. "Budget from the rebuild": the rebuild is avoidable (D6-02), and the snapshot walk adds a per-substep cost (D2-01).
- **Evidence**: each item is verified at `f97775ca8` at the cited lines.
- **Impact**: the next run would start from wrong premises on three live mechanisms (recovery, NPC sweep, sampler parity).
- **Related**: ECS-2026-09-21-D2-01 (same class).
- **Suggested Fix**: update the skill and run `.claude/commands/_audit-validate.sh`.

---

## Existing (matched to an open issue; cited, not re-counted)

### PHYS-D1-2026-09-11-02 (#4134): Character-controller/ground-probe capsule shapes use a floor-only `.max(1e-3)` — now three sites
- **Severity**: LOW
- **Dimension**: Shape Translation
- **Location**: `crates/physics/src/world.rs:1105` (`cast_capsule_down_surface_and_normal`), `:1146` (`capsule_overlaps_solid`, **new since the issue**), `:1325-1328` (`move_character`)
- **Status**: Existing: #4134
- **Trigger Conditions**: a data-derived (e.g. CHARAL per-race) capsule dimension that is `INFINITY` or huge.
- **Description**: `c03d6d6fe` (2026-09-16) added `capsule_overlaps_solid` with the same `SharedShape::capsule_y(half_height.max(1e-3), radius.max(1e-3))`. #4134 (filed 2026-09-11) names only the other two sites.
- **Evidence**: `grep -n 'capsule_y\|\.max(1e-3)' crates/physics/src/world.rs`.
- **Impact**: unchanged. The inputs are engine constants (spawn ladder / door clearance) today.
- **Related**: #4134; PERF-D1-2026-09-21-06, the same capsule constructor, now allocated per walker per tick.
- **Suggested Fix**: add the third site to #4134's checklist; route all three through `clamp_shape_extent`.

---

## Confirmed-Fixed Register (re-derived from code this pass)

| Issue | Fixed by | Re-derivation |
|---|---|---|
| #3964 (Ragdoll read undeclared) | `53bdad9c7` | `.reads::<Ragdoll>()` present; all 23 acquired types of `physics_sync_system` (`sync.rs` + `water.rs`) declared, modes match |
| #3965 (census self-hit) | `53bdad9c7` | `excluded_body` threaded (`sync.rs:682-688`); `phys.census` resolves the player body; guard green |
| #3966 (census authoring disabled) | `53bdad9c7` | wired, but see PHYS-D6-2026-09-21-01 for scope |
| #3967 (census Y-only) | `53bdad9c7` | full AABB + scale carried and printed; guard green |
| #3968 (`build_ragdoll` not marking dirty) | `bb0a5c1ff` | `mark_colliders_dirty()` at `ragdoll.rs:433`; zero-substep ray test green |
| #3974 (player current arm) | `588ed7de2` | stated case fixed and pinned; co-located case see PHYS-D5-2026-09-21-01 |
| #4183 (submersion wave sample under guards) | `4ec5776a5` | hoisted above the water guards (`systems/water.rs:149-164`) |
| #4407 (BhkPlaneShape justification) | `726a2e930` | comment corrected; `plane_shapes` folded into `SpawnCensusAuthoring` and printed |
| #4212 (constraint stubs) | `5172f5e26` | three CInfos decoded; import still declines them (by design); `physal.md` §3 current |

## Known-Open Register (what this pass changed)

- **#4134** OPEN: extended with a third site (above).
- **#3477** OPEN: still true; `collect_newcomers` iterates every `CollisionShape` row per tick (`sync.rs:930`). Perf-owned; cited.
- **#4408** OPEN (NIFAL): not re-verified; cited. **#4562** OPEN (NIFAL): confirmed as the only `packed_havok` identifier left (`convert.rs:239`).
- *tes_grounding_zero_mass_dynamic_fix*: the mass=0 reclassification is unchanged (`collision/mod.rs:424-432`). `c03d6d6fe` adds a capsule-clearance rung to the door-spawn ladder; otherwise the door-threshold content question is unchanged.
- *interior_spawn_point_fix*: untouched.
- Ball-and-socket / stiff-spring / chain: decoded but declined by design; no occupancy census run (RAM), so nothing filed.
- WATAL open items (water-walking, freezing) remain unbuilt per `docs/engine/watal.md`; not filed.
- **New untracked root cause** surfaced by D3-01: the FO3 restored corpse's first solve jumps to about 1e12 BU and is contained, not fixed, by the recovery.

## Cross-Audit Deduplication

| Topic | Owner | Note |
|---|---|---|
| Per-frame cycle-guard-less subtree walks in `ragdoll_writeback_system` | `/audit-ecs` | ECS-2026-09-21-D3-01 (#4572); cited |
| `ragdoll_writeback_system` registered without access declaration | `/audit-ecs` | ECS-2026-09-21-D5-02 (#4574); cited |
| `move_character` allocates a capsule `Arc` per walker per tick | `/audit-performance` | PERF-D1-2026-09-21-06; cited (same constructor as #4134) |
| FO4 rigid-body / constraint-ref over-allocation in `rigid_body.rs` | `/audit-nif` | NIF-D6-2026-09-21-01; parse side, not re-reviewed |
| NPC stuck behaviour on unsteppable dynamic clutter (Travel/Follow/Escort/Guard have no re-pick) | `/audit-gameplay` | consequence of PHYS-D4-2026-09-21-02 |
| `save_io/registry_completeness_tests.rs:385` says `Ragdoll` has "no live inserter … (debug console command only)"; `combat.rs:563` is now a live caller (exclusion still correct, rationale stale) | `/audit-save` | pointer only |
| Phantom-bounds → `WaterVolume` translation (`material_translate.rs:402`) | `/audit-exterior` | the extractor in `collision/mod.rs` was checked here (Dim 1, clean) |
| Lock order across the new physics paths | `/audit-concurrency` | today's leg: 0 NEW (Dim 3/5) |

## Verification notes

- Each finding was re-read at HEAD and a disproof was attempted:
  - D1-01: the CI workflows were re-grepped for `--release` and debug-assertion overrides.
  - D3-01: the smoke scripts were grepped for any log scan.
  - D5-01: `water.rs` was grepped for any `.or(` precedence.
  - D6-02: the rapier source was read and both designs were measured with a query-correctness check.
- Rapier semantics cited come from the vendored `rapier3d-0.22.0` source, with the paths given in each finding.
- Dedup used `/tmp/audit/issues.json`, refreshed during the run (it now reaches #4581). Titles were searched for
  census, authoring, snapshot, recovery, multibody, KCC, locomotion, release-only and QBVH. The sibling reports
  checked were `AUDIT_{RENDERER,SAFETY,ECS,CONCURRENCY,PERFORMANCE,NIF,TECH_DEBT}_2026-09-21.md`,
  `AUDIT_NIFAL_2026-09-21{,b}.md` and `AUDIT_EXTERIOR_2026-09-19.md`.

Next step: `/audit-publish docs/audits/AUDIT_PHYSICS_2026-09-21.md`. Suggested labels: `physics` on all findings;
`water` on D5-01; `performance` on D2-01 and D6-02; `test-gap` on D1-01 and D3-01; `doc-rot` on D2-03, D4-02 and
META-01; `tech-debt` on META-01 (audit infrastructure); `game:fo3` optional on D3-01.
