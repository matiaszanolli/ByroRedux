**HEAD**: `670f81917` · **Baseline**: `docs/audits/AUDIT_PHYSICS_2026-10-27.md` (HEAD `fa6be4a6b`) · **Audited**: Dim 1 (#6459 unlit-blended VFX collider rule), Dim 2 (#6133 held corpse reconciliation vs the physics sync), Dim 3 (#6133, #6357 npc_spawn split, #6463 FO3 XRGD pin, real-data gate), Dim 5 (#6420 `sample_baked`, #6419 shore constants) · **Unchanged since baseline (skimmed)**: Dim 4 (Character & NPC Controller: no commits to `character.rs` / `locomotion.rs` / `world/queries.rs`), Dim 6 (Queries & Diagnostics: no commits to `sync.rs` / `commands/physics.rs`), and the solver half of Dim 2 (`crates/physics/src/**` has zero commits)

# PHYSAL / Physics Audit — 2026-10-31

Suite run (`--preset comprehensive`). Read-only on code, solo (no sub-agents), no engine launch. 136 commits since the baseline; 5 touch physics scope (`16de258dd`, `2e8096dde`, `94159d26e`, `00562f873`, `010bb4314`), plus three adjacent ones read by hand (`1e2c1d30e` #6133, `4385d5fe8` #6459, `613c7f46e` #6357). `crates/physics/**` has no commits at all in this window.

## Executive Summary

| Severity | Count (new) |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 0 |

No new findings. The four 10-27 findings are all still present in code and all four issues are open; none is re-filed:

- **Existing #6487 (MEDIUM)**: `PhysicsWorld::apply_impulse` (`crates/physics/src/world/mod.rs:1185`) still writes the body's `linvel`, which rapier overwrites for a multibody link, so `PushActorAway` collapses but never pushes. Unchanged.
- **Existing #6488 (LOW)**: `byroredux/src/systems/character.rs` has no `over_bed` / `sample_baked` on the player current path; the dynamic path (`physics/src/water.rs:1244`) does. Unchanged.
- **Existing #6489 (LOW)**: `waves_require_contact_rescan` (`physics/src/water.rs:762`) still keys on `wave_amplitude` alone. Unchanged.
- **Existing #6490 (LOW)**: `ragdoll_pose.rs:578` `GameKind::Fallout3NV` branch and `docs/engine/physal.md:227` ("not a `GameKind` branch") unchanged.

**PHYSAL verdict.** `crates/physics/**` and `byroredux/src/ragdoll.rs` contain no `GameKind` / `bsver` / version branch (grep re-run). The only per-game branch near the solver is `ragdoll_pose.rs:575-578` (#6490). The three named seams (constraint CInfo decode, `havok_scale_for`, collision-object-kind dispatch) remain the only translation-time per-game branches. Collision data was traced statically only: the delta did not touch the importer, so the 10-27 traces (FO3/FNV/Skyrim bone path, FO4 decoded path, FO3/FNV XRGD) stand. The real-data gate exercised the FO3 (six environments), FO4 (six-rig free fall, XRGD ghoul), Skyrim SE (skeever) and FNV/FO3 XRGD census rigs.

**Test runs (this pass, repo `target/`, `CARGO_INCREMENTAL=0`)**:
- `cargo test -p byroredux-physics`: 270 passed.
- `cargo test -p byroredux --bin byroredux -- ragdoll character locomotion water`: 307 passed, 17 ignored (combined filter).
- **Data gate**: `-- --ignored ragdoll::installed_tests skeever_ragdoll xrgd_corpse` with `BYROREDUX_FO3_DATA`, `BYROREDUX_FO4_DATA`, `BYROREDUX_SKYRIMSE_DATA`, `BYROREDUX_FNV_DATA` set: 11 of 11 pass in 16.9 s (six FO3 environments, two FO4 rigs, skeever 600 floor ticks, FNV and FO3 XRGD corpse censuses, the latter now pinned by #6463).

## Solver Invariant Matrix

| Invariant | Verdict | Evidence |
|---|---|---|
| Fixed step + accumulator clamp | Verified (unchanged) | `world/mod.rs` has no commits. |
| Wake discipline / spawn re-sleep | Verified (unchanged) | No commits to `world/`, `sync.rs`. |
| Contact-force subscription | Verified (unchanged) | #6325 rule untouched. |
| Lock order | Not re-derived | Physics scope has no lock-touching commits. The one red lock-order lane this window (`crafting::improve_plan`, ECS-2026-10-31-01) is Inventory/ItemInstancePool, not physics. |
| Phase order (collect → kinematic → buoyancy → step → pull) | Verified (unchanged) | `boot/schedule/physics.rs` has no commits. |
| Explosion recovery | Verified (unchanged) | `recovery.rs` has no commits; FO3/FO4/skeever real-data gate green. |
| Teardown / retired rows | Verified | #6133 (1e2c1d30e) holds load-time corpse reconciliation (`PendingDeathReconciliations.held`) until `complete_reference_load` or the resident-restore sweep, then hands it to the Late sink. The `step_*` streaming steppers run between frames, outside the scheduler, so the next physics sync registers the cell's statics before the Late sink builds the ragdoll. `EntityId`s are never reused (`World::spawn` monotonic), so a held id whose entity was despawned by a cancelled load is skipped by the `Dead` lookup. Residual: a held corpse of cell B is released early when cell A's load completes first (streaming); the effect is the pre-fix behaviour for B, not a new fault. |
| Ragdoll impulse API | **Drifted** (existing #6487) | `apply_impulse` still a silent no-op on links. |
| Two water samplers agree | **Drifted** (existing #6488) | Wave height agrees (both read `sample_baked` via `authored_wave_height_over_bed`); river current does not. #6420 moved the CPU shore direction onto the mesh's border rule: `sample_baked` scales `uphill` by the border fade, `over_bed` normalises and gates on `WATER_SHORE_MIN_UPHILL_LEN`, as `water.vert` does. Verified consistent. |
| Synthesised-collider rule | Verified | #6459: `render_geometry_can_collide` now rejects `MATERIAL_KIND_NO_LIGHTING && has_alpha`. FO3/FNV `BSShaderNoLightingProperty` imports as kind 102 (`nif/src/import/material/legacy_properties.rs:851`). Opaque unlit (sign) and lit blended (glass) stay solid. Pinned by `unlit_blended_sheets_do_not_synthesize_collision` and the ignored FNV census. |

## Findings

None new.

## Known-Open Register (changes this pass)

- **Closed since baseline, verified in code**: #6133 (held corpse reconciliation), #6459 (unlit VFX sheet colliders), #6420 / #6419 (shore direction and constants), #6463 (FO3 XRGD pin).
- **Still open, unchanged**: #6487, #6488, #6489, #6490 (the four 10-27 findings), #6048 (terrain LTEX friction/restitution), #6065 (FO4 XRGD rigs past human/ghoul/super mutant). The FO4 tilted-reference corpse pose still has no issue of its own. Ball-and-socket / stiff-spring / chain constraints are still declined by design. The upstream instability of mass-inverted rigs is unchanged and the real-data gate is green (11/11).
- **Unmeasured, not filed**: whether Oblivion has STAT bases that are unlit blended VFX sheets without authored collision (Oblivion has no NoLighting material kind, so #6459's signal cannot catch them; no census was run). A finding would need that census.
- **Observation, not filed**: the debug console `entity.move` (`commands/story_actions.rs:251`, 93428cba1) accepts NaN/inf deltas and nudges `Transform` + `GlobalTransform` of any entity without re-syncing a physics body. It is a staging tool for smoke gates, not a gameplay teleport path.
- **Skill notes for the next sync**: Dim 3 should list the #6133 hold/release pair (`queue_dead_actor_reconciliation_after_cell_load`, `release_held_dead_actor_reconciliations`) among the corpse-pose ordering facts; Dim 1 should name the #6459 unlit-blended clause of `render_geometry_can_collide`; Dim 5 should name `WaterBedField::sample_baked` as the CPU sampler every shore-direction consumer reads.

Suggested next step: nothing to publish. The open PHYS issues are #6487–#6490.
