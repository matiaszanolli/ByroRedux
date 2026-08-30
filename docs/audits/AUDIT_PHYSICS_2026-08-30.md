# PHYSAL / Physics Audit — 2026-08-30 (full 7-dimension pass)

**Run**: `/audit-physics --depth deep`, executed as one arm of an
`audit-suite --preset comprehensive` run. **Solo execution — no sub-agent
fan-out** (`feedback_audit_suite_nested_agent_relay`): every dimension was
read, grepped and traced directly in-process and written to
`/tmp/audit/physics/dim_N.md` before consolidation. No engine process was
launched (`feedback_no_parallel_engine_launch`). Cargo was capped at
`CARGO_BUILD_JOBS=4`, scoped to one package, and never run concurrently.

**Scratch hygiene**: `/tmp/audit/physics/` was empty at start (`mtime` checked);
nothing in this report derives from a previous run's scratch files.

**Scope**: `crates/physics/src/` (world, sync, convert, components, config,
ragdoll, water, lib) + `byroredux/src/ragdoll.rs` +
`byroredux/src/systems/character.rs` + `byroredux/src/systems/water.rs` +
`byroredux/src/commands/{physics,water,mod}.rs` + `byroredux/src/combat.rs`
(the #3119 reconciler) + `byroredux/src/cell_loader/{unload,transition}.rs` +
the parse side `crates/nif/src/import/collision/{shape,ragdoll,mod}.rs` and
`crates/nif/src/blocks/collision/constraints.rs` +
`crates/core/src/ecs/components/{collision,water,global_transform}.rs`.

**Tests**: `cargo test -p byroredux-physics` — **153 passed, 0 failed,
0 ignored** (unchanged from the 08-27b pass).
`cargo check -p byroredux-physics --tests` — **1 warning**, which is the
already-open #3494 and nothing else.

**Delta audited**: `969d81c8..HEAD` over the whole physics scope is
**+31 / −15 across 2 files** — `crates/physics/src/world.rs` (+9, the #3380
`remove_body` idempotency doc) and `crates/nif/src/import/collision/ragdoll.rs`
(+22/−15, the #3330 comment rewrite and a test-table reorder). The subsystem
is essentially unchanged since 2026-08-27b, so this pass is a line-by-line
**re-verification** rather than a delta review, plus a fresh audit of the two
areas the delta touched.

**Games traced**: the solver path is game-agnostic (doctrine re-verified below).
Shape translation was traced against the shared classic-`bhk` producer
(Oblivion / FO3 / FNV / Skyrim LE+SE) and the FO4+ `BhkNPCollisionObject`
opaque-payload census route. Ragdoll articulation was traced against the FNV
creature corpus (#3330's own union-find evidence, re-checked against HEAD's
decoder coverage). Water was traced against the FO3/FNV `XWCU` current-marker
producer and the shared `WaterFlow` / `authored_wave_height_with_weather`
contract.

---

## Executive Summary

| Dimension | CRITICAL | HIGH | MEDIUM | LOW |
|---|---|---|---|---|
| 1 — Shape Translation | 0 | 0 | 0 | 0 |
| 2 — Step Determinism & Budget | 0 | 0 | 0 | 0 |
| 3 — ECS Sync | 0 | 0 | 0 | 0 |
| 4 — Ragdoll Articulation | 0 | 0 | 1 | 0 |
| 5 — Character Controller | 0 | 0 | 0 | 0 |
| 6 — Water / Buoyancy | 0 | 0 | 0 | 1 |
| 7 — Queries & Diagnostics | 0 | 0 | 0 | 0 |
| **Total (new)** | **0** | **0** | **1** | **1** |

**Dimensions 1, 2, 3, 5 and 7 produced no findings.** Two dimensions produced
one finding each, and both are *scope-completion* findings rather than newly
discovered defects — which is the pattern worth naming in this pass:

> **This subsystem's current failure mode is partial closes, not new bugs.**
> Both findings are the same shape: an issue was closed (or, for #3490, is about
> to be fixed) covering a *subset* of the sites its own evidence named, leaving
> the residual untracked in a code comment. #3330 closed on its `bhkHinge` third
> while the Protectron `bhkPrismatic` + breakable two-thirds stayed live; #3490's
> issue body asserts the surface branch is clean when only its *vertical* half is.
> Nine consecutive dimension-level invariants re-verified clean; the risk is in
> how fixes are scoped, not in the code drifting.

### PHYSAL doctrine verdict — **HOLDS**

```
$ grep -rn "GameKind|bsver|NifVersion|game_kind|is_skyrim|is_fo4|is_oblivion|game ==|BS_F76|SF_FORM_ID" \
        crates/physics/src/ byroredux/src/ragdoll.rs byroredux/src/systems/character.rs
(no matches)
```

The constraint CInfo decode is still the **only** per-game seam. The whole
solver side — collider translation, the fixed step, the 4(+1)-phase sync, joint
construction, the writeback, the character controller and the buoyancy sink —
carries no game or version branch. The per-game branching that exists lives in
`LimitedHingeCInfo::parse_oblivion` / `parse_hinge_fo3`
(`crates/nif/src/blocks/collision/constraints.rs`), exactly where
`docs/engine/physal.md` §1 places it. `physal.md` is **not** stale on this point.

`docs/engine/watal.md`'s open-items list was re-read rather than trusted from
the skill text: water-walking and freezing are still named open there
(`watal.md:435-437`), and the physics half — buoyancy, submerged damping,
bounded current drag, character swimming, bounded drowning, splash markers,
underwater audio — is live and was audited as in-scope code, not confirmed absent.

---

## Solver Invariant Matrix

| Invariant | State | Evidence |
|---|---|---|
| Fixed step: clamp precedes the loop | **verified** | `world.rs:474-478` — `accumulator += frame_dt.max(0.0)`, then `max_acc` clamp, then the fast path, then the `while`. |
| Fixed step: NaN `frame_dt` cannot poison | **verified** | `.max(0.0)` chosen over `f32::maximum` deliberately (#2879); `non_finite_frame_dt_cannot_poison_the_accumulator`. |
| Anti-spiral budget times at least one substep | **verified** | `loop_start` before the loop, elapsed check *after* each step (`world.rs:543-585`, #1698). Drops backlog → slow-motion, never a jump. |
| Query pipeline rebuilt once per frame, outside the substep loop | **verified** | `None` passed to `pipeline.step` (`world.rs:566`); single `update` at `:632-635`. |
| Wake discipline: every motion-starting path arms `pending_wake` | **verified** | 11 production `wake()` sites enumerated in Dim 2; each conditional wake carries a comment for why the *other* direction would pin the scene. |
| `pending_wake` survives sub-tick frames | **verified** | cleared only `if steps > 0` (`world.rs:597-599`, #2856). |
| Lock ordering: no storage read survives into a resource/storage write | **verified** | Dim 3 traces all four phases; #2404, #2135, #3303, #313 splits all present. |
| Phase order collect→push→buoyancy→step→pull | **verified** | `sync.rs:126-166`; buoyancy applies forces before the step integrates them; `BYRO_PROFILE` labels match what they bracket. |
| Newcomer registration is idempotent | **verified** | gate at *collect* time (`sync.rs:849`, #2867), not at register time. |
| Teardown releases bodies, colliders, joints | **verified** | `release_victim_rapier_bodies` runs before `despawn_batch`; `remove_body` idempotent (#3380); 9 tests in `rapier_release_tests.rs`. |
| Per-game seam confined to constraint CInfo decode | **verified** | doctrine grep above. |
| Extent clamping unified across primitives | **verified** | one `clamp_shape_extent` (#3238) on all four primitive arms; no reintroduced local `.max(1e-3)`. |
| Contact skin applied at every production collider | **verified** | `sync.rs:961/981`, `ragdoll.rs:259/271`; the only other site is `#[cfg(test)]`. |
| `kcc_offset_bu > 2 × default_contact_skin_bu` | **verified** | asserted by `kcc_offset_clears_the_combined_contact_skin` (#2885). |
| dt-spike cannot tunnel the capsule | **verified** | `dt.min(MAX_FRAME_DT = 1/30)` (#2886) *and* the KCC is a swept shape cast. |
| Buoyancy never pins the static-scene fast path | **verified** | one-shot edge wakes only; `n_new > 0` escape hatch present (`sync.rs:150` → `water.rs:637-651`). |
| Water death reconciles in one place | **verified** | both producers only insert `Dead` + queue; single sink `reconcile_dead_actor` (#3119). |
| Census separates *not authored* / *dropped* / *not walkable* | **verified** | four-arm authoring split + `SpawnProbeVerdict` (#2874), reachable as `phys.census`. |

---

## Findings

### MEDIUM

#### PHYS-D4-2026-08-30-01 — `bhkPrismatic` and breakable-wrapped ragdoll edges still sever FNV articulation; #3330 was closed on its hinge third only

- **Dimension**: Ragdoll Articulation
- **Files**: `crates/nif/src/import/collision/ragdoll.rs:142-155` (the
  `BhkBreakableConstraint` arm) and `:193-215` (the `BhkConstraintData::Other`
  arm); `crates/nif/src/blocks/collision/constraints.rs`
  (`BhkConstraint::parse`).
- **Premise verified at HEAD** — not inherited from the closed issue.
  `BhkConstraint::parse` decodes only `Ragdoll` and `LimitedHinge` (plus the
  malleable wrapper's inner 7/2 and, since #3330, `parse_hinge_fo3`).
  `bhkPrismaticConstraint`, `bhkBallAndSocketConstraint` and
  `bhkStiffSpringConstraint` still arrive as `BhkConstraintData::Other` and are
  dropped with a `warn!`. `BhkBreakableConstraint` still fails the
  `downcast_ref::<BhkConstraint>()` and is dropped with its own `warn!`, because
  its wrapped CInfo geometry is `stream.skip`ped at parse time. The #3330 fix
  commit (`1ccf1abe`) rewrote the comment to say so itself:
  > *"What remains reaching here on vanilla FNV is `creatures\protectron\skeleton.nif`'s
  > two `bhkPrismaticConstraint` edges, which need a canonical prismatic joint
  > kind that does not exist yet."*
- **Impact**: #3330's own corpus evidence named three fragmenting FNV creature
  skeletons and attributed them precisely — `sentryturret` and
  `minisentryturret` to `bhkHingeConstraint` (**fixed**), and `protectron` to
  `2× bhkPrismatic + 1× breakable` (**not fixed**: 12 authored edges → 9
  surfaced, 4 connected components, with `Bip01 Head`, `Bip01 Head Dome` and
  `Bip01 Spine Brain` each becoming an independent free-falling multibody).
  `build_ragdoll`'s forest `warn!` (`crates/physics/src/ragdoll.rs:290-302`)
  fires, so it is diagnosable from the log — but the visual break is unchanged.
  Per `_audit-severity.md`, a translatable block dropped at the canonical
  boundary is MEDIUM minimum.
- **Trigger Conditions**: any FNV/FO3 cell containing a Protectron whose ragdoll
  activates (death, or the `ragdoll` console command). Content:
  `creatures\protectron\skeleton.nif` from `Fallout - Meshes.bsa`.
- **Tracking gap**: #3330, #1539 and #1850 are **all CLOSED**, and #3330 was
  closed on a partial fix with no successor. The residual is tracked only by a
  source comment.
- **Recommendation**: reopen #3330 or file a successor carrying its evidence
  forward. Two distinct pieces of work remain: (a) a canonical prismatic joint
  kind (`ImportedJointKind::Prismatic` → `RagdollJointSpec::Prismatic` →
  a `GenericJoint` leaving the authored linear axis free), and (b) retaining
  `BhkBreakableConstraint`'s wrapped CInfo at parse so the inner
  Ragdoll/LimitedHinge joint can be rebuilt — #1850's own deferred note.

### LOW

#### PHYS-D6-2026-08-30-01 — the surface branch's XZ containment also reads the body origin, so the #3490 fix as currently scoped will land half-done

- **Dimension**: Water / Buoyancy
- **File**: `crates/physics/src/water.rs:781-785` (the `surface` search's
  `filter_map` predicate), read against `:735-748` (the current branch, = #3490)
  and `:764-774` (the #2887 rationale comment).
- **Premise verified at HEAD**: the surface search's containment predicate is
  ```rust
  pos.x >= v.min[0] && pos.x <= v.max[0]
      && pos.z >= v.min[2] && pos.z <= v.max[2]
      && max_y >= v.min[1]
  ```
  `min_y` / `max_y` come from `collider.compute_aabb()`, but **`pos.x` / `pos.z`
  are still the rigid-body origin** — as is the union-footprint prefilter
  (`pos.x < ux0 || …`, `:735`). #2887 moved the vertical metric and `depth` onto
  the AABB centre and left the horizontal pair behind.
- **Why this matters now**: #3490's issue body asserts that *"the surface test
  immediately below deliberately does **not** do this."* That is true only of the
  Y axis. Whoever fixes #3490 by moving the current branch onto the AABB centre
  will read the surface branch as already-correct and leave its XZ on the origin
  — the same partial close that just happened to #3330 above.
- **Impact**: bounded and small. The discrepancy is the compound/bone XZ offset
  (tens of BU) against a water plane's XZ extent (typically thousands), so it can
  only mis-classify a dynamic compound body sitting within its own collider
  offset of a shore edge — it floats when it should be dry, or vice versa.
  The reason to file is the scope-completion risk on #3490, not the impact.
- **Trigger Conditions**: a Dynamic `bhk` body whose shape is a `Compound` /
  `List` with a part offset in XZ from the body origin
  (`collision_shape_to_parts` attaches each part at its own local isometry and
  nothing re-centres the body), positioned within that offset of a
  `WaterVolume`'s XZ boundary.
- **Recommendation**: fold into #3490 rather than tracking separately — derive
  one `reference_point` from `collider.compute_aabb().center()` once at the top
  of the per-body loop and use it for the union prefilter, the current-volume
  containment, and the surface XZ/Y containment alike.

---

## Stale candidates dropped

Four candidates were investigated and **dropped after checking their premise
against HEAD**:

1. **`plane_min` / `plane_max` ragdoll swing limits dropped at
   `joint_from_imported`.** Real, but already documented in-code
   (`byroredux/src/ragdoll.rs:240-251`), in `docs/engine/physal.md:185-190`
   § *Known approximation*, and closed as #1982. Not a finding.
2. **`ContactConfig::DEFAULT` hard-coded at a collider-creation site**
   (`world.rs:2232`) instead of the live resource. That site is inside
   `#[cfg(test)] mod audit_2026_08_13_regressions`. Not production.
3. **`bhkConvexTransformShape` missing a `resolve_shape` arm.** The
   dispatch-vs-resolve diff shows 19 vs 18, but `blocks/mod.rs:1208` maps
   `"bhkTransformShape" | "bhkConvexTransformShape"` onto one struct. Parity
   holds.
4. **Fast-path cost comment doc rot** (the "~8-10 ms × 5 substeps" figure the
   skill flags as suspect). #2890 already replaced it with a measured
   attribution and a stated caveat, and
   `step_cost_rationale_is_scoped_to_history_and_names_the_real_cost_centre`
   pins it. No rot.

---

## Known-Open Register

### The three don't-re-litigate items

| Item | State after this pass |
|---|---|
| **`tes_grounding_zero_mass_dynamic_fix`** — Skyrim architecture ships mass=0 Dynamic-family Havok bodies, reclassified Static (19 → 416 colliders, #1832 / `ae083d69`) | Verified present. **Not re-investigated.** The door-threshold spawn gap stays open and unchanged; Dim 5 confirms the *controller-side* mechanism is intact end-to-end (walkable-slope classification shared between probe and controller, self-exclusion on every cast, `cast_capsule_down_surface_and_normal` existing precisely to separate "hit nothing" from "hit something too steep"), so what remains open is upstream — whether a walkable collider exists at the threshold at all, which `phys.census` is the diagnostic for. |
| **`interior_spawn_point_fix`** — interiors spawn at the first door's own placement; vanilla `coc` has no auto spawn-point logic | Unchanged. No assumption of one was introduced anywhere in this pass. |
| **`fnv_furniture_sit_needs_transition`** — `dynamicidle_*` sit loops have no pelvis/root channel; M42 seat-snap gated behind `BYRO_SANDBOX_SIT` | Unchanged and out of this subsystem's path. |

### Physics/water issues already open — verified still true at HEAD, deliberately not re-filed

| # | Sev | State |
|---|---|---|
| **#3490** | MEDIUM | Still true. The current-volume containment test reads `pos` (rigid-body origin) on all three axes at `water.rs:735-748`. See PHYS-D6-2026-08-30-01 for a scope extension. |
| **#3492** | MEDIUM | Still true. The buoyancy target set is `RapierHandles × RigidBodyData == Dynamic` (`water.rs:683-705`); `activate_ragdoll` removes **both** components from every ragdoll bone (`byroredux/src/ragdoll.rs:429-437`, pinned by `activation_tears_down_keyframed_bone_bodies`). Ragdolls remain structurally invisible to the sink. |
| **#3494** | LOW | Still true, and it is the sole `cargo check` warning. `water.rs:1898` and `:1911` both attach to `current_volume_without_a_water_plane_wakes_a_body_resting_in_it`; the #3114 rationale is stranded above the first, and `current_volume_without_a_water_plane_does_not_wind_up_user_force` (`:2034`) still has no doc. |
| **#3495** | LOW | Still true — `byroredux/src/commands/physics.rs` is absent from CLAUDE.md's workspace tree. **Scope note**: the gap is wider than the issue states. CLAUDE.md lists 8 of the 15 files in `byroredux/src/commands/`; also missing are `water.rs` (in this audit's declared scope), `depth.rs`, `env_health.rs`, `gameplay.rs`, `quest.rs` and `time.rs`. Widen #3495 rather than filing siblings. |
| **#3477** | LOW | Out of this pass's finding set (owned by `/audit-performance`); `collect_newcomers` still rescans every collider row per tick to answer "nothing new". |

### Referenced, not owned by this report

- The `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bins` failure (5
  `ragdoll::tests::*` failures from `combat_approach_line_of_sight_reaches` at
  `byroredux/src/commands/view.rs:175-215` closing
  `PhysicsWorld → RapierHandles → GlobalTransform → PhysicsWorld`) was reproduced
  by a sibling agent this run and belongs to the **concurrency** report. The
  physics-crate lock discipline itself (Dim 3) is clean: the cycle is closed by a
  `byroredux`-side console command, not by `physics_sync_system`.

---

## Cross-Audit Deduplication

| Topic | Owner |
|---|---|
| ECS lock ordering / the `view.rs` ragdoll-test cycle | `/audit-concurrency` Dim 5 |
| `unsafe` blocks | `/audit-safety` (this subsystem contributes none) |
| Water rendering, wave shader, reflection tint | `/audit-renderer` Dim 15 |
| `bhk*` block parsing, stream position, dispatch coverage | `/audit-nif` Dim 5 |
| `Imported*` → `CollisionShape` canonical translation | `/audit-nifal` Dim 6 |
| `XCLW` tri-state / WATR decode | `/audit-esm` Dim 5 |
| `collect_newcomers` per-tick rescan (#3477) | `/audit-performance` Dim 1 |
| CLAUDE.md workspace-tree drift (#3495 and its 6 siblings) | `/audit-tech-debt` Dim 7 |

---

## Publish

```
/audit-publish docs/audits/AUDIT_PHYSICS_2026-08-30.md
```

Domain label: `physics`. Add `water` to PHYS-D6-2026-08-30-01, and
`game:fnv` to PHYS-D4-2026-08-30-01 (the Protectron content is FNV/FO3-specific
Havok authoring).
