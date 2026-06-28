# FNV-D7-NEW-01: keyframed bone bodies never torn down on ragdoll activation

**Severity**: MEDIUM · **Source**: `docs/audits/AUDIT_FNV_2026-06-27.md` (FNV-D7-NEW-01)
**Location**: `byroredux/src/ragdoll.rs` (`activate_ragdoll` — no teardown) · `byroredux/src/npc_spawn.rs` (`keyframe_live_ragdoll_bones`) · `crates/physics/src/sync.rs` (`push_kinematic`, gated only on `motion_type == Keyframed`) · `crates/physics/src/ragdoll.rs` (`build_ragdoll` — no `InteractionGroups`/`solver_groups`)
**Status**: NEW (distinct from #1718 — that is a parse/resolve silent-drop; this is a runtime solver double-body conflict on bones that *did* resolve)

## Description
At NPC spawn, each ragdoll bone NiNode carrying a `bhkRigidBody` gets a `RigidBodyData`, flipped to `Keyframed` by `keyframe_live_ragdoll_bones` and registered with Rapier (`RapierHandles`). When `ragdoll <id>` runs, `activate_ragdoll` builds ~18 *new* dynamic bodies via `build_ragdoll` but does NOT remove, disable, or re-type the pre-existing keyframed bone bodies. From that frame the bone carries TWO live Rapier bodies. Each frame `ragdoll_writeback_system` writes the simulated pose onto the bone `GlobalTransform`; the next frame `push_kinematic` (gated only on `motion_type == Keyframed`, no `RagdollActive` check — `sync.rs:520`) drives the keyframed body to chase it. Because `build_ragdoll` sets no `InteractionGroups`/`solver_groups` (default = collide-with-all) the kinematic followers generate dynamic-vs-kinematic contact forces against the ragdoll bodies they are co-located with and against sibling bones — forces that fight the multibody solver the writeback then reads back.

## Evidence
`activate_ragdoll` (`ragdoll.rs:170-236`) inserts `Ragdoll` + `RagdollActive` and returns; it never calls `remove_body`/`set_body_type` on the bone's existing keyframed body (grep: zero `remove_body` in `ragdoll.rs`). `push_kinematic` (`sync.rs:520`): `if body_data.motion_type != MotionType::Keyframed { continue; }` — no exclusion of ragdolled actors. `build_ragdoll` (`ragdoll.rs:131`) builds colliders with no `.collision_groups()`/`.solver_groups()`; repo-wide grep finds zero `InteractionGroups` usage. The #1698 doc comment (`npc_spawn.rs`) asserts "Death-time ragdoll activation … rebuilds the simulated ragdoll separately" but does not address tearing down the keyframed bodies it left registered.

## Impact
Manifests on the documented `ragdoll <id>` debug/smoke path (`docs/smoke-tests/m41-ragdoll.sh`, Doc Mitchell). ~18 redundant kinematic bodies per ragdolled actor remain in the broad-phase chasing the simulation; kinematic-vs-dynamic contacts can jitter the crumple, push the ragdoll off its authored rest pose, or twitch limbs. Not a crash (the multibody joints keep the chain connected) → MEDIUM. Bounded: only actors explicitly ragdolled via the console/smoke path; the production death-trigger is not wired yet (`docs/engine/physal.md` §6).

## Related
#1698 (keyframe-bone work that introduced the kinematic bodies); `docs/engine/physal.md` §6 step 4. Distinct from #1718.

## Suggested Fix
In `activate_ragdoll`, after a successful `build_ragdoll`, tear down or kinematic-disable the bone entities' pre-existing keyframed `RigidBodyData`/`RapierHandles` bodies (`pw.remove_body` the stale handle per ragdolled bone, or skip them in `push_kinematic` when the owner is `RagdollActive`). Alternatively give the dynamic ragdoll bodies a self-collision-excluding `InteractionGroups`.

## Completeness Checks
- [ ] **SIBLING**: Check `cell_loader/unload.rs` ragdoll release and any other `RagdollActive` insertion path for the same un-torn-down keyframed body
- [ ] **LOCK_ORDER**: If teardown adds a `PhysicsWorld` resource write inside `activate_ragdoll`, preserve the two-phase collect-then-mutate lock discipline used elsewhere in `ragdoll.rs`
- [ ] **TESTS**: A regression test activates a ragdoll and asserts each ragdolled bone has exactly one live Rapier body (or that ragdoll bodies carry a self-excluding interaction group)
