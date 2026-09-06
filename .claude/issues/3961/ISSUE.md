# PHYS-D3-2026-09-06-02: the scale contract is stated **backwards** on the rustdoc of the exact function `register_newcomers` calls, and in the doc that rustdoc cites

Issue: #3961 · Filed from `docs/audits/AUDIT_PHYSICS_2026-09-06.md` (base `229306ce`)

Reported by `/audit-physics` (full 7-dimension pass) — `docs/audits/AUDIT_PHYSICS_2026-09-06.md`, base `229306ce`.

Every claim below was independently re-derived from the code by the audit orchestrator before filing, not accepted from the dimension agent.


- **Severity**: MEDIUM
- **Dimension**: ECS Sync
- **Location**: `crates/physics/src/convert.rs:138-141` · `docs/engine/physics.md:424-428`
- **Status**: NEW
- **Trigger Conditions**: static. It misfires the moment anyone reasons about who applies scale at the `sync.rs:907` boundary — which is exactly what fixing PHYS-D1-2026-09-06-01 requires.
- **Description**: `collision_shape_to_parts` is the sink `register_newcomers` hands `n.global.scale` to. Its own rustdoc carries a census of which producers pre-bake scale, and **both entries are wrong**: it says `synthesize_static_trimesh` multiplies every vertex by `world_scale` (it has taken no scale parameter since `b8c4e6af`, and its own header says the opposite) and that `spawn_packed_havok_proxy` passing `ref_scale` through is *consistency with the design*. `docs/engine/physics.md:424-428` repeats the error and generalises it into a false statement about this dimension: *"physics consumes position and rotation, not `GlobalTransform::scale`"* — untrue since #2860.
- **Evidence**: `git show b8c4e6af --stat` touches `spawn.rs` (removed the pre-bake), `convert.rs` (edited ~100 lines below the stale doc) and `docs/engine/physal.md` — where it **added the correct contract**: *"Producers must not bake that scale into vertices or primitive dimensions; doing both produces scale² geometry"* (`physal.md:105-110`). The commit names, in its own evidence, the exact failure mode D1 found still live — and left both contradicting statements standing in the two places a reader of `collision_shape_to_parts` would actually look.
- **Impact**: three statements of one contract, two inverted, and the correct one is in the file least likely to be open while editing `sync.rs:907`. A maintainer fixing D1-01 from `convert.rs:138-141` would conclude that pre-baking *is* the intended contract for the proxy (leaving the real bug alone) and that `spawn_trimesh_collider_ghost` is the one out of line — "fixing" a currently-correct producer. Not cosmetic: it is the contract statement a HIGH-severity fix will be checked against.
- **Related**: #3064, #3065, #2860, **PHYS-D1-2026-09-06-01 (land together)**, `docs/engine/physal.md:105-110` (the correct wording to propagate).
- **Suggested Fix**: replace `convert.rs:138-141` with the `physal.md` wording — "every producer keeps the shape in local units; this function applies `GlobalTransform::scale` exactly once" — and **delete the per-producer census from the rustdoc** rather than re-listing it (a list of producers on the consumer is the thing that rots). Correct `docs/engine/physics.md:424-428` the same way, in the same commit as the D1 fix.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — this audit's headline result is that *every* recent physics fix closed on fewer sites than its own evidence named; enumerate the full defect class before closing
- [ ] **LOCK_ORDER**: If a `RwLock` scope changes, the canonical acquisition order (`docs/engine/ecs.md`) is preserved and `PhysicsWorld` stays a sink
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser→canonical boundary; the solver side carries no `GameKind`/`bsver` branch (PHYSAL doctrine, `docs/engine/physal.md` §1)
- [ ] **TESTS**: A regression test pins this specific fix
