=== #4680 ===
# null: CHAR-2026-09-21-D1-03: body_condition_base = 100 has no row in the CHARAL capture [OPEN]

**Severity**: LOW
**Dimension**: Ruleset Seam
**Game**: FO3 / FNV

## Description

The #4447 fix correctly moved the base-100 constant onto the profile. It left the capture document silent, even though that document is the stated authority for every CHARAL constant. A profile-row audit can verify every other FO3/FNV row against `charal-fnv-fo3-ruleset.md`, but not this one.

## Evidence

Verified at HEAD `ee6d3fb39`: `crates/core/src/character/profile.rs`'s `body_condition_base: Option<f32>` field doc cites "GECK Stats List" ("FO3/FNV seed the seven GECK limb-condition AVs at 100 (\"GECK Stats List\" — the AV names live in `consumables::BODY_CONDITION_VALUES`)"), and `FALLOUT3`/`FALLOUT_NEW_VEGAS` both set `body_condition_base: Some(100.0)`. `docs/engine/charal-fnv-fo3-ruleset.md` has no occurrence of "body condition" or of the seven AV names; the only citation for the constant is `docs/engine/playable-vertical-slice.md` (GECK Stats List, referenced from the vertical-slice log, not the capture).

## Impact

A future edit to the constant has no capture line to be checked against, which is the exact gap the no-guessing doctrine guards.

## Related

#4447, #4453 (the unsourced-profile-row class).

## Suggested Fix

Add a "Body-condition AVs (7) — base 100, GECK *Stats List*" row to the FNV/FO3 capture, naming `consumables::BODY_CONDITION_VALUES`.

Source: docs/audits/AUDIT_CHARACTER_2026-09-21.md (CHAR-2026-09-21-D1-03)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)

=== #4681 ===
# null: CHAR-2026-09-21-D2-01: fallout.rs module docstring still states Crit/Melee/Unarmed are actor-general as fact — #4450 fix only edited the function doc [OPEN]

**Severity**: LOW
**Dimension**: Derived Formulas
**Game**: FO3 / FNV

## Description

The module doc says "Carry Weight / Melee Damage / Critical Chance / Unarmed Damage are actor-general. That justification is sourced for Health ... but **not** for FO3/FNV Action Points". It presents the three unsourced scopes as settled, and lists AP as the only unsourced exception. This is the overstatement #4450 was filed for. The function doc and the capture were fixed; the module summary a reader meets first was not.

## Evidence

Verified at HEAD `ee6d3fb39`. `crates/core/src/character/fallout.rs` module doc: "Health / Action Points are flagged `player_only` ... Carry Weight / Melee Damage / Critical Chance / Unarmed Damage are actor-general. That justification is sourced for Health (every game) and for FO4/FO76 Action Points ... but **not** for FO3/FNV Action Points, which is `player_only` as a conservative, unsourced choice (#2937)."

This contradicts the function doc on `add_fnv_fo3_shared` in the same file: "Critical Chance, Melee Damage and Unarmed Damage ship `ActorGeneral` as an explicit but UNsourced choice — no capture line states their scope (#4450), the mirror of #2937's documented conservative `player_only` for Action Points."

## Impact

Doc only; misleads a reader about which scopes are sourced.

## Related

#4450 (CLOSED, fixed the function doc and the capture), #2937.

## Suggested Fix

Add to the module doc: "Critical Chance / Melee Damage / Unarmed Damage are an explicit, unsourced `ActorGeneral` choice (#4450, pinned by `fo3_fnv_crit_melee_unarmed_scopes_are_actor_general_pending_a_source`)".

Source: docs/audits/AUDIT_CHARACTER_2026-09-21.md (CHAR-2026-09-21-D2-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)

=== #4682 ===
# null: PHYS-D2-2026-09-21-01: The explosion-recovery snapshot walks the whole body arena (fixed bodies included) on every substep [OPEN]

### PHYS-D2-2026-09-21-01: The explosion-recovery snapshot walks the whole body arena (fixed bodies included) on every substep

- **Severity**: MEDIUM
- **Dimension**: Step & Sync
- **Location**: `crates/physics/src/world.rs:680-690` (snapshot), `:717-721` (restore scan), `:603-635` (cost-attribution comment)
- **Status**: NEW

**Description**: `613ad3124` added a per-substep snapshot of "all dynamics" so a freshly activated ragdoll's first solve is recoverable. It does so by iterating `self.bodies.iter()`: every arena slot, fixed and kinematic bodies included. That happens once per substep, before `pipeline.step`. Fixed bodies can never need recovery, so on a big world almost all of the walk is wasted.

**Evidence**: the snapshot code at `world.rs:680-690`:
```rust
let snapshots: Vec<_> =
    self.bodies
        .iter()
        .filter_map(|(handle, body)| {
            (body.body_type() == RigidBodyType::Dynamic && body_state_is_finite(body))
                .then_some(DynamicBodySnapshot {
                    handle,
                    position: *body.position(),
                })
        })
        .collect();
```
Timed in a standalone release proxy (code copied verbatim from `world.rs`, 40 iterations):

| World | Snapshot per substep | `pipeline.step` |
|---|---|---|
| 30 k fixed + 1 awake | 0.43–0.45 ms | 0.04 ms |
| 30 k fixed + 300 asleep + 1 awake | 0.46–0.58 ms | 0.26–0.28 ms |
| 95 k fixed (Cydonia `rapier_bodies=95,223`) | 1.77 ms | 0.62 ms |
| 1.5 k (interior) | 0.005 ms | — |

Walking only the active set instead costs 0.2–0.7 µs at every size.

**Impact**: on a radius-12-class exterior with one awake body, the recovery costs about 10× the solver it protects. With `SUBSTEP_TIME_BUDGET = PHYSICS_DT`, five catch-up substeps fit on Cydonia, which is about 9 ms of snapshot alone. The `step()` comment's claim that the rebuild "accounts for essentially all of it" is no longer true. There is no correctness impact.

**Related**: PHYS-D6-2026-09-21-02 (the other avoidable per-frame cost, filed alongside this one); `docs/engine/playable-vertical-slice.md:1383-1391` (why the recovery exists).

**Suggested Fix**: keep an index of dynamic-body handles, maintained on insert, remove and `set_motion_type`, and snapshot from that. This keeps bodies woken by contact mid-step covered, which is why the arena walk exists. Re-measure and keep both recovery tests.

Source: docs/audits/AUDIT_PHYSICS_2026-09-21.md (PHYS-D2-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: Check the restore scan (`:717-721`) after the fix — it iterates the same snapshot list, so shrinking the snapshot set shrinks the restore cost too
- [ ] **TESTS**: A regression/benchmark test pins the snapshot cost to the active-dynamic-body count, not the full arena size

=== #4683 ===
# null: PHYS-D3-2026-09-21-01: Solver-explosion recovery is invisible to every ragdoll stability gate — and the FO3 P2 pass already depends on it [OPEN]

### PHYS-D3-2026-09-21-01: Solver-explosion recovery is invisible to every ragdoll stability gate — and the FO3 P2 pass already depends on it

- **Severity**: MEDIUM
- **Dimension**: Ragdoll & Constraint Seam
- **Location**: `byroredux/src/ragdoll_installed_tests.rs:215-236`; `crates/physics/src/world.rs:717-732` (recovery, `log::error!` only), `:258-263` (multibody detach); `byroredux/src/commands/ragdoll_status.rs:41-81`; `docs/smoke-tests/p2-melee-core.sh:326-329`, `:414-436`
- **Status**: NEW

**Description**:
- **Data tests.** The six FO3 data tests (added with the recovery in `613ad3124`) assert after each `physics.step()` that every body is finite and within 512 BU of placement. `step()` now reverts any body that went non-finite or jumped, sleeps it and detaches its multibody before returning. So a solver explosion satisfies both assertions by construction.
- **Runtime command.** `ragdoll.status` reports only `bodies/live/complete/finite/max_distance/max_speed`, all read after recovery. It has no joint, articulation or recovery field — confirmed at `ragdoll_status.rs:41-81`.
- **Smoke gate.** `p2-melee-core.sh` greps `complete=true finite=true` and bounds `max_distance`. Its own comment says it exists because "restored bones flew to millions of units".
- **Only signal.** The recovery reports itself through one `log::error!` (`world.rs:722-726`). No test installs a logger and no smoke script scans for it. No counter exposes it (no `restored|recover` field in `commands/physics.rs`, `sync.rs` or `ragdoll_status.rs`).

**Evidence**: `docs/engine/playable-vertical-slice.md:1383-1399` records three things: the FO3 copied-save restore's first solve jumps the root to about 1e12 BU; the recovery "logs one recovery"; "the full FO3 P2 gate passed". A recovery calls `remove_multibody_articulations` (`world.rs:262`), so that PASS certifies a corpse with no joints. No issue tracks the first-solve root cause: no open corpse or ragdoll explosion/divergence title exists in the repo's issue list.

**Impact**: a regression in the real stability fixes now ships green through the unit tests, the data tests and the smoke gate. Those fixes are the ×12 ragdoll iterations, seed composition and joint seeding. In game the regression shows as a corpse whose limbs silently come apart, which is already the FO3 restore outcome. Slow divergence (under 2,048 BU per substep) is still caught by the distance bounds. The fast NaN/huge-jump class, the Rapier multibody signature (#2337, #1534), is not.

**Related**: #2337, #1534, #3968; PHYS-D2-2026-09-21-02 (filed alongside this one, uncovered edges of the same recovery mechanism).

**Suggested Fix**:
1. Count recoveries on `PhysicsWorld` (total and last frame) and surface the count in `phys.stats` and `ragdoll.status`, together with the ragdoll's live multibody/joint count.
2. Assert zero recoveries in the installed tests and in the p2 gate. FO3 will then fail, correctly.
3. File the FO3 restore first-solve jump as its own root-cause issue.

Source: docs/audits/AUDIT_PHYSICS_2026-09-21.md (PHYS-D3-2026-09-21-01)

## Completeness Checks
- [ ] **TESTS**: A regression test asserts zero recoveries on the FO3 installed-test corpus once the root cause is fixed, or a recovery counter is surfaced and asserted on in the meantime
- [ ] **SIBLING**: Check `phys.stats`/other physics diagnostic commands for the same "reads only post-recovery state" blind spot

=== #4684 ===
# null: PHYS-D6-2026-09-21-01: The census's "DROPPED IN TRANSLATION" verdict runs on process-lifetime registry totals, not the probed cell or column [OPEN]

### PHYS-D6-2026-09-21-01: The census's "DROPPED IN TRANSLATION" verdict runs on process-lifetime registry totals, not the probed cell or column

- **Severity**: MEDIUM
- **Dimension**: Queries & Diagnostics
- **Location**:
  - `crates/physics/src/sync.rs:456-479` (`SpawnCensusAuthoring` doc: "Summed by the caller over the cell's `CachedNifImport` entries") and `:789-815` (verdict)
  - `byroredux/src/cell_loader/nif_import_registry.rs:487-509` (`collision_authoring_totals` sums every entry) and `:395-449` (process-lifetime, LRU cap 2048, "normally never cleared mid-process by design")
  - callers `byroredux/src/commands/physics.rs:139-141` and `byroredux/src/scene/character_spawn.rs:581-583`
- **Status**: NEW (the live path became reachable through the #3966 fix, `53bdad9c7`)

**Description**: the verdict is
`entries.is_empty() && a.classic + a.new_physics + a.phantom > 0 ⇒ "the cell's NIFs DID author collision … DROPPED IN TRANSLATION"`.
The two sides of that test measure different things:
- `entries` is the probed ±radius column.
- `a` is the sum over every cached NIF from every cell visited in the process, up to 2048.

The only other empty-column arm ("authored NO collision at all ⇒ … REFR-level gap") needs the registry-wide total to be zero, which no real session reaches. The inference is also wrong at boot, because it jumps from cell level to column level: the #2202 design text wants "the owning REFR resolves" before blaming translation, and the code never checks placements.

**Evidence**: both callers pass `registry.collision_authoring_totals()`, confirmed re-read at HEAD — `collision_authoring_totals` (`nif_import_registry.rs:487-509`) iterates `self.core.keys()` (the whole process-lifetime cache) with `saturating_add`, no per-cell or per-column scoping. The verdict match arm at `sync.rs:789-796` is unchanged from the report's citation.

**Impact**: the live `phys.census` is the command an operator runs after falling through a floor. It reports every empty column as a translation drop, including genuinely empty ones: a terrain gap, an unstreamed cell, the void outside an interior shell. It is the same misdirection class as #3965–#3967 (all MEDIUM, all fixed), in the opposite direction.

**Related**: #2874, #3966, #2202.

**Suggested Fix**: scope the totals to the column by summing `CollisionAuthoringSummary` over placement roots whose bounds intersect it, or at least scope them to the current cell's placements. Relabel whatever stays registry-wide. Add a test with a populated registry and an empty column that must not say "DROPPED".

Source: docs/audits/AUDIT_PHYSICS_2026-09-21.md (PHYS-D6-2026-09-21-01)

## Completeness Checks
- [ ] **TESTS**: A test with a populated registry (nonzero process-lifetime totals from an unrelated cell) and a genuinely empty probed column must not print "DROPPED IN TRANSLATION"
- [ ] **SIBLING**: #3965's `excluded_body` fix and #3967's full-AABB fix both scoped their data correctly — check this scoping fix doesn't reintroduce either regression

=== #4685 ===
# null: PHYS-D6-2026-09-21-02: The per-frame full QBVH rebuild is avoidable — rapier's incremental path costs ~1-3% of it on large worlds [OPEN]

### PHYS-D6-2026-09-21-02: The per-frame full QBVH rebuild is avoidable — rapier's incremental path costs ~1-3% of it on large worlds, and the rationale that rejected it misreads rapier 0.22

- **Severity**: MEDIUM
- **Dimension**: Queries & Diagnostics
- **Location**: `crates/physics/src/world.rs:702-712` (`None` passed to `pipeline.step`, with the rationale), `:792-795` (post-loop `query_pipeline.update(&self.colliders)` on every stepped frame), `:603-635` (cost attribution)
- **Status**: NEW

**Description**: the rationale says `QueryPipeline::update` is a full `clear_and_rebuild`, "so passing it here rebuilt the whole tree up to 5× per frame". But rapier 0.22's `PhysicsPipeline::step` never calls `update()` on a pipeline it is handed. It calls `update_incremental`, which re-inserts only modified or removed leaves and refits/rebalances once per step. The engine therefore pays an O(all colliders) rebuild per frame to avoid a cost that does not exist.

**Evidence**: in the rapier3d-0.22.0 source, `pipeline/physics_pipeline.rs:494-497` calls `queries.update_incremental(colliders, &modified_colliders, &removed_colliders, false)`, and `:632-640` calls `update_incremental(.., remaining_substeps == 0)`. `pipeline/query_pipeline/mod.rs:315-339` shows it touches only dirty leaves. Re-read at HEAD: `world.rs:712` still passes `None` for the query pipeline argument, and `world.rs:792-795` still runs the full `query_pipeline.update(&self.colliders)` unconditionally after the substep loop when `steps > 0 || self.colliders_dirty`. Measured in a standalone release proxy (1 substep/frame, 360 kinematic movers pushed every frame, 1 falling dynamic, 60 frames):

| World | Engine design (`step(None)` + full `update`) | Incremental (`step(Some(qp))`) |
|---|---|---|
| 30 k fixed | 2.78 ms/frame | 0.089 ms/frame |
| 95 k fixed | 9.59 ms/frame | 0.103 ms/frame |
| 5 k fixed | 0.33 ms/frame | 0.087 ms/frame |

A post-run ray finds the moving body in both designs.

**Impact**: on a Cydonia-sized world it is about 9.6 ms of the 16.7 ms frame on the reference Ryzen 7950X. Together with PHYS-D2-2026-09-21-01 that is about 11 ms per frame of avoidable physics CPU on the largest cells.

**Related**: #2864, #2890 (the history the rationale cites), PHYS-D2-2026-09-21-01 (filed alongside this one).

**Suggested Fix**: pass `Some(&mut self.query_pipeline)` to `pipeline.step` and drop the post-loop full update on stepped frames. Keep a full update (or a tracked-handle incremental one) only for `colliders_dirty` frames that run no substep, which preserves the #2864/#3968 contract. Re-measure on real content, then update the `:603-635` comment and its pin test `step_cost_rationale_is_scoped_to_history_and_names_the_real_cost_centre`.

Source: docs/audits/AUDIT_PHYSICS_2026-09-21.md (PHYS-D6-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: `update_query_pipeline` explicit call sites (e.g. the spawn ground-snap) are unaffected by this change — verify they still see current data
- [ ] **TESTS**: A regression test pins that a moving dynamic body is still found by a post-step ray/shape query with the incremental path enabled

=== #4686 ===
# null: PHYS-D1-2026-09-21-01: The #2543 non-finite extent clamp is pinned only by a release-only test that no CI lane runs [OPEN]

### PHYS-D1-2026-09-21-01: The #2543 non-finite extent clamp is pinned only by a release-only test that no CI lane runs

- **Severity**: LOW
- **Dimension**: Shape Translation
- **Location**: `crates/physics/src/convert.rs:605-616` (`#[cfg(not(debug_assertions))]` test), `:26-32` (`clamp_shape_extent`); `.github/workflows/ci.yml:169-170`, `:197-198`
- **Status**: NEW

**Description**: `non_finite_cuboid_extent_clamps_instead_of_reaching_rapier` is compiled out whenever debug assertions are on. No debug-lane test feeds NaN/±Inf through Ball, Capsule, Cylinder or Cuboid. Those arms have no `debug_assert`, so such a test would work there. CI runs only `cargo test --workspace` in the default profile. The only `--release` test jobs are the cornell oracle and the byroredux-nif data gates.

**Evidence**: re-read at HEAD — `convert.rs:604-616` still gates the test behind `#[cfg(not(debug_assertions))]`, and `clamp_shape_extent` (`convert.rs:23-31`) is the only non-finite backstop:
```rust
fn clamp_shape_extent(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(1e-3, MAX_SANE_SHAPE_EXTENT)
    } else {
        1e-3
    }
}
```
`grep f32::NAN|INFINITY convert.rs` finds only the release test and the Compound twin `non_finite_compound_child_transform_trips_canonical_boundary_assertion`, which does run in the default profile.

**Impact**: suppose someone "simplifies" the function to `value.clamp(1e-3, MAX)`. That is NaN-transparent (`f32::clamp` on NaN is unspecified per IEEE 754 semantics as implemented by Rust — it does not reliably clamp), the #3194/#3529 class, and CI would stay green. The audit skill also lists this test as a "default lane" guard, which is itself wrong (see PHYS-META-2026-09-21-01).

**Related**: #2543, #3194, #3529.

**Suggested Fix**: add a debug-lane test that sends NaN/±Inf through the Ball, Capsule and Cylinder arms, or unit-test `clamp_shape_extent` directly. Keep the release test.

Source: docs/audits/AUDIT_PHYSICS_2026-09-21.md (PHYS-D1-2026-09-21-01)

## Completeness Checks
- [ ] **TESTS**: A debug-lane (default profile) test exercises the non-finite clamp path, so CI catches a regression without needing a release build

=== #4687 ===
# null: PHYS-D2-2026-09-21-02: Three edges of the new explosion recovery are uncovered [OPEN]

### PHYS-D2-2026-09-21-02: Three edges of the new explosion recovery are uncovered

- **Severity**: LOW
- **Dimension**: Step & Sync
- **Location**:
  - `crates/physics/src/world.rs:223-265` (finite/recovery helpers), `:680-690` (snapshot filter), `:768-787` (kill plane), `:792-795` (post-loop rebuild)
  - `crates/physics/src/ragdoll.rs:278-293` (unvalidated seed)
  - `byroredux/src/ragdoll.rs:325-330` (seed composed from the live `GlobalTransform`)
  - `crates/physics/src/sync.rs` (no `is_finite` anywhere)
- **Status**: NEW

**Description**:
- **(a) Bodies already broken before the step are never recovered.** A dynamic body that is already non-finite when a substep starts is filtered out of the snapshot (`body_state_is_finite` guard in the `filter_map`). Its NaN `y` also fails `y < KILL_PLANE_Y`, so it is never slept either. It stays in the active set forever and keeps the fast path off. Seeds are never checked: `activate_ragdoll` composes them from bone `GlobalTransform`s, and `build_ragdoll` and `register_newcomers` insert them as-is.
- **(b) Query geometry stays at the exploded pose for at least a frame.** In rapier 0.22, `set_position` defers collider sync to the next pipeline step (`rigid_body.rs:854-868`), and the repo never calls `RigidBodySet::propagate_modified_body_positions_to_colliders` (confirmed: no such call in `crates/physics/src/sync.rs`). `QueryPipeline::update` builds its leaves from each collider's cached `co.pos` (`query_pipeline/mod.rs:339`, `:348`). So the recovery frame's post-loop rebuild indexes restored bodies at their exploded or NaN pose until the next substep.
- **(c) Only one articulation is detached per substep.** `invalid_articulation_member.get_or_insert(first)` (`world.rs:255`) detaches only the multibody of the first invalid body in arena order (`multibody_joint_set.rs:258-278`), and does nothing if that body is plain clutter. A dynamic-root multibody's link poses are pure forward kinematics of its own joint coordinates (`multibody.rs:999-1019`; the root pose is read from the body only on a root-type change, `:874-881`). For any other articulation in the same substep, `set_position` therefore does not stick: its links that were not invalid stay awake and keep it stepping, and the next substep re-emits the corrupt pose.

**Evidence**: code and rapier source as cited; re-read at HEAD confirms `restore_invalid_dynamic_bodies` (`world.rs:236-265`) still takes only the first `invalid_articulation_member` and never checks incoming-seed finiteness.

**Impact**:
- (a): a permanently awake body, with the fast path off for good.
- (b): stale query geometry for one frame or more after a recovery.
- (c): a second error log and one more forfeited backlog per extra articulation.

No bad pose reaches ECS, and saved state is not corrupted.

**Related**: PHYS-D3-2026-09-21-01 (the recovery this extends), #2337.

**Suggested Fix**: sleep, zero and log dynamic bodies that are already non-finite before the step (or reject non-finite seeds); call `propagate_modified_body_positions_to_colliders` after a restore; collect every distinct multibody instead of the first.

Source: docs/audits/AUDIT_PHYSICS_2026-09-21.md (PHYS-D2-2026-09-21-02)

## Completeness Checks
- [ ] **TESTS**: Regression tests for each of the three edges: (a) a pre-broken seed is slept/rejected rather than staying permanently awake, (b) a post-recovery query reflects the restored pose within the same frame, (c) two simultaneously-invalidated articulations are both detached
- [ ] **SIBLING**: Check `register_newcomers` and `activate_ragdoll` for the same unvalidated-seed pattern beyond the two cited call sites

=== #4688 ===
# null: PHYS-D2-2026-09-21-03: sync.rs module doc points the #2880 "PhysicsWorld is inserted unconditionally" claim at the wrong file [OPEN]

### PHYS-D2-2026-09-21-03: `sync.rs` module doc points the #2880 "PhysicsWorld is inserted unconditionally" claim at the wrong file

- **Severity**: LOW
- **Dimension**: Step & Sync
- **Location**: `crates/physics/src/sync.rs:26-29`
- **Status**: NEW

**Description**: `db70595e3` mechanically repointed the old `boot.rs` reference to `byroredux/src/boot/schedule/`. That directory only registers the system. The one production insertion is `byroredux/src/boot/world.rs:165`.

**Evidence**: re-confirmed at HEAD — `sync.rs:26-29` still reads:
```
//! covers test fixtures and embedders that omit it — **not** a loose-NIF
//! viewer opt-out: the shipping binary inserts `PhysicsWorld`
//! unconditionally (`byroredux/src/boot/schedule/`), so every path including
```
`grep -n "PhysicsWorld::new()\|insert_resource(PhysicsWorld" byroredux/src/boot/` finds exactly one production hit: `byroredux/src/boot/world.rs:165: world.insert_resource(byroredux_physics::PhysicsWorld::new());`. Every other hit is a `#[cfg(test)]` fixture.

**Impact**: doc-rot on the claim that closed #2880: a reader following the pointer lands on the registration, not the insertion.

**Related**: #2880, #4412 batch (`db70595e3`).

**Suggested Fix**: point at `byroredux/src/boot/world.rs`.

Source: docs/audits/AUDIT_PHYSICS_2026-09-21.md (PHYS-D2-2026-09-21-03)

## Completeness Checks
- [ ] **SIBLING**: Check other `db70595e3`-batch doc pointers in the same crate for the same registration-vs-insertion mixup

=== #4689 ===
# null: PHYS-D4-2026-09-21-01: The NPC KCC's pinned copies of ContactConfig / fallback-capsule values have no guard [OPEN]

### PHYS-D4-2026-09-21-01: The NPC KCC's pinned copies of ContactConfig / fallback-capsule values have no guard

- **Severity**: LOW
- **Dimension**: Character & NPC Controller
- **Location**: `byroredux/src/systems/locomotion.rs:53-77`. The sources the constants copy: `crates/physics/src/config.rs:97-98`, `byroredux/src/npc_spawn.rs:359-377`, `CharacterController::HUMAN`.
- **Status**: NEW

**Description**: each constant's doc says "matching" or "pinned as a constant … the invariant is inherited". Nothing checks that. The constants involved are `LOCOMOTION_NPC_CAPSULE_HALF_HEIGHT`/`RADIUS` (32/20), `LOCOMOTION_NPC_STEP_HEIGHT`/`STEP_MIN_WIDTH`/`MAX_SLOPE_DEG`, and `LOCOMOTION_NPC_KCC_OFFSET_BU` (4.0). The player reads the live `ContactConfig` resource (`character.rs:367`); NPCs read the copies.

**Evidence**: re-read at HEAD — `locomotion.rs:53-77` still declares all six constants as bare `pub(crate) const` values with only doc-comment cross-references, no `static_assertions`/equality test tying them to `ContactConfig::DEFAULT` or `CharacterController::HUMAN`. The only uses are `locomotion.rs:164-187`. `kcc_offset_clears_the_combined_contact_skin` pins only the resource default. There is no runtime `ContactConfig` writer, so the values are equal today.

**Impact**: after such a retune (#2885 already moved these values once), walking NPCs keep the old numbers with CI green. Their offset could drop back inside the combined contact skin: the #2193 "blocked but permanently ungrounded" shape.

**Related**: #2885, #2193, M42.10 (`913fd39d8`).

**Suggested Fix**: derive the NPC offset from `ContactConfig::DEFAULT`, share one const for the 32/20 capsule, or add an equality pin test.

Source: docs/audits/AUDIT_PHYSICS_2026-09-21.md (PHYS-D4-2026-09-21-01)

## Completeness Checks
- [ ] **TESTS**: An equality pin test asserts the NPC locomotion constants match `ContactConfig::DEFAULT` / `CharacterController::HUMAN` / the fallback capsule spawn values, so a retune of one side fails CI instead of silently diverging
- [ ] **SIBLING**: Check for other copied-not-shared constant pairs between the player controller and NPC locomotion

